//! Authenticated manager lifecycle. Production trust remains unconfigured.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::{Command, Stdio};
use std::time::Duration;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tough::Transport;

use crate::distribution::{self, Client, Source};
use crate::error::{Error, Result};
use crate::identity::{ExactVersion, HOSTS, current_host};
use crate::state::{ArtifactIdentity, Home, check_manager_identity, required_json};
use crate::{files, state};

const MANAGER_LIMIT: u64 = 128 * 1024 * 1024;
const PROXIES: [&str; 6] = ["qli", "qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"];
const MARKER: &str = ".qleisliup-managed.json";
#[cfg(all(test, unix))]
mod tests;
const INIT_HELP: &str = "qleisliup-init - authenticated manager bootstrap

Usage:
  qleisliup-init                 Install the manager and six proxy symlinks
  qleisliup-init --help | -h
  qleisliup-init --version | -V

Current implementation: Stage 4, bootstrap and manager self-update.
Production distribution URLs and trusted root are not configured.
Bootstrap fails closed until production trust is established.
Uses QLEISLIUP_HOME or ~/.qleisliup. Requires an absent bin destination.
Does not install a toolchain or modify shell configuration.
";

#[derive(Clone)]
pub(crate) enum Mode {
    Bootstrap,
    Update {
        running: PathBuf,
        version: ExactVersion,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Boundary {
    Metadata,
    Channel,
    Manifest,
    Download,
    Verify,
    Identity,
    Publish,
    Published,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Channel {
    schema: u32,
    qleisliup: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    qleisliup: String,
    artifacts: BTreeMap<String, Artifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    target: String,
    sha256: String,
    size: u64,
}
// This immutable bootstrap marker and the authenticated identity ledger let a
// replacement recover without a mutable version pointer or a two-file commit.
// The ledger records past authentication; the filesystem owner can change it.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    schema: u32,
    host: String,
    authenticated: bool,
}

pub(crate) fn init_cli(args: &[OsString]) -> Result<()> {
    let output = match args {
        [flag] if flag == "--help" || flag == "-h" => INIT_HELP.to_owned(),
        [flag] if flag == "--version" || flag == "-V" => {
            format!("qleisliup-init {}\n", env!("CARGO_PKG_VERSION"))
        }
        [] => {
            let home = Home::from_environment()?;
            let host = current_host()?;
            preflight(&home, host, &Mode::Bootstrap)?;
            let version = run(&home, host, Mode::Bootstrap, Source::official()?, &|_| {
                Ok(())
            })?;
            format!(
                "installed qleisliup {version}; add {} to PATH\n",
                home.path.join("bin").display()
            )
        }
        _ => {
            return Err(Error::usage(
                "unsupported arguments; run qleisliup-init --help",
            ));
        }
    };
    std::io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|e| Error::operational(format!("cannot write output: {e}")))
}

pub(crate) fn update(home: &Home, host: &str) -> Result<ExactVersion> {
    let mode = Mode::Update {
        running: std::env::current_exe().map_err(|e| Error::operational(e.to_string()))?,
        version: ExactVersion::parse(env!("CARGO_PKG_VERSION"))?,
    };
    preflight(home, host, &mode)?;
    run(home, host, mode, Source::official()?, &|_| Ok(()))
}

pub(crate) fn run<T: Transport + Clone + 'static>(
    home: &Home,
    host: &str,
    mode: Mode,
    source: Source<T>,
    boundary: &dyn Fn(Boundary) -> Result<()>,
) -> Result<ExactVersion> {
    preflight(home, host, &mode)?;
    let _lock = home.lock()?;
    preflight(home, host, &mode)?;
    crate::cleanup::stale(home, &_lock)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| Error::operational(e.to_string()))?;
    runtime.block_on(transaction(home, host, &mode, source, boundary))
}

fn preflight(home: &Home, host: &str, mode: &Mode) -> Result<()> {
    if !HOSTS.contains(&host) {
        return Err(Error::operational("unsupported manager distribution host"));
    }
    home.manager_identities()?;
    match mode {
        Mode::Bootstrap => match fs::symlink_metadata(home.path.join("bin")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::file(&home.path.join("bin"), e)),
            Ok(_) => Err(Error::operational(
                "bootstrap requires an absent bin destination; existing commands are preserved; use the installed manager's self update",
            )),
        },
        Mode::Update { running, version } => owned(home, host, running, version).map(|_| ()),
    }
}

