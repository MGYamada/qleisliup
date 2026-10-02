use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::files;
use crate::identity::ExactVersion;
use crate::state::Home;

pub(crate) const NAME: &str = "qleisli-toolchain.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    toolchain: Toolchain,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Toolchain {
    version: String,
}

pub(crate) fn parse(bytes: &[u8], path: &Path) -> Result<ExactVersion> {
    let text = std::str::from_utf8(bytes).map_err(|error| Error::file(path, error))?;
    let declaration: Declaration = toml::from_str(text)
        .map_err(|error| Error::file(path, format!("invalid toolchain declaration: {error}")))?;
    ExactVersion::parse(&declaration.toolchain.version).map_err(|error| Error::file(path, error))
}

pub(crate) fn nearest(cwd: &Path) -> Result<Option<(ExactVersion, PathBuf)>> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| Error::file(cwd, error))?;
    for parent in cwd.ancestors() {
        let path = parent.join(NAME);
        if let Some(bytes) = files::read_optional(&path)? {
            return Ok(Some((parse(&bytes, &path)?, path)));
        }
    }
    Ok(None)
}

pub(crate) fn pin(home: &Home, cwd: &Path, version: &ExactVersion) -> Result<PathBuf> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| Error::file(cwd, error))?;
    let path = cwd.join(NAME);
    validate_existing(&path)?;
    let _home_lock = home.lock()?;
    // Directory locking also serializes pin writers using different manager homes.
    let _directory_lock = files::Lock::directory(&cwd)?;
    validate_existing(&path)?;
    let bytes = format!("[toolchain]\nversion = \"{version}\"\n");
    files::replace(&cwd, NAME, bytes.as_bytes(), 0o644)?;
    Ok(path)
}

fn validate_existing(path: &Path) -> Result<()> {
    if let Some(bytes) = files::read_optional(path)? {
        parse(&bytes, path)?;
    }
    Ok(())
}
