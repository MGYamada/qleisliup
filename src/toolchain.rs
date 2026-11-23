use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{Error, Result};
use crate::files;
use crate::identity::ExactVersion;
use crate::state::{ArtifactIdentity, Home, read_json, relative_path, required_json};

pub(crate) const LEGACY_PROXIES: [&str; 6] =
    ["qli", "qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"];
pub(crate) const QLIRGO_PROXIES: [&str; 2] = ["qlirgo", "qlinties"];
const LEGACY_TOOLS: [&str; 5] = ["qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"];
const QLIRGO_TOOLS: [&str; 5] = ["qleisli", "qlirgo", "qlinties", "qlifmt", "qlidoc"];
pub(crate) const RECEIPT: &str = ".qleisliup-receipt.json";

pub(crate) fn proxy_name(name: &str) -> bool {
    LEGACY_PROXIES.contains(&name) || QLIRGO_PROXIES.contains(&name)
}

// A missing member can belong to the other schema. An explicit null is invalid.
pub(crate) fn present_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum Verifier {
    #[serde(rename = "embedded-rust")]
    EmbeddedRust,
    #[serde(rename = "external")]
    External { protocol: u32, path: String },
    #[serde(rename = "qleisli-kernel")]
    QleisliKernel { path: String },
}

impl Verifier {
    fn path(&self) -> Option<&str> {
        match self {
            Self::EmbeddedRust => None,
            Self::External { path, .. } | Self::QleisliKernel { path } => Some(path),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StdKind {
    Embedded,
    Directory,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) schema: u32,
    pub(crate) qleisli: String,
    pub(crate) std: String,
    pub(crate) std_kind: StdKind,
    #[serde(default, deserialize_with = "present_string")]
    pub(crate) qargo: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    pub(crate) qargo_checker_qleisli: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    pub(crate) qlirgo: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    pub(crate) qlirgo_checker_qleisli: Option<String>,
    pub(crate) verifier: Verifier,
    pub(crate) compiler_commit: String,
    pub(crate) verifier_commit: String,
    pub(crate) host: String,
}

impl Manifest {
    pub(crate) fn suite(&self) -> Result<(&'static str, &str, &str)> {
        match (
            self.schema,
            self.qargo.as_deref(),
            self.qargo_checker_qleisli.as_deref(),
            self.qlirgo.as_deref(),
            self.qlirgo_checker_qleisli.as_deref(),
        ) {
            (1, Some(version), Some(checker), None, None) => Ok(("qargo", version, checker)),
            (2, None, None, Some(version), Some(checker)) => Ok(("qlirgo", version, checker)),
            (1 | 2, ..) => Err(Error::operational(
                "tool suite fields do not match the manifest schema",
            )),
            _ => Err(Error::operational(format!(
                "unsupported schema {}",
                self.schema
            ))),
        }
    }

    fn inventory(&self) -> &[&str] {
        if self.schema == 2 {
            &QLIRGO_TOOLS
        } else {
            &LEGACY_TOOLS
        }
    }

    pub(crate) fn validate(&self, path: &Path, host: &str) -> Result<()> {
        let (_, version, checker) = self.suite().map_err(|error| Error::file(path, error))?;
        for value in [&*self.qleisli, &*self.std, version, checker] {
            ExactVersion::parse(value).map_err(|error| Error::file(path, error))?;
        }
        if self.qleisli != self.std || self.host != host {
            return Err(Error::file(path, "Qleisli/std version or host mismatch"));
        }
        for commit in [&self.compiler_commit, &self.verifier_commit] {
            if !matches!(commit.len(), 40 | 64)
                || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(Error::file(
                    path,
                    "expected full compiler/verifier commit IDs",
                ));
            }
        }
        if matches!(self.verifier, Verifier::QleisliKernel { .. }) != (self.schema == 2) {
            return Err(Error::file(
                path,
                "verifier kind does not match the manifest schema",
            ));
        }
        if matches!(self.verifier, Verifier::External { protocol: 0, .. }) {
            return Err(Error::file(
                path,
                "external verifier protocol must be positive",
            ));
        }
        if let Some(external) = self.verifier.path() {
            relative_path(external).map_err(|error| Error::file(path, error))?;
        }
        Ok(())
    }

    pub(crate) fn write_receipt(&self, root: &Path, identity: &ArtifactIdentity) -> Result<()> {
        let receipt = Receipt {
            schema: self.schema,
            qleisli: self.qleisli.clone(),
            std: self.std.clone(),
            std_kind: self.std_kind.clone(),
            qargo: self.qargo.clone(),
            qargo_checker_qleisli: self.qargo_checker_qleisli.clone(),
            qlirgo: self.qlirgo.clone(),
            qlirgo_checker_qleisli: self.qlirgo_checker_qleisli.clone(),
            host: self.host.clone(),
            verifier: self.verifier.clone(),
            manifest_target: identity.manifest_target.clone(),
            manifest_sha256: identity.manifest_sha256.clone(),
            artifact_target: identity.artifact_target.clone(),
            artifact_sha256: identity.artifact_sha256.clone(),
            artifact_size: identity.artifact_size,
            authenticated: true,
        };
        let bytes = serde_json::to_vec_pretty(&receipt)
            .map_err(|error| Error::operational(error.to_string()))?;
        files::replace_staged(root, RECEIPT, &bytes, 0o600)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    qleisli: String,
    std: String,
    std_kind: StdKind,
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    qargo: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    qargo_checker_qleisli: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    qlirgo: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_string",
        skip_serializing_if = "Option::is_none"
    )]
    qlirgo_checker_qleisli: Option<String>,
    host: String,
    verifier: Verifier,
    manifest_target: String,
    manifest_sha256: String,
    artifact_target: String,
    artifact_sha256: String,
    artifact_size: u64,
    authenticated: bool,
}