fn owned(
    home: &Home,
    host: &str,
    running: &Path,
    version: &ExactVersion,
) -> Result<ArtifactIdentity> {
    let rejected = || {
        Error::operational(
            "self update requires the manager installed by qleisliup-init; for Cargo or package-manager installations, use the original installation method",
        )
    };
    files::directory(&home.path).map_err(|_| rejected())?;
    let bin = home.path.join("bin");
    files::directory(&bin).map_err(|_| rejected())?;
    let path = bin.join("qleisliup");
    // Compare actual paths while accepting invocation aliases of this manager.
    // The one-link and digest checks still reject external or stale executables.
    if path.canonicalize().map_err(|_| rejected())?
        != running.canonicalize().map_err(|_| rejected())?
    {
        return Err(rejected());
    }
    let marker: Ownership = required_json(&bin.join(MARKER)).map_err(|_| rejected())?;
    state::schema(marker.schema, &bin.join(MARKER))?;
    if marker.host != host || !marker.authenticated {
        return Err(rejected());
    }
    check_proxies(&bin)?;
    let identity = home
        .manager_identities()?
        .remove(&format!("{version}-{host}"))
        .ok_or_else(rejected)?;
    let (sha256, size) = hash_executable(&path)?;
    if sha256 != identity.artifact_sha256 || size != identity.artifact_size {
        return Err(Error::operational(
            "managed executable differs from its recorded version/identity; rerun the installed manager or restore it using the original installation method",
        ));
    }
    Ok(identity)
}

fn check_proxies(bin: &Path) -> Result<()> {
    for proxy in PROXIES {
        let path = bin.join(proxy);
        if fs::read_link(&path).map_err(|e| Error::file(&path, e))? != Path::new("qleisliup") {
            return Err(Error::file(
                &path,
                "expected a relative proxy symlink to qleisliup",
            ));
        }
    }
    Ok(())
}

