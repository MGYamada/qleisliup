use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::state::Home;
use crate::toolchain::Toolchain;

pub(crate) fn link(home: &Home, name: &str, path: &Path, host: &str) -> Result<PathBuf> {
    let root = path
        .canonicalize()
        .map_err(|error| Error::file(path, error))?;
    if root.to_str().is_none() {
        return Err(Error::file(
            &root,
            "link paths must be UTF-8 for JSON storage",
        ));
    }
    Toolchain::linked(name, &root, host)?;
    ensure_absent(home, name)?;
    let _lock = home.lock()?;
    let mut links = home.links()?;
    if links.contains_key(name) {
        return Err(already_registered(name));
    }
    // Revalidate after acquiring the lock; do not register a vanished build.
    Toolchain::linked(name, &root, host)?;
    links.insert(name.to_owned(), root.clone());
    home.save_links(links)?;
    Ok(root)
}

fn ensure_absent(home: &Home, name: &str) -> Result<()> {
    if home.links()?.contains_key(name) {
        return Err(already_registered(name));
    }
    Ok(())
}

fn already_registered(name: &str) -> Error {
    Error::operational(format!(
        "local toolchain {name:?} is already registered; unlink it before re-registering"
    ))
}

pub(crate) fn unlink(home: &Home, name: &str) -> Result<()> {
    if !home.links()?.contains_key(name) {
        return Err(not_registered(name));
    }
    let _lock = home.lock()?;
    let mut links = home.links()?;
    if links.remove(name).is_none() {
        return Err(not_registered(name));
    }
    home.save_links(links)
}

fn not_registered(name: &str) -> Error {
    Error::operational(format!("local toolchain {name:?} is not registered"))
}
