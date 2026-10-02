use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::{Error, Result};
use crate::files;
use crate::identity::{ExactVersion, release_directory, valid_link_name};

#[derive(Clone, Debug)]
pub(crate) struct Home {
    pub(crate) path: PathBuf,
}

impl Home {
    pub(crate) fn from_environment() -> Result<Self> {
        Self::from_values(std::env::var_os("QLEISLIUP_HOME"), std::env::var_os("HOME"))
    }

    pub(crate) fn from_values(
        explicit: Option<OsString>,
        user_home: Option<OsString>,
    ) -> Result<Self> {
        let path = match explicit {
            Some(value) => PathBuf::from(value),
            None => PathBuf::from(user_home.ok_or_else(|| {
                Error::operational("HOME is unset; set an absolute QLEISLIUP_HOME")
            })?)
            .join(".qleisliup"),
        };
        if !path.is_absolute() {
            return Err(Error::operational(
                "QLEISLIUP_HOME (or HOME) must be a nonempty absolute path",
            ));
        }
        Ok(Self { path })
    }

    pub(crate) fn lock(&self) -> Result<files::Lock> {
        files::create_home(&self.path)?;
        files::Lock::home(&self.path)
    }

    pub(crate) fn settings(&self) -> Result<Settings> {
        let path = self.path.join("settings.json");
        let settings = read_json::<Settings>(&path)?.unwrap_or(Settings {
            schema: 1,
            default: DefaultValue::Unset(()),
        });
        schema(settings.schema, &path)?;
        if let DefaultValue::Version(value) = &settings.default {
            ExactVersion::parse(value).map_err(|error| Error::file(&path, error))?;
        }
        Ok(settings)
    }

    pub(crate) fn links(&self) -> Result<BTreeMap<String, PathBuf>> {
        let path = self.path.join("links.json");
        let links = read_json::<Links>(&path)?.unwrap_or(Links {
            schema: 1,
            links: BTreeMap::new(),
        });
        schema(links.schema, &path)?;
        for (name, linked_path) in &links.links {
            if !valid_link_name(name) || !linked_path.is_absolute() {
                return Err(Error::file(
                    &path,
                    "invalid local name or non-absolute linked path",
                ));
            }
        }
        Ok(links.links)
    }

    pub(crate) fn identities(&self) -> Result<BTreeMap<String, ArtifactIdentity>> {
        Ok(self.identity_record()?.releases)
    }

    fn identity_record(&self) -> Result<Identities> {
        let path = self.path.join("identities.json");
        let identities = read_json::<Identities>(&path)?.unwrap_or(Identities {
            schema: 1,
            releases: BTreeMap::new(),
            managers: BTreeMap::new(),
        });
        schema(identities.schema, &path)?;
        for (name, identity) in &identities.releases {
            let (version, host) =
                release_directory(name).map_err(|error| Error::file(&path, error))?;
            identity
                .validate()
                .map_err(|error| Error::file(&path, error))?;
            identity
                .bind_release(&version, host)
                .map_err(|error| Error::file(&path, error))?;
        }
        for (name, identity) in &identities.managers {
            let (version, host) =
                release_directory(name).map_err(|error| Error::file(&path, error))?;
            identity
                .validate()
                .map_err(|error| Error::file(&path, error))?;
            identity
                .bind_manager(&version, host)
                .map_err(|error| Error::file(&path, error))?;
        }
        Ok(identities)
    }

    pub(crate) fn remember(&self, name: &str, identity: &ArtifactIdentity) -> Result<()> {
        let mut record = self.identity_record()?;
        let (version, host) = release_directory(name)?;
        identity.validate()?;
        identity.bind_release(&version, host)?;
        if record
            .releases
            .get(name)
            .is_some_and(|known| known != identity)
        {
            return Err(Error::operational(
                "release republication differs from the remembered identity",
            ));
        }
        if record.releases.values().any(|known| {
            known.manifest_target == identity.manifest_target
                && known.manifest_sha256 != identity.manifest_sha256
        }) {
            return Err(Error::operational(
                "release manifest republication differs from the remembered identity for another host",
            ));
        }
        record.releases.insert(name.to_owned(), identity.clone());
        self.save_record("identities.json", &record)
    }

    pub(crate) fn manager_identities(&self) -> Result<BTreeMap<String, ArtifactIdentity>> {
        Ok(self.identity_record()?.managers)
    }

    pub(crate) fn remember_manager(&self, name: &str, identity: &ArtifactIdentity) -> Result<()> {
        let mut record = self.identity_record()?;
        let (version, host) = release_directory(name)?;
        identity.validate()?;
        identity.bind_manager(&version, host)?;
        check_manager_identity(&record.managers, name, identity)?;
        record.managers.insert(name.to_owned(), identity.clone());
        self.save_record("identities.json", &record)
    }

    fn channels(&self) -> Result<Channels> {
        let path = self.path.join("channels.json");
        let record = read_json::<Channels>(&path)
            .map_err(|error| {
                Error::operational(format!(
                    "invalid TUF-authenticated channel history: {error}"
                ))
            })?
            .unwrap_or(Channels {
                schema: 1,
                channels: BTreeMap::new(),
            });
        schema(record.schema, &path)?;
        for (name, observation) in &record.channels {
            if !matches!(name.as_str(), "stable" | "qleisliup-stable")
                || observation.target != format!("channels/{name}.json")
                || !sha256(&observation.sha256)
                || !observation.authenticated
            {
                return Err(Error::file(
                    &path,
                    "invalid TUF-authenticated channel observation",
                ));
            }
            ExactVersion::channel(&observation.version)
                .map_err(|error| Error::file(&path, error))?;
        }
        Ok(record)
    }

