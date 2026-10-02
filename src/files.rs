use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn create_directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {
            sync_directory(path)?;
            sync_parent(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => directory(path),
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
    fs::create_dir_all(path).map_err(|e| Error::file(path, e))?;
    directory(path)?;
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
        directory(path)?;
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
fn open_directory(path: &Path) -> Result<File> {
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
    let temporary = format!(
        ".qleisliup-{}-{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = File::from(
        openat(
            &directory,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(mode as _),
        )
        .map_err(|error| Error::file(&destination, error))?,
    );
    let result = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| Error::file(&destination, error))?;
        renameat(&directory, temporary.as_str(), &directory, name)
            .map_err(|error| Error::file(&destination, error))?;
        directory.sync_all().map_err(|error| {
            Error::file(
                &destination,
                format!("file was replaced but directory synchronization failed: {error}"),
            )
        })
    })();
    if result.is_err() {
        let _ = unlinkat(&directory, temporary.as_str(), AtFlags::empty());
    }
    result
}

#[cfg(not(unix))]
pub(crate) fn replace(path: &Path, _name: &str, _bytes: &[u8], _mode: u32) -> Result<()> {
    Err(Error::file(
        path,
        "atomic state mutation requires a supported Unix host",
    ))
}
