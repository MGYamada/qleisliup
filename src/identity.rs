use std::fmt;

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ExactVersion(semver::Version);

impl ExactVersion {
    pub(crate) fn channel(text: &str) -> Result<Self> {
        let version = Self::parse(text)?;
        if !version.0.pre.is_empty() || !version.0.build.is_empty() {
            return Err(Error::operational(
                "channel versions cannot have prerelease or build metadata",
            ));
        }
        Ok(version)
    }

    pub(crate) fn parse(text: &str) -> Result<Self> {
        let version = semver::Version::parse(text).map_err(|_| {
            Error::operational(format!("expected an exact SemVer version, got {text:?}"))
        })?;
        if version.to_string() != text {
            return Err(Error::operational(format!(
                "expected a canonical exact SemVer version, got {text:?}"
            )));
        }
        Ok(Self(version))
    }
}

impl fmt::Display for ExactVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selector {
    Release(ExactVersion),
    Stable,
    Linked(String),
}

impl Selector {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        if text == "stable" {
            return Ok(Self::Stable);
        }
        if let Ok(version) = ExactVersion::parse(text) {
            return Ok(Self::Release(version));
        }
        if valid_link_name(text) {
            return Ok(Self::Linked(text.to_owned()));
        }
        Err(Error::operational(format!(
            "invalid toolchain selector {text:?}; expected an exact version, stable, or a local name"
        )))
    }
}

pub(crate) fn valid_link_name(name: &str) -> bool {
    !matches!(name, "stable" | "beta" | "nightly")
        && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) const HOSTS: [&str; 3] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
];

pub(crate) fn current_host() -> Result<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok(HOSTS[0])
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Ok(HOSTS[1])
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Ok(HOSTS[2])
    } else {
        Err(Error::operational(
            "unsupported host; this stage supports macOS ARM64/x86_64 and Linux x86_64",
        ))
    }
}

pub(crate) fn release_directory(name: &str) -> Result<(ExactVersion, &'static str)> {
    for host in HOSTS {
        if let Some(version) = name.strip_suffix(&format!("-{host}")) {
            return Ok((ExactVersion::parse(version)?, host));
        }
    }
    Err(Error::operational(format!(
        "invalid or unsupported toolchain directory {name:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_identity_preserves_prerelease_and_build_metadata() {
        let version = ExactVersion::parse("0.4.0-rc.1+build.001").unwrap();
        assert_eq!(version.to_string(), "0.4.0-rc.1+build.001");
        assert_ne!(
            version,
            ExactVersion::parse("0.4.0-rc.1+build.002").unwrap()
        );
        assert_eq!(
            release_directory("0.4.0-rc.1+build.001-aarch64-apple-darwin")
                .unwrap()
                .0,
            version
        );
    }

    #[test]
    fn constraints_aliases_and_noncanonical_versions_are_not_release_identities() {
        for value in [
            "",
            "stable",
            "dev",
            "^0.4",
            ">=0.4.0",
            "0.4",
            "v0.4.0",
            "0.04.0",
            " 0.4.0",
            "0.4.0\n",
            "0.4.0-01",
            "0.4.0+",
            "0.4.0/other",
        ] {
            assert!(ExactVersion::parse(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn linked_names_cannot_shadow_channels_or_paths() {
        for name in ["dev", "local-test_1"] {
            assert!(valid_link_name(name));
        }
        for name in [
            "", "stable", "beta", "nightly", "0.4.0", "../dev", "dev/name", "dev name", "開発",
        ] {
            assert!(!valid_link_name(name), "accepted {name:?}");
        }
    }
}
