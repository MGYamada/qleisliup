use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use tough::Transport;

use crate::archive;
use crate::distribution::{self, Client, Source};
use crate::error::{Error, Result};
use crate::files;
use crate::identity::{ExactVersion, HOSTS, Selector};
use crate::state::{ArtifactIdentity, DefaultValue, Home, required_json};
use crate::toolchain::{Manifest, StdKind, Toolchain, Verifier};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    schema: u32,
    qleisli: String,
    std: String,
    std_kind: StdKind,
    qargo: String,
    qargo_checker_qleisli: String,
    verifier: Verifier,
    compiler_commit: String,
    verifier_commit: String,
    artifacts: BTreeMap<String, Artifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    target: String,
    sha256: String,
    size: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Channel {
    schema: u32,
    qleisli: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Boundary {
    Metadata,
    Channel,
    Manifest,
    Download,
    Extract,
    Receipt,
    Identity,
    Publish,
}

pub(crate) fn install(home: &Home, selector: &Selector, host: &str) -> Result<ExactVersion> {
    // An already installed exact selection is a strictly local operation.
    if let Selector::Release(version) = selector {
        if existing(home, version, host)? {
            return Ok(version.clone());
        }
    }
    let source = Source::official()?;
    run(home, selector, host, source, &|_| Ok(()))
}

pub(crate) fn run<T: Transport + Clone + 'static>(
    home: &Home,
    selector: &Selector,
    host: &str,
    source: Source<T>,
    boundary: &dyn Fn(Boundary) -> Result<()>,
) -> Result<ExactVersion> {
    if !HOSTS.contains(&host) {
        return Err(Error::operational("unsupported distribution host"));
    }
    home.identities()?;
    let _lock = home.lock()?;
    crate::cleanup::stale(home, &_lock)?;
    if let Selector::Release(version) = selector {
        if existing(home, version, host)? {
            return Ok(version.clone());
        }
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| Error::operational(e.to_string()))?;
    runtime.block_on(transaction(home, selector, host, source, boundary))
}

async fn transaction<T: Transport + Clone + 'static>(
    home: &Home,
    selector: &Selector,
    host: &str,
    source: Source<T>,
    boundary: &dyn Fn(Boundary) -> Result<()>,
) -> Result<ExactVersion> {
    let client = Client::load(home, source).await?;
    boundary(Boundary::Metadata)?;
    let parent = home.path.join("toolchains");
    files::create_directory(&parent)?;
    let transactions = parent.join(".transactions");
    files::create_directory(&transactions)?;
    let work = files::temporary_directory(&transactions, "install-")?;
    let version = match selector {
        Selector::Release(version) => version.clone(),
        Selector::Stable => {
            let info = client.info("channels/stable.json", distribution::SMALL_TARGET)?;
            let path = work.path().join("channel.json");
            client.download(&info, &path).await?;
            let channel: Channel = required_json(&path)?;
            crate::state::schema(channel.schema, &path)?;
            let version = ExactVersion::channel(&channel.qleisli)?;
            home.observe_stable(&version, info.sha256)?;
            boundary(Boundary::Channel)?;
            version
        }
        Selector::Linked(_) => {
            return Err(Error::usage("install accepts an exact version or stable"));
        }
    };
    let name = format!("{version}-{host}");
    let info = client.info(
        &format!("releases/{version}/manifest.json"),
        distribution::SMALL_TARGET,
    )?;
    let manifest_path = work.path().join("manifest.json");
    client.download(&info, &manifest_path).await?;
    let release: Release = required_json(&manifest_path)?;
    let expected = release.manifest(&manifest_path, &version, host)?;
    let artifact = release
        .artifacts
        .get(host)
        .ok_or_else(|| Error::operational("release has no artifact for this host"))?;
    let target = client.info(&artifact.target, distribution::ARCHIVE_TARGET)?;
    if target.sha256 != artifact.sha256 || target.size != artifact.size {
        return Err(Error::operational(
            "release manifest disagrees with the authenticated artifact identity",
        ));
    }
    let identity = ArtifactIdentity {
        manifest_target: info.name,
        manifest_sha256: info.sha256,
        artifact_target: target.name.clone(),
        artifact_sha256: target.sha256.clone(),
        artifact_size: target.size,
    };
    identity.validate()?;
    identity.bind_release(&version, host)?;
    let mut identities = home.identities()?;
    if identities
        .get(&name)
        .is_some_and(|known| *known != identity)
    {
        return Err(Error::operational(
            "release republication differs from the remembered identity",
        ));
    }
    if identities.values().any(|known| {
        known.manifest_target == identity.manifest_target
            && known.manifest_sha256 != identity.manifest_sha256
    }) {
        return Err(Error::operational(
            "release manifest republication differs from a remembered identity for another host",
        ));
    }
    boundary(Boundary::Manifest)?;
    if existing(home, &version, host)? {
        return Ok(version);
    }
    let download = work.path().join("artifact.tar.zst");
    client.download(&target, &download).await?;
    boundary(Boundary::Download)?;
    let payload = work.path().join("payload");
    files::create_directory(&payload)?;
    let root = archive::extract(&download, &payload, &name, archive::Limits::default())?;
    let actual: Manifest = required_json(&root.join("toolchain.json"))?;
    if actual != expected {
        return Err(Error::operational(
            "internal manifest differs from the authenticated release manifest",
        ));
    }
    boundary(Boundary::Extract)?;
    expected.write_receipt(&root, &identity)?;
    identities.insert(name.clone(), identity.clone());
    Toolchain::release_at(&root, &version, host, &identities)?;
    archive::sync_tree(&root)?;
    boundary(Boundary::Receipt)?;
    home.remember(&name, &identity)?;
    boundary(Boundary::Identity)?;
    boundary(Boundary::Publish)?;
    publish(&root, &parent.join(&name))?;
    files::sync_directory(&parent).map_err(|e| {
        Error::operational(format!(
            "installation published; directory durability was not confirmed: {e}"
        ))
    })?;
    Ok(version)
}

