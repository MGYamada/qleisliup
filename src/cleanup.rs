//! Reclaim only reserved private transaction trees while a lifecycle owns the lock.
use crate::error::Result;
use crate::{files, state::Home};

#[cfg(unix)]
mod unix {
    use std::ffi::CStr;
    use std::fs::File;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};

    use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, openat, readlinkat, statat, unlinkat};

    use super::*;
    use crate::error::Error;

    const MAX_PARENT_ENTRIES: usize = 4096;
    const MAX_TREES: usize = 32;
    const MAX_NODES: usize = 200_000;
    const MAX_DEPTH: usize = 128;

    struct Directory {
        file: File,
        path: PathBuf,
    }

    impl Directory {
        fn child(&self, name: &CStr) -> Result<Option<Self>> {
            let path = self.path.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
            match openat(
                &self.file,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => {
                    let file = File::from(fd);
                    files::check_managed_directory(&file, &path)?;
                    Ok(Some(Self { file, path }))
                }
                Err(rustix::io::Errno::NOENT) => Ok(None),
                Err(e) => Err(Error::file(
                    &path,
                    format!("stale cleanup requires a real directory: {e}"),
                )),
            }
        }

        fn identity(&self) -> Result<(u64, u64)> {
            let metadata = self
                .file
                .metadata()
                .map_err(|e| Error::file(&self.path, e))?;
            Ok((metadata.dev(), metadata.ino()))
        }

        fn entries(&self) -> Result<Dir> {
            Dir::read_from(&self.file).map_err(|e| Error::file(&self.path, e))
        }
    }

    fn reclaim(
        parent: &Directory,
        prefixes: &[&[u8]],
        keep: Option<(u64, u64)>,
        proxies: bool,
        remaining: &mut usize,
    ) -> Result<()> {
        let mut names = Vec::new();
        for (count, entry) in parent.entries()?.enumerate() {
            if count >= MAX_PARENT_ENTRIES {
                return Err(Error::file(
                    &parent.path,
                    "stale cleanup parent entry limit exceeded",
                ));
            }
            let entry = entry.map_err(|e| Error::file(&parent.path, e))?;
            let name = entry.file_name();
            if prefixes
                .iter()
                .any(|prefix| files::generated_name(name.to_bytes(), prefix))
            {
                names.push(name.to_owned());
            }
        }
        names.sort();
        for name in names {
            if *remaining == 0 {
                break;
            }
            let directory = parent
                .child(&name)?
                .ok_or_else(|| Error::file(&parent.path, "stale directory disappeared"))?;
            let identity = directory.identity()?;
            // Compare identities, not spelling: current.json may use a case alias.
            if Some(identity) == keep {
                continue;
            }
            if identity.0 != parent.identity()?.0 {
                return Err(Error::file(
                    &directory.path,
                    "stale cleanup refuses a filesystem boundary",
                ));
            }
            let mut scan_budget = MAX_NODES;
            visit(
                &directory,
                Path::new(""),
                identity.0,
                proxies,
                false,
                &mut scan_budget,
                0,
            )?;
            let mut removal_budget = MAX_NODES;
            visit(
                &directory,
                Path::new(""),
                identity.0,
                proxies,
                true,
                &mut removal_budget,
                0,
            )?;
            // NOFOLLOW opens and descriptor-relative deletion never traverse links.
            unlinkat(&parent.file, &name, AtFlags::REMOVEDIR)
                .map_err(|e| Error::file(&directory.path, e))?;
            parent
                .file
                .sync_all()
                .map_err(|e| Error::file(&parent.path, e))?;
            *remaining -= 1;
        }
        Ok(())
    }

    fn visit(
        directory: &Directory,
        relative: &Path,
        device: u64,
        proxies: bool,
        remove: bool,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(Error::file(
                &directory.path,
                "stale cleanup depth limit exceeded",
            ));
        }
        for entry in directory.entries()? {
            let entry = entry.map_err(|e| Error::file(&directory.path, e))?;
            let name = entry.file_name();
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            *remaining = remaining
                .checked_sub(1)
                .ok_or_else(|| Error::file(&directory.path, "stale cleanup node limit exceeded"))?;
            let component = std::ffi::OsStr::from_bytes(name.to_bytes());
            let path = directory.path.join(component);
            let relative = relative.join(component);
            let stat = statat(&directory.file, name, AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|e| Error::file(&path, e))?;
            let kind = FileType::from_raw_mode(stat.st_mode);
            match kind {
                FileType::Directory => {
                    let child = directory
                        .child(name)?
                        .ok_or_else(|| Error::file(&path, "stale directory disappeared"))?;
                    if child.identity()?.0 != device {
                        return Err(Error::file(
                            &path,
                            "stale cleanup refuses a filesystem boundary",
                        ));
                    }
                    visit(
                        &child,
                        &relative,
                        device,
                        proxies,
                        remove,
                        remaining,
                        depth + 1,
                    )?;
                    if remove {
                        unlinkat(&directory.file, name, AtFlags::REMOVEDIR)
                            .map_err(|e| Error::file(&path, e))?;
                    }
                }
                FileType::RegularFile => {
                    if remove {
                        unlinkat(&directory.file, name, AtFlags::empty())
                            .map_err(|e| Error::file(&path, e))?;
                    }
                }
                FileType::Symlink
                    if proxies
                        && relative.parent() == Some(Path::new("bin"))
                        && [
                            b"qli".as_slice(),
                            b"qleisli",
                            b"qargo",
                            b"qlippy",
                            b"qlifmt",
                            b"qlidoc",
                        ]
                        .contains(&name.to_bytes()) =>
                {
                    // Interrupted bootstrap legitimately contains these six links.
                    // Accept their exact relative target, then unlink without following.
                    let target = readlinkat(&directory.file, name, Vec::new())
                        .map_err(|e| Error::file(&path, e))?;
                    if target.to_bytes() != b"qleisliup" {
                        return Err(Error::file(
                            &path,
                            "stale cleanup rejects an unexpected proxy target",
                        ));
                    }
                    if remove {
                        unlinkat(&directory.file, name, AtFlags::empty())
                            .map_err(|e| Error::file(&path, e))?;
                    }
                }
                _ => {
                    return Err(Error::file(
                        &path,
                        "stale cleanup rejects symlinks and special files",
                    ));
                }
            }
        }
        if remove {
            directory
                .file
                .sync_all()
                .map_err(|e| Error::file(&directory.path, e))?;
        }
        Ok(())
    }

    fn generations(state: &Directory, remaining: &mut usize) -> Result<()> {
        let name = crate::distribution::current_generation(&state.path)?;
        let Some(generations) = state.child(c"generations")? else {
            if name.is_some() {
                return Err(Error::file(
                    &state.path,
                    "committed metadata generation is missing",
                ));
            }
            return Ok(());
        };
        let keep = if let Some(name) = name {
            let name = std::ffi::CString::new(name).map_err(|e| Error::file(&state.path, e))?;
            let active = generations.child(&name)?.ok_or_else(|| {
                Error::file(&state.path, "committed metadata generation is missing")
            })?;
            Some(active.identity()?)
        } else {
            None
        };
        reclaim(&generations, &[b"state-"], keep, false, remaining)
    }

    pub(super) fn metadata_generations(state: &Path) -> Result<()> {
        files::managed_directory(state)?;
        let mut remaining = MAX_TREES;
        generations(
            &Directory {
                file: files::open_directory(state)?,
                path: state.to_owned(),
            },
            &mut remaining,
        )
    }

    pub(super) fn stale(home: &Home) -> Result<()> {
        let root = Directory {
            file: files::open_directory(&home.path)?,
            path: home.path.clone(),
        };
        files::check_managed_directory(&root.file, &root.path)?;
        files::reclaim_state_temporaries(&root.file, &root.path)?;
        let mut remaining = MAX_TREES;
        // Validate the committed pointer before deleting any metadata generations.
        if let Some(metadata) = root.child(c"metadata")? {
            if let Some(state) = metadata.child(c"official")? {
                generations(&state, &mut remaining)?;
                files::reclaim_state_temporaries(&state.file, &state.path)?;
                reclaim(&state, &[b"work-"], None, false, &mut remaining)?;
            }
        }
        if let Some(toolchains) = root.child(c"toolchains")? {
            if let Some(transactions) = toolchains.child(c".transactions")? {
                reclaim(
                    &transactions,
                    &[b"install-", b"uninstall-"],
                    None,
                    false,
                    &mut remaining,
                )?;
            }
        }
        reclaim(&root, &[b".qleisliup-manager-"], None, true, &mut remaining)?;
        if let Some(bin) = root.child(c"bin")? {
            files::reclaim_state_temporaries(&bin.file, &bin.path)?;
            reclaim(&bin, &[b".qleisliup-manager-"], None, false, &mut remaining)?;
        }
        Ok(())
    }
}

pub(crate) fn stale(home: &Home, _lock: &files::Lock) -> Result<()> {
    #[cfg(unix)]
    return unix::stale(home);
    #[cfg(not(unix))]
    {
        let _ = home;
        Ok(())
    }
}

pub(crate) fn metadata_generations(state: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    return unix::metadata_generations(state);
    #[cfg(not(unix))]
    {
        let _ = state;
        Ok(())
    }
}
