//! TUF is the only authority for downloaded distribution bytes.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tough::{
    Repository, RepositoryLoader, TargetName, Transport, TransportError, TransportErrorKind,
    TransportStream,
};
use url::Url;

use crate::error::{Error, Result};
use crate::{files, state::Home};

pub(crate) const SMALL_TARGET: u64 = 1024 * 1024;
pub(crate) const ARCHIVE_TARGET: u64 = 512 * 1024 * 1024;
// tough 0.24.0 reads only these persistent records on the next refresh.
// Delegated role versions remain protected by the signed snapshot; their bytes
// are re-fetched and verified, so versioned delegated cache files need not grow
// across refreshes. Validate the complete working store before compacting it.
const PERSISTENT_ROLES: [&str; 5] = [
    "root.json",
    "timestamp.json",
    "snapshot.json",
    "targets.json",
    "latest_known_time.json",
];

pub(crate) struct Source<T> {
    pub(crate) root: Vec<u8>,
    pub(crate) metadata: Url,
    pub(crate) targets: Url,
    pub(crate) transport: T,
}

impl Source<tough::DefaultTransport> {
    pub(crate) fn official() -> Result<Self> {
        Err(Error::operational(
            "production distribution is not configured: metadata/target URLs and an initial trusted TUF root must be established separately",
        ))
    }
}

// Bound metadata even when a signed snapshot supplies a larger length than
// tough's configured fallback limit. Clones share the refresh-wide budget.
#[derive(Clone, Debug)]
struct Bounded<T> {
    inner: T,
    metadata_phase: Arc<AtomicBool>,
    bytes: Arc<AtomicU64>,
    requests: Arc<AtomicU64>,
}

#[async_trait]
impl<T: Transport + Clone + 'static> Transport for Bounded<T> {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        if self.requests.fetch_add(1, Ordering::Relaxed) >= 2048 {
            return Err(TransportError::new_with_cause(
                TransportErrorKind::Other,
                url,
                "refresh request limit exceeded",
            ));
        }
        // tough 0.24.0 eagerly loads metadata before returning Repository; all
        // subsequent Client reads are targets. Classify by that boundary rather
        // than URL prefixes, which can legally overlap or be identical.
        let metadata = self.metadata_phase.load(Ordering::Acquire);
        let stream = self.inner.fetch(url.clone()).await?;
        if !metadata {
            return Ok(stream);
        }
        let budget = self.bytes.clone();
        let mut length = 0_u64;
        Ok(Box::pin(stream.map(move |item| {
            let bytes = item?;
            length = length.saturating_add(bytes.len() as u64);
            let total = budget.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            if length > SMALL_TARGET || total.saturating_add(bytes.len() as u64) > 64 * SMALL_TARGET
            {
                Err(TransportError::new_with_cause(
                    TransportErrorKind::Other,
                    &url,
                    "TUF metadata byte limit exceeded",
                ))
            } else {
                Ok(bytes)
            }
        })))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Current {
    schema: u32,
    generation: String,
}

pub(crate) fn current_generation(state: &Path) -> Result<Option<String>> {
    let pointer = state.join("current.json");
    let Some(bytes) = files::read_optional(&pointer)? else {
        return Ok(None);
    };
    let current: Current = serde_json::from_slice(&bytes).map_err(|e| Error::file(&pointer, e))?;
    crate::state::schema(current.schema, &pointer)?;
    if !current.generation.starts_with("state-")
        || !current
            .generation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(Error::file(&pointer, "invalid metadata generation"));
    }
    Ok(Some(current.generation))
}

pub(crate) struct Client {
    repository: Repository,
    work: TempDir,
    state: PathBuf,
}