async fn transaction<T: Transport + Clone + 'static>(
    home: &Home,
    host: &str,
    mode: &Mode,
    source: Source<T>,
    boundary: &dyn Fn(Boundary) -> Result<()>,
) -> Result<ExactVersion> {
    let client = Client::load(home, source).await?;
    boundary(Boundary::Metadata)?;
    // Update staging is inside bin, even if bin is on another filesystem.
    let parent = match mode {
        Mode::Bootstrap => home.path.clone(),
        Mode::Update { .. } => home.path.join("bin"),
    };
    let work = tempfile::Builder::new()
        .prefix(".qleisliup-manager-")
        .tempdir_in(&parent)
        .map_err(|e| Error::file(&parent, e))?;
    let channel_info = client.info("channels/qleisliup-stable.json", distribution::SMALL_TARGET)?;
    let channel_path = work.path().join("channel.json");
    client.download(&channel_info, &channel_path).await?;
    let channel: Channel = required_json(&channel_path)?;
    state::schema(channel.schema, &channel_path)?;
    let version = ExactVersion::channel(&channel.qleisliup)?;
    home.observe_manager(&version, channel_info.sha256)?;
    boundary(Boundary::Channel)?;
    if let Mode::Update { version: old, .. } = mode {
        if version < *old {
            return Err(Error::operational(
                "manager downgrade below the running version is forbidden",
            ));
        }
    }
    let manifest_info = client.info(
        &format!("qleisliup/{version}/manifest.json"),
        distribution::SMALL_TARGET,
    )?;
    let manifest_path = work.path().join("manifest.json");
    client.download(&manifest_info, &manifest_path).await?;
    let manifest: Manifest = required_json(&manifest_path)?;
    state::schema(manifest.schema, &manifest_path)?;
    if manifest.qleisliup != version.to_string() || manifest.artifacts.is_empty() {
        return Err(Error::operational(
            "manager manifest version or inventory mismatch",
        ));
    }
    for (target_host, artifact) in &manifest.artifacts {
        if !HOSTS.contains(&target_host.as_str())
            || !state::sha256(&artifact.sha256)
            || artifact.size == 0
            || artifact.size > MANAGER_LIMIT
            || artifact.target != format!("qleisliup/{version}/{target_host}/qleisliup")
        {
            return Err(Error::operational(
                "invalid manager artifact identity or host binding",
            ));
        }
    }
    let artifact = manifest
        .artifacts
        .get(host)
        .ok_or_else(|| Error::operational("manager manifest has no artifact for this host"))?;
    let target = client.info(&artifact.target, MANAGER_LIMIT)?;
    if target.sha256 != artifact.sha256 || target.size != artifact.size {
        return Err(Error::operational(
            "manager manifest disagrees with the authenticated artifact identity",
        ));
    }
    let identity = ArtifactIdentity {
        manifest_target: manifest_info.name,
        manifest_sha256: manifest_info.sha256,
        artifact_target: target.name.clone(),
        artifact_sha256: target.sha256.clone(),
        artifact_size: target.size,
    };
    identity.validate()?;
    identity.bind_manager(&version, host)?;
    check_manager_identity(
        &home.manager_identities()?,
        &format!("{version}-{host}"),
        &identity,
    )?;
    boundary(Boundary::Manifest)?;
    if let Mode::Update {
        running,
        version: old,
    } = mode
    {
        if version == *old {
            if owned(home, host, running, old)? != identity {
                return Err(Error::operational(
                    "installed manager identity differs from the authenticated target",
                ));
            }
            return Ok(version);
        }
    }
    let staged_bin = work.path().join("bin");
    files::create_directory(&staged_bin)?;
    let staged = staged_bin.join("qleisliup");
    client.download(&target, &staged).await?;
    boundary(Boundary::Download)?;
    set_executable(&staged)?;
    verify_version(&staged, &version, work.path(), Duration::from_secs(10))?;
    if hash_executable(&staged)? != (identity.artifact_sha256.clone(), identity.artifact_size) {
        return Err(Error::operational(
            "staged manager changed after authentication",
        ));
    }
    if let Mode::Bootstrap = mode {
        make_proxies(&staged_bin)?;
        let marker = Ownership {
            schema: 1,
            host: host.into(),
            authenticated: true,
        };
        let bytes = serde_json::to_vec(&marker).map_err(|e| Error::operational(e.to_string()))?;
        files::replace(&staged_bin, MARKER, &bytes, 0o600)?;
    }
    files::sync_directory(&staged_bin)?;
    boundary(Boundary::Verify)?;
    home.remember_manager(&format!("{version}-{host}"), &identity)?;
    boundary(Boundary::Identity)?;
    // Revalidate after the potentially long refresh/download and before rename.
    preflight(home, host, mode)?;
    boundary(Boundary::Publish)?;
    match mode {
        Mode::Bootstrap => crate::install::publish(&staged_bin, &home.path.join("bin"))?,
        Mode::Update { .. } => fs::rename(&staged, home.path.join("bin/qleisliup"))
            .map_err(|e| Error::file(&home.path.join("bin/qleisliup"), e))?,
    }
    boundary(Boundary::Published).map_err(|e| {
        Error::operational(format!(
            "manager {version} was published; completion was interrupted: {e}"
        ))
    })?;
    files::sync_directory(&parent).map_err(|e| {
        Error::operational(format!(
            "manager {version} was published; directory durability was not confirmed: {e}"
        ))
    })?;
    Ok(version)
}

#[cfg(unix)]
fn make_proxies(bin: &Path) -> Result<()> {
    for proxy in PROXIES {
        std::os::unix::fs::symlink("qleisliup", bin.join(proxy))
            .map_err(|e| Error::file(&bin.join(proxy), e))?;
    }
    Ok(())
}
#[cfg(not(unix))]
fn make_proxies(_: &Path) -> Result<()> {
    Err(Error::operational(
        "bootstrap requires a supported Unix host",
    ))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| Error::file(path, e))?;
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| Error::file(path, e))
}
#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<()> {
    Err(Error::operational(
        "bootstrap requires a supported Unix host",
    ))
}

