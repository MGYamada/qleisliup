use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use crate::error::{Error, Result};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;

// This namespace is shared by staging creation and stale-tree reclamation.
pub(crate) fn generated_name(name: &[u8], prefix: &[u8]) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|suffix| suffix.len() == 6 && suffix.iter().all(u8::is_ascii_alphanumeric))
}

fn private_directory_builder() -> fs::DirBuilder {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
}

// Exclusive creation is also needed while extracting archives: accepting an
// existing directory there would merge filesystem aliases of distinct names.
pub(crate) fn create_private_directory(path: &Path) -> std::io::Result<()> {
    private_directory_builder().create(path)
}

pub(crate) fn temporary_directory(parent: &Path, prefix: &str) -> Result<tempfile::TempDir> {
    managed_directory(parent)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix).rand_bytes(6);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder
        .tempdir_in(parent)
        .map_err(|e| Error::file(parent, e))
}

pub(crate) fn create_directory(path: &Path) -> Result<()> {
    match create_private_directory(path) {
        Ok(()) => {
            sync_directory(path)?;
            sync_parent(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => managed_directory(path),
        Err(error) => Err(Error::file(path, error)),
    }
}

pub(crate) fn create_home(path: &Path) -> Result<()> {
    // Remember every newly created ancestor. Sync child inodes and their parent
    // entries so a durable state file cannot survive without its home directory.
    let missing: Vec<_> = path
        .ancestors()
        .take_while(|ancestor| {
            fs::symlink_metadata(ancestor).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        })
        .collect();
    private_directory_builder()
        .recursive(true)
        .create(path)
        .map_err(|e| Error::file(path, e))?;
    managed_directory(path)?;
    for created in missing {
        sync_directory(created)?;
        sync_parent(created)?;
    }
    Ok(())
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::file(path, "directory has no parent"))?;
    // An explicitly chosen home can have a symlink ancestor (e.g. /tmp on macOS).
    // Its final directory still must be real; sync the actual parent inode.
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|e| Error::file(parent, e))
}

pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    directory(path)?;
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| Error::file(path, error))
}

pub(crate) fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    let file = match open_read(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::file(path, error)),
    };
    let metadata = file.metadata().map_err(|error| Error::file(path, error))?;
    if !metadata.is_file() {
        return Err(Error::file(
            path,
            "expected a regular file, not a link or special file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::file(path, error))?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(Error::file(path, "metadata exceeds the 1 MiB limit"));
    }
    Ok(Some(bytes))
}

#[cfg(unix)]
pub(crate) fn open_read(path: &Path) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, open};
    open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(Into::into)
}

#[cfg(not(unix))]
pub(crate) fn open_read(path: &Path) -> std::io::Result<File> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("symlink metadata is not supported"));
    }
    File::open(path)
}

pub(crate) fn directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| Error::file(path, error))?;
    if !metadata.is_dir() {
        return Err(Error::file(
            path,
            "expected a real directory, not a symlink",
        ));
    }
    Ok(())
}

pub(crate) fn managed_directory(path: &Path) -> Result<()> {
    directory(path)?;
    #[cfg(unix)]
    {
        let file = open_directory(path)?;
        check_managed_directory(&file, path)
    }
    #[cfg(not(unix))]
    Ok(())
}

#[cfg(unix)]
pub(crate) fn check_managed_directory(file: &File, path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = file.metadata().map_err(|e| Error::file(path, e))?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o022 != 0 {
        return Err(Error::file(
            path,
            "managed directory must be real and must not be group- or world-writable; repair its permissions before retrying",
        ));
    }
    Ok(())
}