impl Receipt {
    fn identity(&self) -> ArtifactIdentity {
        ArtifactIdentity {
            manifest_target: self.manifest_target.clone(),
            manifest_sha256: self.manifest_sha256.clone(),
            artifact_target: self.artifact_target.clone(),
            artifact_sha256: self.artifact_sha256.clone(),
            artifact_size: self.artifact_size,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Toolchain {
    pub(crate) name: String,
    pub(crate) root: PathBuf,
    pub(crate) host: String,
    pub(crate) manifest: Option<Manifest>,
    pub(crate) linked: bool,
}

impl Toolchain {
    pub(crate) fn release(
        home: &Home,
        version: &ExactVersion,
        host: &str,
        identities: &BTreeMap<String, ArtifactIdentity>,
    ) -> Result<Self> {
        let name = format!("{version}-{host}");
        let root = home.path.join("toolchains").join(&name);
        files::managed_directory(&home.path.join("toolchains"))?;
        Self::release_at(&root, version, host, identities)
    }

    pub(crate) fn release_at(
        root: &Path,
        version: &ExactVersion,
        host: &str,
        identities: &BTreeMap<String, ArtifactIdentity>,
    ) -> Result<Self> {
        let name = format!("{version}-{host}");
        files::managed_directory(root)?;
        let manifest_path = root.join("toolchain.json");
        let manifest: Manifest = required_json(&manifest_path)?;
        manifest.validate(&manifest_path, host)?;
        if manifest.qleisli != version.to_string() {
            return Err(Error::file(
                &manifest_path,
                "manifest version differs from selected release",
            ));
        }
        let receipt_path = root.join(RECEIPT);
        let receipt: Receipt = required_json(&receipt_path)?;
        if !receipt.authenticated
            || receipt.schema != manifest.schema
            || receipt.qleisli != manifest.qleisli
            || receipt.std != manifest.std
            || receipt.std_kind != manifest.std_kind
            || receipt.qargo != manifest.qargo
            || receipt.qargo_checker_qleisli != manifest.qargo_checker_qleisli
            || receipt.qlirgo != manifest.qlirgo
            || receipt.qlirgo_checker_qleisli != manifest.qlirgo_checker_qleisli
            || receipt.host != manifest.host
            || receipt.verifier != manifest.verifier
        {
            return Err(Error::file(
                &receipt_path,
                "receipt does not match the release manifest and installation authentication record",
            ));
        }
        let identity = receipt.identity();
        identity
            .validate()
            .map_err(|error| Error::file(&receipt_path, error))?;
        identity
            .bind_release(version, host)
            .map_err(|error| Error::file(&receipt_path, error))?;
        if identities.get(&name) != Some(&identity) {
            return Err(Error::file(
                &receipt_path,
                "receipt differs from the remembered release identity (or identity is missing)",
            ));
        }
        files::managed_directory(&root.join("bin"))?;
        for tool in manifest.inventory() {
            files::regular(&root.join("bin").join(tool), true)?;
        }
        for file in ["LICENSE", "NOTICE"] {
            files::regular(&root.join(file), false)?;
        }
        if manifest.std_kind == StdKind::Directory {
            files::managed_directory(&root.join("std"))?;
        }
        validate_external(root, &manifest, false)?;
        Ok(Self {
            name: version.to_string(),
            root: root
                .canonicalize()
                .map_err(|error| Error::file(root, error))?,
            host: host.to_owned(),
            manifest: Some(manifest),
            linked: false,
        })
    }

    pub(crate) fn linked(name: &str, root: &Path, host: &str) -> Result<Self> {
        files::directory(root).map_err(|error| {
            Error::operational(format!(
                "linked toolchain {name:?} is unavailable; repair its directory: {error}"
            ))
        })?;
        let canonical = root
            .canonicalize()
            .map_err(|error| Error::file(root, error))?;
        if canonical != root {
            return Err(Error::file(
                root,
                "linked path is not canonical; repair the registration",
            ));
        }
        files::directory(&root.join("bin"))?;
        files::regular(&root.join("bin/qleisli"), true)?;
        files::not_manager(&root.join("bin/qleisli"))?;
        let manifest_path = root.join("toolchain.json");
        let manifest: Option<Manifest> = read_json(&manifest_path)?;
        if let Some(manifest) = &manifest {
            manifest.validate(&manifest_path, host)?;
            if manifest.std_kind == StdKind::Directory {
                files::directory(&root.join("std"))?;
            }
            validate_external(root, manifest, true)?;
        }
        Ok(Self {
            name: name.to_owned(),
            root: canonical,
            host: host.to_owned(),
            manifest,
            linked: true,
        })
    }

    pub(crate) fn executable(&self, tool: &str) -> Result<PathBuf> {
        let tool = if tool == "qli" { "qleisli" } else { tool };
        if !proxy_name(tool) {
            return Err(Error::usage(format!("unknown tool {tool:?}")));
        }
        if !self.linked
            && self
                .manifest
                .as_ref()
                .is_some_and(|manifest| !manifest.inventory().contains(&tool))
        {
            return Err(Error::operational(format!(
                "selected release does not provide {tool}"
            )));
        }
        let path = self.root.join("bin").join(tool);
        if self.linked {
            files::directory(&self.root.join("bin"))?;
        } else {
            files::managed_directory(&self.root.join("bin"))?;
        }
        files::regular(&path, true)?;
        files::not_manager(&path)?;
        path.canonicalize()
            .map_err(|error| Error::file(&path, error))
    }

    pub(crate) fn native_kernel(&self) -> Option<PathBuf> {
        match &self.manifest.as_ref()?.verifier {
            Verifier::QleisliKernel { path } => Some(self.root.join(path)),
            _ => None,
        }
    }
}

fn validate_external(root: &Path, manifest: &Manifest, linked: bool) -> Result<()> {
    if let Some(path) = manifest.verifier.path() {
        let relative = relative_path(path)?;
        let mut parent = root.to_path_buf();
        if let Some(directories) = relative.parent() {
            for component in directories.components() {
                parent.push(component);
                if linked {
                    files::directory(&parent)?;
                } else {
                    files::managed_directory(&parent)?;
                }
            }
        }
        let executable = root.join(relative);
        files::regular(&executable, true)?;
        if matches!(manifest.verifier, Verifier::QleisliKernel { .. }) {
            files::not_manager(&executable)?;
        }
    }
    Ok(())
}