    pub(crate) fn stable(&self) -> Result<ExactVersion> {
        let record = self.channels()?;
        let value = record.channels.get("stable").ok_or_else(|| Error::operational(
            "no TUF-authenticated stable observation; run qleisliup install stable (production distribution is not configured)"))?;
        ExactVersion::channel(&value.version)
    }

    pub(crate) fn observe_stable(&self, version: &ExactVersion, sha256: String) -> Result<()> {
        self.observe_channel("stable", version, sha256)
    }

    pub(crate) fn observe_manager(&self, version: &ExactVersion, sha256: String) -> Result<()> {
        self.observe_channel("qleisliup-stable", version, sha256)
    }

    fn observe_channel(&self, name: &str, version: &ExactVersion, sha256: String) -> Result<()> {
        let mut record = self.channels()?;
        if let Some(old) = record.channels.get(name) {
            if *version < ExactVersion::channel(&old.version)? {
                return Err(Error::operational(format!(
                    "{name} channel rollback below the observed version is forbidden"
                )));
            }
        }
        record.channels.insert(
            name.into(),
            ChannelObservation {
                version: version.to_string(),
                target: format!("channels/{name}.json"),
                sha256,
                authenticated: true,
            },
        );
        self.save_record("channels.json", &record)
    }

    pub(crate) fn save_record(&self, name: &str, record: &impl Serialize) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(record)
            .map_err(|error| Error::operational(error.to_string()))?;
        bytes.push(b'\n');
        if bytes.len() > 1024 * 1024 {
            return Err(Error::operational(
                "local state exceeds the 1 MiB limit; existing state preserved",
            ));
        }
        files::replace(&self.path, name, &bytes, 0o600)
    }

    pub(crate) fn save_default(&self, version: &ExactVersion) -> Result<()> {
        let settings = Settings {
            schema: 1,
            default: DefaultValue::Version(version.to_string()),
        };
        let mut bytes = serde_json::to_vec_pretty(&settings)
            .map_err(|error| Error::operational(error.to_string()))?;
        bytes.push(b'\n');
        files::replace(&self.path, "settings.json", &bytes, 0o600)
    }

    pub(crate) fn save_links(&self, links: BTreeMap<String, PathBuf>) -> Result<()> {
        let record = Links { schema: 1, links };
        self.save_record("links.json", &record)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum DefaultValue {
    Version(String),
    Unset(()),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) schema: u32,
    pub(crate) default: DefaultValue,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Links {
    schema: u32,
    links: BTreeMap<String, PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Identities {
    schema: u32,
    releases: BTreeMap<String, ArtifactIdentity>,
    #[serde(default)]
    managers: BTreeMap<String, ArtifactIdentity>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Channels {
    schema: u32,
    channels: BTreeMap<String, ChannelObservation>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChannelObservation {
    version: String,
    target: String,
    sha256: String,
    authenticated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactIdentity {
    pub(crate) manifest_target: String,
    pub(crate) manifest_sha256: String,
    pub(crate) artifact_target: String,
    pub(crate) artifact_sha256: String,
    pub(crate) artifact_size: u64,
}

impl ArtifactIdentity {
    pub(crate) fn validate(&self) -> Result<()> {
        if !sha256(&self.manifest_sha256)
            || !sha256(&self.artifact_sha256)
            || self.artifact_size == 0
        {
            return Err(Error::operational(
                "invalid recorded SHA-256 or artifact size",
            ));
        }
        relative_path(&self.manifest_target)?;
        relative_path(&self.artifact_target)?;
        Ok(())
    }

    pub(crate) fn bind_release(&self, version: &ExactVersion, host: &str) -> Result<()> {
        if self.manifest_target != format!("releases/{version}/manifest.json")
            || self.artifact_target != format!("releases/{version}/{host}.tar.zst")
        {
            return Err(Error::operational(
                "recorded targets do not match the release version/host",
            ));
        }
        Ok(())
    }

    pub(crate) fn bind_manager(&self, version: &ExactVersion, host: &str) -> Result<()> {
        if self.manifest_target != format!("qleisliup/{version}/manifest.json")
            || self.artifact_target != format!("qleisliup/{version}/{host}/qleisliup")
        {
            return Err(Error::operational(
                "recorded targets do not match the manager version/host",
            ));
        }
        Ok(())
    }
}

pub(crate) fn check_manager_identity(
    known: &BTreeMap<String, ArtifactIdentity>,
    name: &str,
    identity: &ArtifactIdentity,
) -> Result<()> {
    if known.get(name).is_some_and(|old| old != identity)
        || known.values().any(|old| {
            old.manifest_target == identity.manifest_target
                && old.manifest_sha256 != identity.manifest_sha256
        })
    {
        return Err(Error::operational(
            "manager republication differs from the remembered identity",
        ));
    }
    Ok(())
}

pub(crate) fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(crate) fn relative_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || !path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Error::operational(format!(
            "expected a confined relative path, got {value:?}"
        )));
    }
    Ok(path)
}

pub(crate) fn schema(version: u32, path: &Path) -> Result<()> {
    if version != 1 {
        return Err(Error::file(
            path,
            format!("unsupported schema {version}; expected 1"),
        ));
    }
    Ok(())
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    files::read_optional(path)?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| Error::file(path, format!("invalid JSON: {error}")))
        })
        .transpose()
}

pub(crate) fn required_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    read_json(path)?.ok_or_else(|| Error::file(path, "required metadata is missing"))
}