pub(crate) fn regular(path: &Path, executable: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| Error::file(path, error))?;
    if !metadata.is_file() {
        return Err(Error::file(path, "expected a regular file, not a symlink"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if executable && metadata.permissions().mode() & 0o111 == 0 {
            return Err(Error::file(path, "required tool is not executable"));
        }
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

pub(crate) fn not_manager(path: &Path) -> Result<()> {
    let manager = std::env::current_exe().map_err(|error| {
        Error::operational(format!("cannot identify manager executable: {error}"))
    })?;
    #[cfg(unix)]
    let same = {
        use std::os::unix::fs::MetadataExt;
        let target = fs::metadata(path).map_err(|error| Error::file(path, error))?;
        let current = fs::metadata(&manager).map_err(|error| Error::file(&manager, error))?;
        target.dev() == current.dev() && target.ino() == current.ino()
    };
    #[cfg(not(unix))]
    let same = path
        .canonicalize()
        .map_err(|error| Error::file(path, error))?
        == manager;
    if same {
        return Err(Error::file(
            path,
            "refusing recursive dispatch to the manager",
        ));
    }
    Ok(())
}

// Holding the file owns the process-scoped advisory lock; closing it releases it.
pub(crate) struct Lock {
    _file: File,
}

impl Lock {
    #[cfg(unix)]
    pub(crate) fn home(path: &Path) -> Result<Self> {
        use rustix::fs::{FlockOperation, Mode, OFlags, flock, open};
        managed_directory(path)?;
        let lock_path = path.join(".mutation-lock");
        let file = File::from(
            open(
                &lock_path,
                OFlags::RDWR
                    | OFlags::CREATE
                    | OFlags::NOFOLLOW
                    | OFlags::CLOEXEC
                    | OFlags::NONBLOCK,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|error| Error::file(&lock_path, error))?,
        );
        if !file
            .metadata()
            .map_err(|error| Error::file(&lock_path, error))?
            .is_file()
        {
            return Err(Error::file(&lock_path, "lock must be a regular file"));
        }
        flock(&file, FlockOperation::LockExclusive)
            .map_err(|error| Error::file(&lock_path, error))?;
        Ok(Self { _file: file })
    }

    #[cfg(unix)]
    pub(crate) fn directory(path: &Path) -> Result<Self> {
        use rustix::fs::{FlockOperation, flock};
        let file = open_directory(path)?;
        flock(&file, FlockOperation::LockExclusive).map_err(|error| Error::file(path, error))?;
        Ok(Self { _file: file })
    }

    #[cfg(not(unix))]
    pub(crate) fn home(path: &Path) -> Result<Self> {
        Err(Error::file(
            path,
            "state mutation requires a supported Unix host",
        ))
    }

    #[cfg(not(unix))]
    pub(crate) fn directory(path: &Path) -> Result<Self> {
        Self::home(path)
    }
}

#[cfg(unix)]
pub(crate) fn open_directory(path: &Path) -> Result<File> {
    use rustix::fs::{Mode, OFlags, open};
    open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(|error| Error::file(path, error))
}

#[cfg(unix)]
pub(crate) fn replace(directory_path: &Path, name: &str, bytes: &[u8], mode: u32) -> Result<()> {
    replace_with(directory_path, name, bytes, mode, true, || {})
}

// A staged bundle may contain authenticated files with temporary-looking names.
// Its receipt must preserve those bytes; abandoned staging is reclaimed as a
// whole transaction tree, never by scanning the bundle as a state directory.
pub(crate) fn replace_staged(path: &Path, name: &str, bytes: &[u8], mode: u32) -> Result<()> {
    #[cfg(unix)]
    return replace_with(path, name, bytes, mode, false, || {});
    #[cfg(not(unix))]
    replace(path, name, bytes, mode)
}

// Callers hold the home mutation lock and, for a project pin, its directory
// lock. Private transaction directories are protected by the same home lock.
#[cfg(unix)]
fn replace_with(
    directory_path: &Path,
    name: &str,
    bytes: &[u8],
    mode: u32,
    reclaim: bool,
    before_rename: impl FnOnce(),
) -> Result<()> {
    use rustix::fs::{AtFlags, Mode, OFlags, openat, renameat, unlinkat};
    let directory = open_directory(directory_path)?;
    let destination = directory_path.join(name);
    // Reject special/symlink destinations immediately before replacing the name.
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(Error::file(
                &destination,
                "refusing to replace a symlink or special file",
            ));
        }
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(Error::file(&destination, error)),
    }
    if reclaim {
        reclaim_state_temporaries(&directory, directory_path)?;
    }
    // tempfile supplies randomized names and retries exclusive-create collisions.
    // Creation, rename, and cleanup remain relative to the opened directory.
    let temporary = tempfile::Builder::new()
        .prefix(".qleisliup-state-")
        .rand_bytes(16)
        .suffix(".tmp")
        .disable_cleanup(true)
        .make_in(directory_path, |path| {
            openat(
                &directory,
                path.file_name().expect("temporary file has a name"),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(mode as _),
            )
            .map(File::from)
            .map_err(std::io::Error::from)
        })
        .map_err(|error| Error::file(&destination, error))?;
    let name_temporary = temporary
        .path()
        .file_name()
        .expect("temporary file has a name")
        .to_owned();
    let (mut file, _path) = temporary.into_parts();
    let result = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| Error::file(&destination, error))?;
        before_rename();
        renameat(&directory, &name_temporary, &directory, name)
            .map_err(|error| Error::file(&destination, error))?;
        directory.sync_all().map_err(|error| {
            Error::file(
                &destination,
                format!("file was replaced but directory synchronization failed: {error}"),
            )
        })
    })();
    if result.is_err() {
        let _ = unlinkat(&directory, &name_temporary, AtFlags::empty());
    }
    result
}

