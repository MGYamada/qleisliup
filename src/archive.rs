//! Extract only a verified tar.zst into a private transaction directory.
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::files;

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) expanded: u64,
    pub(crate) file: u64,
    pub(crate) entries: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            expanded: 2 * 1024 * 1024 * 1024,
            file: 512 * 1024 * 1024,
            entries: 100_000,
        }
    }
}

struct Budget<R> {
    inner: R,
    remaining: u64,
}
impl<R: Read> Read for Budget<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        // Read one extra byte to distinguish exact EOF from an exceeded limit.
        let limit = bytes.len().min(self.remaining.saturating_add(1) as usize);
        let count = self.inner.read(&mut bytes[..limit])?;
        if count as u64 > self.remaining {
            return Err(io::Error::other("archive expanded byte limit exceeded"));
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

pub(crate) fn extract(
    verified: &Path,
    payload: &Path,
    root_name: &str,
    limits: Limits,
) -> Result<PathBuf> {
    let file = File::open(verified).map_err(|e| Error::file(verified, e))?;
    let mut decoder = zstd::stream::read::Decoder::new(file).map_err(archive_error)?;
    decoder.window_log_max(27).map_err(archive_error)?;
    let budget = Budget {
        inner: decoder,
        remaining: limits.expanded,
    };
    let mut archive = tar::Archive::new(budget);
    let mut paths = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    for entry in archive.entries().map_err(archive_error)?.raw(true) {
        let mut entry = entry.map_err(archive_error)?;
        if paths.len() >= limits.entries {
            return Err(archive_error("archive entry limit exceeded"));
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err(archive_error(
                "archive links, extensions, and special files are forbidden",
            ));
        }
        let raw = entry.path_bytes();
        let text = std::str::from_utf8(&raw).map_err(archive_error)?;
        let text = if kind.is_dir() {
            text.strip_suffix('/').unwrap_or(text)
        } else {
            text
        };
        if text.len() > 4096
            || text.split('/').count() > 64
            || text
                .split('/')
                .any(|p| p.is_empty() || matches!(p, "." | ".."))
        {
            return Err(archive_error(
                "archive path is not a confined canonical path",
            ));
        }
        let relative = crate::state::relative_path(text)?;
        if text.split('/').next() != Some(root_name)
            || relative
                .file_name()
                .is_some_and(|n| n == crate::toolchain::RECEIPT)
        {
            return Err(archive_error(
                "archive path is outside its release root or supplies an installer receipt",
            ));
        }
        if !paths.insert(text.to_owned()) {
            return Err(archive_error("duplicate archive path"));
        }
        let mode = entry.header().mode().map_err(archive_error)?;
        if mode & !0o777 != 0 {
            return Err(archive_error(
                "privileged or unsupported archive permissions",
            ));
        }
        let size = entry.header().size().map_err(archive_error)?;
        if size > limits.file || (kind.is_dir() && size != 0) {
            return Err(archive_error(
                "archive entry size limit or directory payload violation",
            ));
        }
        let destination = payload.join(relative);
        if kind.is_dir() {
            make_directories(payload, relative, &mut nodes, limits.entries)?;
        } else {
            let parent = relative
                .parent()
                .ok_or_else(|| archive_error("archive file has no parent"))?;
            make_directories(payload, parent, &mut nodes, limits.entries)?;
            nodes.insert(relative.to_owned());
            if nodes.len() > limits.entries {
                return Err(archive_error("archive filesystem node limit exceeded"));
            }
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&destination)
                .map_err(|e| Error::file(&destination, e))?;
            let copied = io::copy(&mut entry, &mut file).map_err(archive_error)?;
            if copied != size {
                return Err(archive_error("truncated archive entry"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(mode))
                    .map_err(|e| Error::file(&destination, e))?;
            }
            file.sync_all().map_err(|e| Error::file(&destination, e))?;
        }
    }
    // tar stops at its EOF marker. Drain zstd to its own verified end and reject
    // a second hidden archive or nonzero trailing data, still under the budget.
    let mut remaining = archive.into_inner();
    let mut bytes = [0_u8; 8192];
    loop {
        let count = remaining.read(&mut bytes).map_err(archive_error)?;
        if count == 0 {
            break;
        }
        if bytes[..count].iter().any(|b| *b != 0) {
            return Err(archive_error("nonzero data follows the tar archive"));
        }
    }
    let root = payload.join(root_name);
    files::directory(&root)?;
    // Let this filesystem detect aliases of the reserved destination. A string
    // comparison alone misses case folding or other filename normalization.
    let receipt = root.join(crate::toolchain::RECEIPT);
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let reservation = options.open(&receipt).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            archive_error("archive supplies an installer receipt or a filesystem alias")
        } else {
            Error::file(&receipt, error)
        }
    })?;
    drop(reservation);
    fs::remove_file(&receipt).map_err(|error| Error::file(&receipt, error))?;
    sync_tree(&root)?;
    Ok(root)
}

fn make_directories(
    base: &Path,
    relative: &Path,
    nodes: &mut BTreeSet<PathBuf>,
    limit: usize,
) -> Result<()> {
    let mut path = PathBuf::new();
    for component in relative.components() {
        path.push(component);
        if nodes.insert(path.clone()) {
            if nodes.len() > limit {
                return Err(archive_error("archive filesystem node limit exceeded"));
            }
            let destination = base.join(&path);
            // This private tree contains only previously recorded nodes. A new
            // spelling that already exists is a filesystem alias (case folding
            // or Unicode normalization), and must not merge archive directories.
            fs::create_dir(&destination).map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    archive_error(format!(
                        "duplicate or aliased archive directory: {}",
                        destination.display()
                    ))
                } else {
                    Error::file(&destination, error)
                }
            })?;
        } else {
            files::directory(&base.join(&path))?;
        }
    }
    Ok(())
}

pub(crate) fn sync_tree(root: &Path) -> Result<()> {
    files::directory(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::file(root, e))?;
    }
    for entry in fs::read_dir(root).map_err(|e| Error::file(root, e))? {
        let path = entry.map_err(|e| Error::file(root, e))?.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| Error::file(&path, e))?;
        if metadata.is_dir() {
            sync_tree(&path)?;
        } else {
            files::regular(&path, false)?;
            File::open(&path)
                .and_then(|f| f.sync_all())
                .map_err(|e| Error::file(&path, e))?;
        }
    }
    files::sync_directory(root)
}
fn archive_error(error: impl std::fmt::Display) -> Error {
    Error::operational(format!("archive rejected: {error}"))
}