fn hash_executable(path: &Path) -> Result<(String, u64)> {
    files::regular(path, true)?;
    let mut file = files::open_read(path).map_err(|e| Error::file(path, e))?;
    let metadata = file.metadata().map_err(|e| Error::file(path, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 || metadata.permissions().mode() & 0o7000 != 0 {
            return Err(Error::file(
                path,
                "manager must have one link and no privileged permission bits",
            ));
        }
    }
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MANAGER_LIMIT {
        return Err(Error::file(path, "invalid manager file type or size"));
    }
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0; 65536];
    loop {
        let length = file.read(&mut buffer).map_err(|e| Error::file(path, e))?;
        if length == 0 {
            break;
        }
        size += length as u64;
        if size > MANAGER_LIMIT {
            return Err(Error::file(path, "manager exceeds size limit"));
        }
        digest.update(&buffer[..length]);
    }
    Ok((distribution::hex(&digest.finalize()), size))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn verify_version(
    path: &Path,
    version: &ExactVersion,
    work: &Path,
    timeout: Duration,
) -> Result<()> {
    use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
    use std::os::unix::process::CommandExt;

    let mut magic = [0; 4];
    files::open_read(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|e| Error::file(path, e))?;
    // Reject scripts/text. The kernel and bounded version probe check actual
    // executability; full release CPU/linkage compatibility remains a host gate.
    if !matches!(
        magic,
        [0x7f, b'E', b'L', b'F']
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xca, 0xfe, 0xba, 0xbf]
    ) {
        return Err(Error::file(
            path,
            "manager target must be a native executable",
        ));
    }
    let out = work.join("version.stdout");
    let err = work.join("version.stderr");
    let stdout = File::create(&out).map_err(|e| Error::file(&out, e))?;
    let stderr = File::create(&err).map_err(|e| Error::file(&err, e))?;
    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .map_err(|e| Error::file(path, e))?;
    let group = Pid::from_child(&child);
    let deadline = Instant::now() + timeout;
    let check_output_limit = || {
        if fs::metadata(&out).map_err(|e| Error::file(&out, e))?.len() > 1024
            || fs::metadata(&err).map_err(|e| Error::file(&err, e))?.len() > 1024
        {
            Err(Error::operational(
                "manager version probe exceeded output limit",
            ))
        } else {
            Ok(())
        }
    };
    let result = (|| loop {
        check_output_limit()?;
        // Keep the leader waitable until its group has been terminated.
        // Reaping first would allow the process-group ID to be reused.
        match waitid(
            WaitId::Pid(group),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) | Err(rustix::io::Errno::INTR) => (),
            Err(error) => return Err(Error::file(path, error)),
        }
        if Instant::now() >= deadline {
            return Err(Error::operational("manager version probe timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    })();
    // A successful --version must not leave helpers running either. Signal the
    // whole group before reaping the leader, then validate the captured output.
    let cleanup = kill_process_group(group, Signal::KILL);
    if cleanup.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| Error::file(path, e));
    // Darwin excludes zombies from group signalling and can return EPERM for
    // a group containing only the exited leader. After reaping, accept that
    // case only if a non-destructive existence check confirms the group is gone.
    #[cfg(target_os = "macos")]
    let cleanup = match cleanup {
        Err(rustix::io::Errno::PERM)
            if status.is_ok()
                && rustix::process::test_kill_process_group(group)
                    == Err(rustix::io::Errno::SRCH) =>
        {
            Ok(())
        }
        other => other,
    };
    match cleanup {
        Ok(()) | Err(rustix::io::Errno::SRCH) => (),
        Err(error) => {
            return Err(Error::file(
                path,
                format!("cannot terminate manager version probe group: {error}"),
            ));
        }
    }
    result?;
    check_output_limit()?;
    let expected = format!("qleisliup {version}\n");
    if !status?.success()
        || files::read_optional(&out)?.as_deref() != Some(expected.as_bytes())
        || !files::read_optional(&err)?.is_some_and(|bytes| bytes.is_empty())
    {
        return Err(Error::operational(
            "manager version probe failed or reported a different version",
        ));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn verify_version(_: &Path, _: &ExactVersion, _: &Path, _: Duration) -> Result<()> {
    Err(Error::operational(
        "manager version probing requires a supported Unix host",
    ))
}
