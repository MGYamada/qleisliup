use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::files;
use crate::identity::ExactVersion;
use crate::state::{ArtifactIdentity, Home, read_json, relative_path, required_json, schema};

pub(crate) const TOOLS: [&str; 5] = ["qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"];
pub(crate) const RECEIPT: &str = ".qleisliup-receipt.json";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum Verifier {
    #[serde(rename = "embedded-rust")]
    EmbeddedRust,
    #[serde(rename = "external")]
    External { protocol: u32, path: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StdKind {
    Embedded,
    Directory,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) schema: u32,
    pub(crate) qleisli: String,
    pub(crate) std: String,
    pub(crate) std_kind: StdKind,
    pub(crate) qargo: String,
    pub(crate) qargo_checker_qleisli: String,
    pub(crate) verifier: Verifier,
    pub(crate) compiler_commit: String,
    pub(crate) verifier_commit: String,
    pub(crate) host: String,
}

impl Manifest {
    pub(crate) fn validate(&self, path: &Path, host: &str) -> Result<()> {
        schema(self.schema, path)?;
        for value in [
            &self.qleisli,
            &self.std,
            &self.qargo,
            &self.qargo_checker_qleisli,
        ] {
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
        if let Verifier::External {
            protocol,
            path: external,
        } = &self.verifier
        {
            if *protocol == 0 {
                return Err(Error::file(
                    path,
                    "external verifier protocol must be positive",
                ));
            }
            relative_path(external).map_err(|error| Error::file(path, error))?;
        }
        Ok(())
    }

    pub(crate) fn write_receipt(&self, root: &Path, identity: &ArtifactIdentity) -> Result<()> {
        let receipt = Receipt {
            schema: 1,
            qleisli: self.qleisli.clone(),
            std: self.std.clone(),
            std_kind: self.std_kind.clone(),
            qargo: self.qargo.clone(),
            qargo_checker_qleisli: self.qargo_checker_qleisli.clone(),
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
        files::replace(root, RECEIPT, &bytes, 0o600)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    qleisli: String,
    std: String,
    std_kind: StdKind,
    qargo: String,
    qargo_checker_qleisli: String,
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
        files::directory(&home.path.join("toolchains"))?;
        Self::release_at(&root, version, host, identities)
    }

    pub(crate) fn release_at(
        root: &Path,
        version: &ExactVersion,
        host: &str,
        identities: &BTreeMap<String, ArtifactIdentity>,
    ) -> Result<Self> {
        let name = format!("{version}-{host}");
        files::directory(root)?;
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
        schema(receipt.schema, &receipt_path)?;
        if !receipt.authenticated
            || receipt.qleisli != manifest.qleisli
            || receipt.std != manifest.std
            || receipt.std_kind != manifest.std_kind
            || receipt.qargo != manifest.qargo
            || receipt.qargo_checker_qleisli != manifest.qargo_checker_qleisli
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
        files::directory(&root.join("bin"))?;
        for tool in TOOLS {
            files::regular(&root.join("bin").join(tool), true)?;
        }
        for file in ["LICENSE", "NOTICE"] {
            files::regular(&root.join(file), false)?;
        }
        if manifest.std_kind == StdKind::Directory {
            files::directory(&root.join("std"))?;
        }
        validate_external(root, &manifest)?;
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
            validate_external(root, manifest)?;
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
        if !TOOLS.contains(&tool) {
            return Err(Error::usage(format!(
                "unknown tool {tool:?}; expected qli, qleisli, qargo, qlippy, qlifmt, or qlidoc"
            )));
        }
        let path = self.root.join("bin").join(tool);
        files::directory(&self.root.join("bin"))?;
        files::regular(&path, true)?;
        files::not_manager(&path)?;
        path.canonicalize()
            .map_err(|error| Error::file(&path, error))
    }
}

fn validate_external(root: &Path, manifest: &Manifest) -> Result<()> {
    if let Verifier::External { path, .. } = &manifest.verifier {
        let relative = relative_path(path)?;
        let mut parent = root.to_path_buf();
        if let Some(directories) = relative.parent() {
            for component in directories.components() {
                parent.push(component);
                files::directory(&parent)?;
            }
        }
        files::regular(&root.join(relative), true)?;
    }
    Ok(())
}