#[cfg(unix)]
fn state_temporary(name: &[u8]) -> bool {
    let Some(stem) = name.strip_suffix(b".tmp") else {
        return false;
    };
    if let Some(random) = stem.strip_prefix(b".qleisliup-state-") {
        return random.len() == 16 && random.iter().all(u8::is_ascii_alphanumeric);
    }
    // Reclaim the exact pre-0.1.2 PID/counter namespace as well, without
    // interpreting arbitrary dotfiles or noncanonical decimal spellings.
    let Some(legacy) = stem
        .strip_prefix(b".qleisliup-")
        .and_then(|n| std::str::from_utf8(n).ok())
    else {
        return false;
    };
    let Some((pid, counter)) = legacy.split_once('-') else {
        return false;
    };
    pid.parse::<u32>()
        .is_ok_and(|n| n != 0 && n.to_string() == pid)
        && counter
            .parse::<u64>()
            .is_ok_and(|n| n.to_string() == counter)
}

// Requires the caller's mutation lock. Inspect a bounded directory before any
// deletion; never follow links, recurse, or remove committed state filenames.
#[cfg(unix)]
pub(crate) fn reclaim_state_temporaries(directory: &File, path: &Path) -> Result<()> {
    use rustix::fs::{AtFlags, Dir, FileType, statat, unlinkat};
    let mut names = Vec::new();
    for (count, entry) in Dir::read_from(directory)
        .map_err(|e| Error::file(path, e))?
        .enumerate()
    {
        if count >= 4096 {
            return Err(Error::file(
                path,
                "state temporary cleanup parent entry limit exceeded",
            ));
        }
        let entry = entry.map_err(|e| Error::file(path, e))?;
        if state_temporary(entry.file_name().to_bytes()) {
            names.push(entry.file_name().to_owned());
        }
    }
    names.sort();
    names.truncate(32);
    for name in &names {
        let stat =
            statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|e| Error::file(path, e))?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(Error::file(
                path,
                "state temporary cleanup rejects symlinks and special files",
            ));
        }
    }
    for name in &names {
        unlinkat(directory, name, AtFlags::empty()).map_err(|e| Error::file(path, e))?;
    }
    if !names.is_empty() {
        directory.sync_all().map_err(|e| Error::file(path, e))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn replace(path: &Path, _name: &str, _bytes: &[u8], _mode: u32) -> Result<()> {
    Err(Error::file(
        path,
        "atomic state mutation requires a supported Unix host",
    ))
}

#[cfg(all(test, unix))]
mod tests;