impl Release {
    fn manifest(&self, path: &Path, version: &ExactVersion, host: &str) -> Result<Manifest> {
        let manifest = Manifest {
            schema: self.schema,
            qleisli: self.qleisli.clone(),
            std: self.std.clone(),
            std_kind: self.std_kind.clone(),
            qargo: self.qargo.clone(),
            qargo_checker_qleisli: self.qargo_checker_qleisli.clone(),
            verifier: self.verifier.clone(),
            compiler_commit: self.compiler_commit.clone(),
            verifier_commit: self.verifier_commit.clone(),
            host: host.to_owned(),
        };
        manifest.validate(path, host)?;
        if self.qleisli != version.to_string() {
            return Err(Error::file(path, "release manifest version mismatch"));
        }
        for (host, artifact) in &self.artifacts {
            if !HOSTS.contains(&host.as_str())
                || artifact.target != format!("releases/{version}/{host}.tar.zst")
                || !crate::state::sha256(&artifact.sha256)
                || artifact.size == 0
            {
                return Err(Error::file(path, "invalid release artifact binding"));
            }
        }
        Ok(manifest)
    }
}

fn existing(home: &Home, version: &ExactVersion, host: &str) -> Result<bool> {
    let path = home
        .path
        .join("toolchains")
        .join(format!("{version}-{host}"));
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            Toolchain::release(home, version, host, &home.identities()?)?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::file(&path, e)),
    }
}

#[cfg(unix)]
pub(crate) fn publish(staged: &Path, final_path: &Path) -> Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged,
        rustix::fs::CWD,
        final_path,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|e| Error::file(final_path, e))
}
#[cfg(not(unix))]
pub(crate) fn publish(_: &Path, _: &Path) -> Result<()> {
    Err(Error::operational(
        "publication requires a supported Unix host",
    ))
}

pub(crate) fn uninstall(home: &Home, version: &ExactVersion, host: &str) -> Result<()> {
    home.identities()?;
    let _lock = home.lock()?;
    crate::cleanup::stale(home, &_lock)?;
    if matches!(home.settings()?.default, DefaultValue::Version(value) if value == version.to_string())
    {
        return Err(Error::operational(
            "cannot uninstall the global default; choose another installed exact default first",
        ));
    }
    if !existing(home, version, host)? {
        return Err(Error::operational(format!(
            "toolchain {version} is not installed for {host}"
        )));
    }
    let parent = home.path.join("toolchains");
    let transactions = parent.join(".transactions");
    files::create_directory(&transactions)?;
    let trash = files::temporary_directory(&transactions, "uninstall-")?;
    let root = parent.join(format!("{version}-{host}"));
    publish(&root, &trash.path().join("removed"))?;
    files::sync_directory(&parent).map_err(|e| {
        Error::operational(format!(
            "installation removed; directory durability was not confirmed: {e}"
        ))
    })?;
    trash.close().map_err(|e| {
        Error::operational(format!(
            "installation removed; temporary cleanup failed: {e}"
        ))
    })
}

pub(crate) fn sync_version(cwd: &Path) -> Result<ExactVersion> {
    crate::declaration::nearest(cwd)?
        .map(|(version, _)| version)
        .ok_or_else(|| {
            Error::operational("sync requires a repository qleisli-toolchain.toml declaration")
        })
}

#[cfg(test)]
mod tests;