impl Client {
    // Call only while holding the home mutation lock. tough writes its datastore
    // directly; keep that mutable working copy private and commit immutable
    // snapshots through a durable pointer, including authenticated advances on
    // failed refreshes. A process killed inside tough cannot corrupt old state.
    pub(crate) async fn load<T: Transport + Clone + 'static>(
        home: &Home,
        source: Source<T>,
    ) -> Result<Self> {
        let metadata = home.path.join("metadata");
        files::create_directory(&metadata)?;
        let state = metadata.join("official");
        files::create_directory(&state)?;
        files::create_directory(&state.join("generations"))?;
        let work = tempfile::Builder::new()
            .prefix("work-")
            .tempdir_in(&state)
            .map_err(|e| Error::file(&state, e))?;
        if let Some(generation) = current_generation(&state)? {
            copy_store(&state.join("generations").join(generation), work.path())?;
        } else {
            if source.root.len() as u64 > SMALL_TARGET {
                return Err(Error::operational("trusted root exceeds metadata limit"));
            }
            fs::write(work.path().join("root.json"), &source.root)
                .map_err(|e| Error::file(work.path(), e))?;
        }
        let root = files::read_optional(&work.path().join("root.json"))?
            .ok_or_else(|| Error::operational("persisted trusted root is missing"))?;
        let metadata_phase = Arc::new(AtomicBool::new(true));
        let transport = Bounded {
            inner: source.transport,
            metadata_phase: metadata_phase.clone(),
            bytes: Arc::default(),
            requests: Arc::default(),
        };
        let loaded = RepositoryLoader::new(&root, source.metadata, source.targets)
            .datastore(work.path())
            .transport(transport)
            .limits(tough::Limits {
                max_root_size: SMALL_TARGET,
                max_timestamp_size: SMALL_TARGET,
                max_snapshot_size: SMALL_TARGET,
                max_targets_size: SMALL_TARGET,
                max_root_updates: 1024,
            })
            .expiration_enforcement(tough::ExpirationEnforcement::Safe)
            .load()
            .await;
        metadata_phase.store(false, Ordering::Release);
        checkpoint(&state, work.path())?;
        let repository =
            loaded.map_err(|e| Error::operational(format!("TUF refresh rejected: {e}")))?;
        Ok(Self {
            repository,
            work,
            state,
        })
    }

    pub(crate) fn info(&self, name: &str, limit: u64) -> Result<TargetInfo> {
        crate::state::relative_path(name)?;
        let target_name = TargetName::new(name).map_err(tuf_error)?;
        let target = self
            .repository
            .targets()
            .signed
            .find_target(&target_name, false)
            .map_err(tuf_error)?;
        if target.length == 0 || target.length > limit {
            return Err(Error::operational(format!(
                "target {name:?} has an invalid or oversized length"
            )));
        }
        Ok(TargetInfo {
            name: name.to_owned(),
            sha256: hex(target.hashes.sha256.as_ref()),
            size: target.length,
        })
    }

    pub(crate) async fn download(&self, info: &TargetInfo, destination: &Path) -> Result<()> {
        let result = self.download_inner(info, destination).await;
        // Preserve tough's last-observed time without mutating a committed generation.
        checkpoint(&self.state, self.work.path())?;
        result
    }

    async fn download_inner(&self, info: &TargetInfo, destination: &Path) -> Result<()> {
        let name = TargetName::new(&info.name).map_err(tuf_error)?;
        let stream = self
            .repository
            .read_target(&name)
            .await
            .map_err(tuf_error)?
            .ok_or_else(|| Error::operational("authenticated target is absent"))?;
        let mut stream = std::pin::pin!(stream);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(destination)
            .map_err(|e| Error::file(destination, e))?;
        let mut digest = Sha256::new();
        let mut size = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(tuf_error)?;
            size = size
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| Error::operational("target length overflow"))?;
            if size > info.size {
                return Err(Error::operational("target exceeds authenticated length"));
            }
            digest.update(&chunk);
            file.write_all(&chunk)
                .map_err(|e| Error::file(destination, e))?;
        }
        if size != info.size || hex(&digest.finalize()) != info.sha256 {
            return Err(Error::operational("target digest/length mismatch"));
        }
        file.sync_all().map_err(|e| Error::file(destination, e))
    }
}

pub(crate) struct TargetInfo {
    pub(crate) name: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| [DIGITS[(b >> 4) as usize], DIGITS[(b & 15) as usize]])
        .map(char::from)
        .collect()
}
fn tuf_error(error: impl std::fmt::Display) -> Error {
    Error::operational(format!("TUF target rejected: {error}"))
}

fn copy_store(source: &Path, destination: &Path) -> Result<()> {
    files::directory(source)?;
    let mut count = 0;
    let mut total = 0_u64;
    for entry in fs::read_dir(source).map_err(|e| Error::file(source, e))? {
        let entry = entry.map_err(|e| Error::file(source, e))?;
        count += 1;
        if count > 2048 {
            return Err(Error::file(source, "metadata file count limit exceeded"));
        }
        let bytes = files::read_optional(&entry.path())?
            .ok_or_else(|| Error::file(&entry.path(), "metadata disappeared"))?;
        total += bytes.len() as u64;
        if total > 64 * SMALL_TARGET {
            return Err(Error::file(
                source,
                "persisted metadata byte limit exceeded",
            ));
        }
        validate_cached(&entry.path(), &bytes)?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| PERSISTENT_ROLES.contains(&name))
        {
            continue;
        }
        let path = destination.join(entry.file_name());
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|e| Error::file(&path, e))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| Error::file(&path, e))?;
    }
    files::sync_directory(destination)
}

fn checkpoint(state: &Path, work: &Path) -> Result<()> {
    let generations = state.join("generations");
    let snapshot = tempfile::Builder::new()
        .prefix("state-")
        .tempdir_in(&generations)
        .map_err(|e| Error::file(&generations, e))?;
    copy_store(work, snapshot.path())?;
    let generation = snapshot
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::operational("invalid generated metadata name"))?
        .to_owned();
    let _kept = snapshot.keep();
    files::sync_directory(&generations)?;
    let bytes = serde_json::to_vec(&Current {
        schema: 1,
        generation,
    })
    .map_err(|e| Error::operational(e.to_string()))?;
    files::replace(state, "current.json", &bytes, 0o600)?;
    crate::cleanup::metadata_generations(state).map_err(|error| {
        Error::operational(format!(
            "metadata checkpoint published; stale cleanup failed: {error}"
        ))
    })
}

fn validate_cached(path: &Path, bytes: &[u8]) -> Result<()> {
    use tough::schema::{Root, Signed, Snapshot, Targets, Timestamp};
    // tough intentionally ignores malformed optional cached JSON. Do not let a
    // corrupt persisted record silently reset a rollback or clock high-water mark.
    let result = match path.file_name().and_then(|n| n.to_str()) {
        Some("root.json") => serde_json::from_slice::<Signed<Root>>(bytes).map(|_| ()),
        Some("timestamp.json") => serde_json::from_slice::<Signed<Timestamp>>(bytes).map(|_| ()),
        Some("snapshot.json") => serde_json::from_slice::<Signed<Snapshot>>(bytes).map(|_| ()),
        Some("latest_known_time.json") => {
            serde_json::from_slice::<jiff::Timestamp>(bytes).map(|_| ())
        }
        Some(name) if name.ends_with(".json") => {
            serde_json::from_slice::<Signed<Targets>>(bytes).map(|_| ())
        }
        _ => return Err(Error::file(path, "unexpected metadata datastore entry")),
    };
    result.map_err(|e| Error::file(path, format!("corrupt persisted TUF metadata: {e}")))
}
