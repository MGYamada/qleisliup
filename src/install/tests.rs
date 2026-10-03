//! Isolated TUF fixtures. These keys and repository constructors do not exist
//! in the production binary; there is no CLI or environment trust override.
use super::*;
use aws_lc_rs::signature::Ed25519KeyPair;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tough::sign::Sign;
use tough::{FilesystemTransport, TransportError, TransportStream};
use url::Url;

const HOST: &str = "aarch64-apple-darwin";
const FUTURE: &str = "2100-01-01T00:00:00Z";
const PAST: &str = "2000-01-01T00:00:00Z";

mod hardening;
mod manager;

fn digest(bytes: &[u8]) -> String {
    distribution::hex(&Sha256::digest(bytes))
}
fn key(seed: u8) -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
}
fn key_id(key: &Ed25519KeyPair) -> String {
    distribution::hex(key.tuf_key().key_id().unwrap().as_ref())
}
fn signed(value: Value, keys: &[&Ed25519KeyPair]) -> Vec<u8> {
    let mut canonical = Vec::new();
    value
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut canonical,
            olpc_cjson::CanonicalFormatter::new(),
        ))
        .unwrap();
    let signatures: Vec<_> = keys.iter().map(|key| json!({"keyid":key_id(key), "sig":distribution::hex(Ed25519KeyPair::sign(key, &canonical).as_ref())})).collect();
    serde_json::to_vec(&json!({"signed":value,"signatures":signatures})).unwrap()
}
fn target(bytes: &[u8]) -> Value {
    json!({"length":bytes.len(),"hashes":{"sha256":digest(bytes)}})
}
fn meta(bytes: &[u8], version: u64) -> Value {
    let mut value = target(bytes);
    value["version"] = json!(version);
    value
}
fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[derive(Clone, Debug)]
struct Counting {
    requests: Arc<AtomicU64>,
}
#[async_trait::async_trait]
impl Transport for Counting {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        FilesystemTransport.fetch(url).await
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    home: Home,
    metadata: PathBuf,
    targets: PathBuf,
    root: Vec<u8>,
    keys: [Ed25519KeyPair; 5],
    contents: BTreeMap<String, Vec<u8>>,
    requests: Arc<AtomicU64>,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let keys = [key(11), key(12), key(13), key(14), key(15)];
        let root = root(1, &keys[0], &[&keys[0]]);
        let mut fixture = Self {
            _temp: temp,
            home: Home {
                path: base.join("home"),
            },
            metadata: base.join("metadata"),
            targets: base.join("targets"),
            root,
            keys,
            contents: BTreeMap::new(),
            requests: Arc::default(),
        };
        fs::create_dir(&fixture.metadata).unwrap();
        fs::create_dir(&fixture.targets).unwrap();
        fixture.bundle("0.4.0", None);
        fixture.channel("0.4.0");
        fixture.publish(1, FUTURE, FUTURE, true);
        fixture
    }
    fn source(&self) -> Source<Counting> {
        Source {
            root: self.root.clone(),
            metadata: Url::from_directory_path(&self.metadata).unwrap(),
            targets: Url::from_directory_path(&self.targets).unwrap(),
            transport: Counting {
                requests: self.requests.clone(),
            },
        }
    }
    fn install(&self, selector: &str) -> Result<ExactVersion> {
        run(
            &self.home,
            &Selector::parse(selector)?,
            HOST,
            self.source(),
            &|_| Ok(()),
        )
    }
    fn channel(&mut self, version: &str) {
        self.contents.insert(
            "channels/stable.json".into(),
            serde_json::to_vec(&json!({"schema":1,"qleisli":version})).unwrap(),
        );
    }
    fn bundle(&mut self, version: &str, mutate: Option<fn(&mut Value)>) {
        let mut internal = json!({"schema":1,"qleisli":version,"std":version,"std_kind":"embedded","qargo":"0.2.1","qargo_checker_qleisli":"0.3.0","verifier":{"kind":"embedded-rust"},"compiler_commit":"a".repeat(40),"verifier_commit":"b".repeat(40),"host":HOST});
        if let Some(mutate) = mutate {
            mutate(&mut internal);
        }
        let archive = bundle_archive(version, &internal);
        self.contents.insert(
            format!("releases/{version}/{HOST}.tar.zst"),
            archive.clone(),
        );
        internal.as_object_mut().unwrap().remove("host");
        internal["artifacts"] = json!({HOST:{"target":format!("releases/{version}/{HOST}.tar.zst"),"sha256":digest(&archive),"size":archive.len()}});
        self.contents.insert(
            format!("releases/{version}/manifest.json"),
            serde_json::to_vec(&internal).unwrap(),
        );
    }
    fn replace_archive(&mut self, version: &str, archive: Vec<u8>) {
        let path = format!("releases/{version}/{HOST}.tar.zst");
        let manifest_path = format!("releases/{version}/manifest.json");
        let mut manifest: Value = serde_json::from_slice(&self.contents[&manifest_path]).unwrap();
        manifest["artifacts"][HOST]["sha256"] = json!(digest(&archive));
        manifest["artifacts"][HOST]["size"] = json!(archive.len());
        self.contents
            .insert(manifest_path, serde_json::to_vec(&manifest).unwrap());
        self.contents.insert(path, archive);
    }
    fn publish(&self, version: u64, expiry: &str, timestamp_expiry: &str, delegation: bool) {
        for (name, bytes) in &self.contents {
            write(
                &self.targets.join(format!("{}.{}", digest(bytes), name)),
                bytes,
            );
        }
        let mut delegated = BTreeMap::new();
        for (role, prefix, signing_key) in [
            ("releases", "releases/", &self.keys[2]),
            ("channels", "channels/", &self.keys[3]),
            ("qleisliup", "qleisliup/", &self.keys[4]),
        ] {
            let targets: BTreeMap<_, _> = self
                .contents
                .iter()
                .filter(|(n, _)| n.starts_with(prefix))
                .map(|(n, b)| (n, target(b)))
                .collect();
            let bytes = signed(
                json!({"_type":"targets","spec_version":"1.0.0","version":version,"expires":expiry,"targets":targets}),
                &[signing_key],
            );
            write(
                &self.metadata.join(format!("{version}.{role}.json")),
                &bytes,
            );
            delegated.insert(format!("{role}.json"), meta(&bytes, version));
        }
        let mut targets = json!({"_type":"targets","spec_version":"1.0.0","version":version,"expires":expiry,"targets":{}});
        if delegation {
            targets["delegations"] = json!({"keys":{key_id(&self.keys[2]):self.keys[2].tuf_key(), key_id(&self.keys[3]):self.keys[3].tuf_key(), key_id(&self.keys[4]):self.keys[4].tuf_key()},"roles":[
                {"name":"releases","keyids":[key_id(&self.keys[2])],"threshold":1,"terminating":true,"paths":["releases/*"]},
                {"name":"channels","keyids":[key_id(&self.keys[3])],"threshold":1,"terminating":true,"paths":["channels/*"]},
                {"name":"qleisliup","keyids":[key_id(&self.keys[4])],"threshold":1,"terminating":true,"paths":["qleisliup/*"]}
            ]});
        }
        let bytes = signed(targets, &[&self.keys[1]]);
        write(
            &self.metadata.join(format!("{version}.targets.json")),
            &bytes,
        );
        delegated.insert("targets.json".into(), meta(&bytes, version));
        let snapshot = signed(
            json!({"_type":"snapshot","spec_version":"1.0.0","version":version,"expires":expiry,"meta":delegated}),
            &[&self.keys[1]],
        );
        write(
            &self.metadata.join(format!("{version}.snapshot.json")),
            &snapshot,
        );
        let timestamp = signed(
            json!({"_type":"timestamp","spec_version":"1.0.0","version":version,"expires":timestamp_expiry,"meta":{"snapshot.json":meta(&snapshot,version)}}),
            &[&self.keys[1]],
        );
        write(&self.metadata.join("timestamp.json"), &timestamp);
    }
    fn final_path(&self, version: &str) -> PathBuf {
        self.home
            .path
            .join("toolchains")
            .join(format!("{version}-{HOST}"))
    }
    fn current(&self) -> PathBuf {
        let state = self.home.path.join("metadata/official");
        let pointer: Value =
            serde_json::from_slice(&fs::read(state.join("current.json")).unwrap()).unwrap();
        state
            .join("generations")
            .join(pointer["generation"].as_str().unwrap())
    }
}
fn root(version: u64, root_key: &Ed25519KeyPair, signing_keys: &[&Ed25519KeyPair]) -> Vec<u8> {
    let roles_key = key(12);
    let roles: BTreeMap<_,_> = ["root","targets","snapshot","timestamp"].into_iter().map(|role| (role, json!({"keyids":[key_id(if role == "root" {root_key} else {&roles_key})],"threshold":1}))).collect();
    signed(
        json!({"_type":"root","spec_version":"1.0.0","version":version,"expires":FUTURE,"consistent_snapshot":true,"keys":{key_id(root_key):root_key.tuf_key(),key_id(&roles_key):roles_key.tuf_key()},"roles":roles}),
        signing_keys,
    )
}

fn append(
    builder: &mut tar::Builder<Vec<u8>>,
    name: &str,
    bytes: &[u8],
    mode: u32,
    kind: tar::EntryType,
) {
    let mut header = tar::Header::new_ustar();
    header.set_size(bytes.len() as u64);
    header.set_mode(mode);
    header.set_entry_type(kind);
    header.set_cksum();
    builder.append_data(&mut header, name, bytes).unwrap();
}
fn bundle_archive(version: &str, manifest: &Value) -> Vec<u8> {
    let root = format!("{version}-{HOST}");
    let mut builder = tar::Builder::new(Vec::new());
    append(
        &mut builder,
        &format!("{root}/toolchain.json"),
        &serde_json::to_vec(manifest).unwrap(),
        0o644,
        tar::EntryType::Regular,
    );
    for name in ["qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"] {
        append(
            &mut builder,
            &format!("{root}/bin/{name}"),
            b"#!/bin/sh\nprintf '%s\\n' authenticated-fixture\n",
            0o755,
            tar::EntryType::Regular,
        );
    }
    for name in ["LICENSE", "NOTICE"] {
        append(
            &mut builder,
            &format!("{root}/{name}"),
            b"test-only",
            0o644,
            tar::EntryType::Regular,
        );
    }
    if manifest["std_kind"] == "directory" {
        append(
            &mut builder,
            &format!("{root}/std/"),
            b"",
            0o755,
            tar::EntryType::Directory,
        );
    }
    if manifest["verifier"]["kind"] == "external" {
        append(
            &mut builder,
            &format!("{root}/{}", manifest["verifier"]["path"].as_str().unwrap()),
            b"#!/bin/sh\nexit 0\n",
            0o755,
            tar::EntryType::Regular,
        );
    }
    zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap()
}
fn rejected<T: std::fmt::Debug>(result: Result<T>, message: &str) {
    let error = result.unwrap_err().to_string();
    assert!(error.contains(message), "{error}");
}

#[cfg(unix)]
#[test]
fn signed_archive_modes_are_normalized_before_publication() {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o001, 0o010, 0o777, 0o755] {
        let mut fixture = Fixture::new();
        let original = &fixture.contents[&format!("releases/0.4.0/{HOST}.tar.zst")];
        let decoded = zstd::stream::decode_all(original.as_slice()).unwrap();
        let mut builder = tar::Builder::new(Vec::new());
        for entry in tar::Archive::new(decoded.as_slice()).entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            let executable = path.contains("/bin/");
            append(
                &mut builder,
                &path,
                &bytes,
                if executable { mode } else { 0o666 },
                tar::EntryType::Regular,
            );
        }
        let archive =
            zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap();
        fixture.replace_archive("0.4.0", archive);
        fixture.publish(2, FUTURE, FUTURE, true);
        fixture.install("0.4.0").unwrap();
        let root = fixture.final_path("0.4.0");
        for name in ["qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"] {
            let path = root.join("bin").join(name);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                0o755
            );
            assert!(
                std::process::Command::new(path)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        for name in ["toolchain.json", "LICENSE", "NOTICE"] {
            assert_eq!(
                fs::metadata(root.join(name)).unwrap().permissions().mode() & 0o7777,
                0o644
            );
        }
    }
}

#[test]
fn stable_default_resolves_the_channel_after_acquiring_the_mutation_lock() {
    let mut fixture = Fixture::new();
    fixture.install("stable").unwrap();
    fixture.bundle("0.5.0", None);
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("0.5.0").unwrap();
    let lock = fixture.home.lock().unwrap();
    let (ready, started) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let home = &fixture.home;
        let operation = scope.spawn(|| {
            crate::selection::set_default_with(home, &Selector::Stable, HOST, || {
                ready.send(()).unwrap();
            })
        });
        // The operation has observed the old channel in its nonmutating preflight
        // and can only acquire the lock after this channel mutation is committed.
        started
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        home.observe_stable(&ExactVersion::parse("0.5.0").unwrap(), "a".repeat(64))
            .unwrap();
        drop(lock);
        assert_eq!(operation.join().unwrap().unwrap().to_string(), "0.5.0");
    });
    assert!(
        matches!(fixture.home.settings().unwrap().default, DefaultValue::Version(v) if v == "0.5.0")
    );
}

#[cfg(unix)]
#[test]
fn lifecycle_cleanup_reclaims_only_private_staging_and_preserves_committed_state() {
    use std::os::unix::fs::symlink;
    let mut fixture = Fixture::new();
    fixture.install("stable").unwrap();
    fixture.bundle("0.5.0", None);
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("0.5.0").unwrap();
    crate::selection::set_default(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST)
        .unwrap();
    let home = &fixture.home.path;
    write(&home.join("bin/qleisliup"), b"existing manager");
    write(
        &home.join("bin/.qleisliup-managed.json"),
        b"existing marker",
    );
    symlink("qleisliup", home.join("bin/qli")).unwrap();
    let private = [
        "toolchains/.transactions/install-AbCd01",
        "toolchains/.transactions/uninstall-AbCd02",
        "metadata/official/work-AbCd03",
        "metadata/official/generations/state-AbCd04",
        ".qleisliup-manager-AbCd05",
        "bin/.qleisliup-manager-AbCd06",
    ];
    for name in private {
        write(&home.join(name).join("payload/data"), b"abandoned");
    }
    let bootstrap = home.join(private[4]).join("bin");
    fs::create_dir(&bootstrap).unwrap();
    for name in ["qli", "qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"] {
        symlink("qleisliup", bootstrap.join(name)).unwrap();
    }
    write(&home.join(".qleisliup-manager-not-owned/sentinel"), b"keep");
    let protected = [
        home.join("settings.json"),
        home.join("identities.json"),
        home.join("channels.json"),
        home.join("metadata/official/current.json"),
        fixture.current().join("root.json"),
        home.join("bin/qleisliup"),
        home.join("bin/.qleisliup-managed.json"),
        fixture.final_path("0.4.0").join("toolchain.json"),
    ];
    let before: Vec<_> = protected.iter().map(|p| fs::read(p).unwrap()).collect();
    uninstall(&fixture.home, &ExactVersion::parse("0.5.0").unwrap(), HOST).unwrap();
    for name in private {
        assert!(!home.join(name).exists(), "left {name}");
    }
    for (path, bytes) in protected.iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    assert_eq!(
        fs::read_link(home.join("bin/qli")).unwrap(),
        Path::new("qleisliup")
    );
    assert!(home.join(".qleisliup-manager-not-owned/sentinel").exists());
}

#[cfg(unix)]
#[test]
fn lifecycle_cleanup_rejects_symlinks_special_files_and_invalid_pointers() {
    use std::os::unix::fs::symlink;
    for attack in 0..6 {
        let fixture = Fixture::new();
        fixture.install("stable").unwrap();
        let home = &fixture.home.path;
        let outside = home.parent().unwrap().join("outside");
        write(&outside.join("sentinel"), b"untouched");
        let stale = home.join("toolchains/.transactions/install-AbCd01");
        match attack {
            0 => symlink(&outside, &stale).unwrap(),
            1 => {
                fs::create_dir(&stale).unwrap();
                symlink(&outside, stale.join("escape")).unwrap();
            }
            2 => {
                fs::create_dir(&stale).unwrap();
                let socket = home.parent().unwrap().join("socket");
                let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
                fs::rename(socket, stale.join("socket")).unwrap();
            }
            3 => {
                let transactions = home.join("toolchains/.transactions");
                fs::remove_dir(&transactions).unwrap();
                symlink(&outside, transactions).unwrap();
            }
            4 => {
                write(&stale.join("data"), b"keep on invalid pointer");
                write(
                    &home.join("metadata/official/current.json"),
                    br#"{"schema":1,"generation":"../outside"}"#,
                );
            }
            5 => {
                let staging = home.join(".qleisliup-manager-AbCd02/bin");
                fs::create_dir_all(&staging).unwrap();
                symlink(&outside, staging.join("qli")).unwrap();
            }
            _ => unreachable!(),
        }
        let pointer = fs::read(home.join("metadata/official/current.json")).unwrap();
        let identity = fs::read(home.join("identities.json")).unwrap();
        assert!(
            uninstall(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST).is_err(),
            "accepted attack {attack}"
        );
        assert!(fixture.final_path("0.4.0").join("bin/qleisli").exists());
        assert_eq!(fs::read(home.join("identities.json")).unwrap(), identity);
        assert_eq!(
            fs::read(home.join("metadata/official/current.json")).unwrap(),
            pointer
        );
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"untouched");
        if attack == 4 {
            assert!(stale.join("data").exists());
        }
    }
}

#[test]
fn stale_cleanup_is_bounded_and_preserves_active_generation_aliases() {
    let fixture = Fixture::new();
    fixture.install("stable").unwrap();
    let home = &fixture.home.path;
    let active = fixture.current();
    let state = home.join("metadata/official");
    let renamed = state.join("generations/state-aBcD01");
    fs::rename(&active, &renamed).unwrap();
    let alias = state.join("generations/state-AbCd01");
    let generation = if alias.exists() {
        "state-AbCd01"
    } else {
        "state-aBcD01"
    };
    write(
        &state.join("current.json"),
        &serde_json::to_vec(&json!({"schema":1,"generation":generation})).unwrap(),
    );
    let before = fs::read(renamed.join("root.json")).unwrap();
    for index in 0..33 {
        write(
            &home.join(format!(".qleisliup-manager-{index:06}/data")),
            b"abandoned",
        );
    }
    let lock = fixture.home.lock().unwrap();
    crate::cleanup::stale(&fixture.home, &lock).unwrap();
    let count = || {
        fs::read_dir(home)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".qleisliup-manager-")
            })
            .count()
    };
    assert_eq!(count(), 1);
    crate::cleanup::stale(&fixture.home, &lock).unwrap();
    assert_eq!(count(), 0);
    assert_eq!(fs::read(renamed.join("root.json")).unwrap(), before);
}

#[test]
fn authenticated_install_receipt_offline_reuse_and_inspection() {
    let fixture = Fixture::new();
    assert_eq!(fixture.install("0.4.0").unwrap().to_string(), "0.4.0");
    let toolchain = Toolchain::release(
        &fixture.home,
        &ExactVersion::parse("0.4.0").unwrap(),
        HOST,
        &fixture.home.identities().unwrap(),
    )
    .unwrap();
    let text = crate::selection::show(&toolchain, &crate::selection::Source::CommandLine);
    assert!(text.contains("qargo checker Qleisli: 0.3.0"));
    assert!(text.contains("not revalidated"));
    assert!(toolchain.root.join(".qleisliup-receipt.json").is_file());
    assert!(!toolchain.root.join("std").exists());
    let count = fixture.requests.load(Ordering::Relaxed);
    fixture.install("0.4.0").unwrap();
    install(&fixture.home, &Selector::parse("0.4.0").unwrap(), HOST).unwrap();
    crate::selection::list(&fixture.home).unwrap();
    crate::selection::set_default(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST)
        .unwrap();
    assert_eq!(fixture.requests.load(Ordering::Relaxed), count);
    let process = std::process::Command::new(toolchain.executable("qli").unwrap())
        .output()
        .unwrap();
    assert!(process.status.success());
    assert_eq!(process.stdout, b"authenticated-fixture\n");
    rejected(
        uninstall(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST),
        "global default",
    );
    assert!(fixture.final_path("0.4.0").is_dir());
}

#[test]
fn stable_high_water_default_freeze_and_explicit_older_install() {
    let mut fixture = Fixture::new();
    fixture.install("stable").unwrap();
    let original = fixture.home.stable().unwrap();
    crate::selection::set_default(&fixture.home, &original, HOST).unwrap();
    fixture.bundle("0.5.0", None);
    fixture.channel("0.5.0");
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("stable").unwrap();
    assert_eq!(fixture.home.stable().unwrap().to_string(), "0.5.0");
    assert!(
        matches!(fixture.home.settings().unwrap().default,DefaultValue::Version(v) if v == "0.4.0")
    );
    fixture.bundle("0.3.0", None);
    fixture.channel("0.3.0");
    fixture.publish(3, FUTURE, FUTURE, true);
    rejected(fixture.install("stable"), "channel rollback");
    assert_eq!(fixture.home.stable().unwrap().to_string(), "0.5.0");
    fixture.install("0.3.0").unwrap();
    uninstall(&fixture.home, &ExactVersion::parse("0.3.0").unwrap(), HOST).unwrap();
    assert!(
        fixture
            .home
            .identities()
            .unwrap()
            .contains_key(&format!("0.3.0-{HOST}"))
    );
    fixture.install("0.3.0").unwrap();
}

#[test]
fn stable_observation_survives_a_later_failed_install() {
    let mut fixture = Fixture::new();
    fixture.channel("0.8.0");
    fixture.publish(2, FUTURE, FUTURE, true);
    assert!(fixture.install("stable").is_err());
    assert_eq!(fixture.home.stable().unwrap().to_string(), "0.8.0");
    assert!(!fixture.final_path("0.8.0").exists());
    rejected(ExactVersion::channel("1.0.0-rc.1"), "prerelease");
    rejected(ExactVersion::channel("1.0.0+build"), "prerelease");
}

#[test]
fn remembered_identity_rejects_republication_after_uninstall() {
    let mut fixture = Fixture::new();
    fixture.install("0.4.0").unwrap();
    uninstall(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST).unwrap();
    let before = fs::read(fixture.home.path.join("identities.json")).unwrap();
    fixture.bundle(
        "0.4.0",
        Some(|m| m["compiler_commit"] = json!("c".repeat(40))),
    );
    fixture.publish(2, FUTURE, FUTURE, true);
    rejected(fixture.install("0.4.0"), "republication");
    assert!(!fixture.final_path("0.4.0").exists());
    assert_eq!(
        fs::read(fixture.home.path.join("identities.json")).unwrap(),
        before
    );
}

#[test]
fn sequential_root_rotation_is_persistent_including_on_failed_refresh() {
    let fixture = Fixture::new();
    fixture.install("stable").unwrap();
    let second = key(21);
    let third = key(22);
    write(
        &fixture.metadata.join("2.root.json"),
        &root(2, &second, &[&fixture.keys[0], &second]),
    );
    write(
        &fixture.metadata.join("3.root.json"),
        &root(3, &third, &[&second, &third]),
    );
    fixture.publish(2, FUTURE, PAST, true);
    assert!(fixture.install("stable").is_err());
    let persisted: Value =
        serde_json::from_slice(&fs::read(fixture.current().join("root.json")).unwrap()).unwrap();
    assert_eq!(persisted["signed"]["version"], 3);
    fs::remove_file(fixture.metadata.join("2.root.json")).unwrap();
    fs::remove_file(fixture.metadata.join("3.root.json")).unwrap();
    fixture.publish(3, FUTURE, FUTURE, true);
    fixture.install("stable").unwrap(); // Restarts from version 3, not fixture root 1.
}

#[test]
fn root_threshold_and_skipped_version_are_enforced() {
    let fixture = Fixture::new();
    let next = key(21);
    write(
        &fixture.metadata.join("2.root.json"),
        &root(2, &next, &[&next]),
    );
    rejected(fixture.install("stable"), "TUF refresh rejected");
    let fixture = Fixture::new();
    write(
        &fixture.metadata.join("2.root.json"),
        &root(3, &next, &[&fixture.keys[0], &next]),
    );
    rejected(fixture.install("stable"), "TUF refresh rejected");
    assert!(!fixture.final_path("0.4.0").exists());
}

#[test]
fn expiry_signature_delegation_and_metadata_mix_match_fail_closed() {
    for case in 0..5 {
        let fixture = Fixture::new();
        match case {
            0 => fixture.publish(1, FUTURE, PAST, true),
            1 => fixture.publish(1, PAST, FUTURE, true),
            2 => {
                let mut timestamp: Value = serde_json::from_slice(
                    &fs::read(fixture.metadata.join("timestamp.json")).unwrap(),
                )
                .unwrap();
                timestamp["signatures"][0]["sig"] = json!("00".repeat(64));
                write(
                    &fixture.metadata.join("timestamp.json"),
                    &serde_json::to_vec(&timestamp).unwrap(),
                );
            }
            3 => fixture.publish(1, FUTURE, FUTURE, false),
            4 => write(
                &fixture.metadata.join("1.targets.json"),
                b"tampered snapshot-pinned metadata",
            ),
            _ => unreachable!(),
        }
        assert!(fixture.install("0.4.0").is_err(), "accepted case {case}");
        assert!(!fixture.final_path("0.4.0").exists());
    }
}

#[test]
fn rollback_and_corrupt_cached_state_are_rejected_after_restart() {
    let fixture = Fixture::new();
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("stable").unwrap();
    fixture.publish(1, FUTURE, FUTURE, true);
    rejected(fixture.install("stable"), "TUF refresh rejected");
    let cached: Value =
        serde_json::from_slice(&fs::read(fixture.current().join("timestamp.json")).unwrap())
            .unwrap();
    assert_eq!(cached["signed"]["version"], 2);
    write(
        &fixture.current().join("timestamp.json"),
        br#"{"signed":{}}"#,
    );
    rejected(fixture.install("stable"), "corrupt persisted TUF");
}

#[test]
fn target_final_digest_and_length_are_verified_before_extracting() {
    for case in 0..2 {
        let fixture = Fixture::new();
        let name = format!("releases/0.4.0/{HOST}.tar.zst");
        let original = &fixture.contents[&name];
        let mut corrupt = original.clone();
        if case == 0 {
            let last = corrupt.len() - 1;
            corrupt[last] ^= 1;
        } else {
            corrupt.pop();
        }
        write(
            &fixture
                .targets
                .join(format!("{}.{}", digest(original), name)),
            &corrupt,
        );
        assert!(fixture.install("0.4.0").is_err());
        assert!(!fixture.final_path("0.4.0").exists());
        assert!(fixture.home.identities().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(fixture.home.path.join("toolchains/.transactions"))
                .unwrap()
                .count(),
            0
        );
    }
}

#[test]
fn component_arrangements_and_inventory_are_validated() {
    let mut fixture = Fixture::new();
    fixture.bundle(
        "0.4.0",
        Some(|m| {
            m["std_kind"] = json!("directory");
            m["verifier"] = json!({"kind":"external","protocol":1,"path":"lib/verifier"});
        }),
    );
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("0.4.0").unwrap();
    assert!(fixture.final_path("0.4.0").join("std").is_dir());
    assert!(fixture.final_path("0.4.0").join("lib/verifier").is_file());
    for mutate in [
        (|m: &mut Value| m["std"] = json!("0.3.0")) as fn(&mut Value),
        |m| m["schema"] = json!(9),
        |m| m["verifier"] = json!({"kind":"external","protocol":0,"path":"lib/verifier"}),
    ] {
        let mut fixture = Fixture::new();
        fixture.bundle("0.4.0", Some(mutate));
        fixture.publish(2, FUTURE, FUTURE, true);
        assert!(fixture.install("0.4.0").is_err());
        assert!(!fixture.final_path("0.4.0").exists());
    }
}

#[test]
fn failure_boundaries_preserve_old_install_and_default_with_safe_retry() {
    for boundary in [
        Boundary::Metadata,
        Boundary::Channel,
        Boundary::Manifest,
        Boundary::Download,
        Boundary::Extract,
        Boundary::Receipt,
        Boundary::Identity,
        Boundary::Publish,
    ] {
        let mut fixture = Fixture::new();
        fixture.install("0.4.0").unwrap();
        let old = ExactVersion::parse("0.4.0").unwrap();
        crate::selection::set_default(&fixture.home, &old, HOST).unwrap();
        let receipt =
            fs::read(fixture.final_path("0.4.0").join(".qleisliup-receipt.json")).unwrap();
        fixture.bundle("0.5.0", None);
        fixture.channel("0.5.0");
        fixture.publish(2, FUTURE, FUTURE, true);
        let result = run(
            &fixture.home,
            &Selector::Stable,
            HOST,
            fixture.source(),
            &|b| {
                if b == boundary {
                    Err(Error::operational("injected boundary failure"))
                } else {
                    Ok(())
                }
            },
        );
        rejected(result, "injected boundary failure");
        assert!(!fixture.final_path("0.5.0").exists());
        assert_eq!(
            fs::read(fixture.final_path("0.4.0").join(".qleisliup-receipt.json")).unwrap(),
            receipt
        );
        assert!(
            matches!(fixture.home.settings().unwrap().default,DefaultValue::Version(v) if v == "0.4.0")
        );
        assert_eq!(
            fs::read_dir(fixture.home.path.join("toolchains/.transactions"))
                .unwrap()
                .count(),
            0
        );
        fixture.install("stable").unwrap();
    }
}

#[test]
fn concurrent_installs_serialize_and_never_replace_an_existing_release() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let home = fixture.home.clone();
    let first = std::thread::spawn(move || {
        run(
            &home,
            &Selector::parse("0.4.0").unwrap(),
            HOST,
            source,
            &|_| Ok(()),
        )
    });
    fixture.install("0.4.0").unwrap();
    first.join().unwrap().unwrap();
    assert_eq!(fixture.home.identities().unwrap().len(), 1);
    assert_eq!(
        fs::read_dir(fixture.home.path.join("toolchains/.transactions"))
            .unwrap()
            .count(),
        0
    );
    let collision = fixture.final_path("0.5.0");
    fs::create_dir(&collision).unwrap();
    write(&collision.join("keep"), b"existing");
    assert!(fixture.install("0.5.0").is_err());
    assert_eq!(fs::read(collision.join("keep")).unwrap(), b"existing");
}

#[test]
fn production_configuration_is_absent_and_cannot_fall_back_to_fixtures() {
    let fixture = Fixture::new();
    rejected(
        install(&fixture.home, &Selector::Stable, HOST),
        "production distribution is not configured",
    );
    assert!(!fixture.home.path.exists());
}

#[test]
fn dangerous_archives_and_extraction_limits_are_rejected() {
    let fixture = Fixture::new();
    let root = "0.4.0-aarch64-apple-darwin";
    for case in 0..11 {
        let temp = tempfile::tempdir().unwrap();
        let mut builder = tar::Builder::new(Vec::new());
        let name = format!("{root}/file");
        let kind = match case {
            1 => tar::EntryType::Symlink,
            2 => tar::EntryType::Link,
            3 => tar::EntryType::Fifo,
            4 => tar::EntryType::Char,
            5 => tar::EntryType::GNULongName,
            _ => tar::EntryType::Regular,
        };
        append(
            &mut builder,
            &name,
            b"payload",
            if case == 6 { 0o4755 } else { 0o644 },
            kind,
        );
        if case == 0 {
            append(
                &mut builder,
                &name,
                b"duplicate",
                0o644,
                tar::EntryType::Regular,
            );
        }
        if case == 7 {
            append(
                &mut builder,
                &format!("{root}/.qleisliup-receipt.json"),
                b"forged",
                0o644,
                tar::EntryType::Regular,
            );
        }
        if case == 8 {
            append(
                &mut builder,
                "other-root/file",
                b"outside",
                0o644,
                tar::EntryType::Regular,
            );
        }
        let mut tar = builder.into_inner().unwrap();
        if case == 9 {
            tar.extend_from_slice(b"hidden second archive");
        }
        let bytes = zstd::stream::encode_all(tar.as_slice(), 1).unwrap();
        let path = temp.path().join("archive");
        write(&path, &bytes);
        let payload = temp.path().join("payload");
        fs::create_dir(&payload).unwrap();
        let limits = if case == 10 {
            archive::Limits {
                expanded: 1024,
                file: 10,
                entries: 1,
            }
        } else {
            archive::Limits::default()
        };
        assert!(
            archive::extract(&path, &payload, root, limits).is_err(),
            "accepted archive case {case}"
        );
    }
    // Bypass tar's builder validation to exercise raw malicious paths.
    for name in [
        "/absolute/file",
        "../escape",
        "0.4.0-aarch64-apple-darwin/../escape",
        "0.4.0-aarch64-apple-darwin/./file",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut header = tar::Header::new_ustar();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        let mut raw = header.as_bytes().to_vec();
        raw.extend_from_slice(&[0; 1024]);
        let bytes = zstd::stream::encode_all(raw.as_slice(), 1).unwrap();
        let path = temp.path().join("archive");
        write(&path, &bytes);
        let payload = temp.path().join("payload");
        fs::create_dir(&payload).unwrap();
        assert!(
            archive::extract(&path, &payload, root, archive::Limits::default()).is_err(),
            "accepted {name}"
        );
        assert!(!temp.path().join("escape").exists());
    }
    // A dangerous archive with valid TUF signatures still cannot be installed.
    let mut fixture = fixture;
    let mut builder = tar::Builder::new(Vec::new());
    append(
        &mut builder,
        &format!("{root}/link"),
        b"",
        0o755,
        tar::EntryType::Symlink,
    );
    fixture.replace_archive(
        "0.4.0",
        zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap(),
    );
    fixture.publish(2, FUTURE, FUTURE, true);
    rejected(fixture.install("0.4.0"), "archive rejected");
    assert!(!fixture.final_path("0.4.0").exists());
}

#[test]
fn snapshot_rollback_with_a_new_timestamp_and_backward_clock_are_rejected() {
    let fixture = Fixture::new();
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("stable").unwrap();
    fixture.publish(1, FUTURE, FUTURE, true);
    let snapshot = fs::read(fixture.metadata.join("1.snapshot.json")).unwrap();
    let timestamp = signed(
        json!({"_type":"timestamp","spec_version":"1.0.0","version":3,"expires":FUTURE,"meta":{"snapshot.json":meta(&snapshot,1)}}),
        &[&fixture.keys[1]],
    );
    write(&fixture.metadata.join("timestamp.json"), &timestamp);
    rejected(fixture.install("stable"), "TUF refresh rejected");
    let cached: Value =
        serde_json::from_slice(&fs::read(fixture.current().join("snapshot.json")).unwrap())
            .unwrap();
    assert_eq!(cached["signed"]["version"], 2);
    // tough exposes no test clock setter. Deterministic expired/future dates and
    // its persisted clock mark test expiry/rollback with Safe still enabled.
    write(
        &fixture.current().join("latest_known_time.json"),
        br#""2100-01-01T00:00:00Z""#,
    );
    fixture.publish(4, FUTURE, FUTURE, true);
    rejected(fixture.install("stable"), "TUF refresh rejected");
}

#[test]
fn authenticated_manifest_mismatch_missing_inventory_and_binding_are_rejected() {
    for case in 0..5 {
        let mut fixture = Fixture::new();
        let path = "releases/0.4.0/manifest.json";
        let mut release: Value = serde_json::from_slice(&fixture.contents[path]).unwrap();
        match case {
            0 => release["qargo"] = json!("0.2.2"), // Archive still says 0.2.1.
            1 => release["artifacts"][HOST]["sha256"] = json!("f".repeat(64)),
            2 => release["artifacts"][HOST]["target"] = json!("releases/other/archive"),
            3 => {
                let mut internal = release.clone();
                internal.as_object_mut().unwrap().remove("artifacts");
                internal["host"] = json!("x86_64-apple-darwin");
                fixture.replace_archive("0.4.0", bundle_archive("0.4.0", &internal));
            }
            4 => {
                let mut internal = release.clone();
                internal.as_object_mut().unwrap().remove("artifacts");
                internal["host"] = json!(HOST);
                let mut builder = tar::Builder::new(Vec::new());
                append(
                    &mut builder,
                    &format!("0.4.0-{HOST}/toolchain.json"),
                    &serde_json::to_vec(&internal).unwrap(),
                    0o644,
                    tar::EntryType::Regular,
                );
                fixture.replace_archive(
                    "0.4.0",
                    zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap(),
                );
            }
            _ => unreachable!(),
        }
        if case < 3 {
            fixture
                .contents
                .insert(path.into(), serde_json::to_vec(&release).unwrap());
        }
        fixture.publish(2, FUTURE, FUTURE, true);
        assert!(fixture.install("0.4.0").is_err(), "accepted case {case}");
        assert!(!fixture.final_path("0.4.0").exists());
        assert!(fixture.home.identities().unwrap().is_empty());
    }
}

#[test]
fn individual_archive_limits_truncation_and_hidden_frames_fail() {
    let internal = json!({"std_kind":"embedded","verifier":{"kind":"embedded-rust"}});
    let bytes = bundle_archive("0.4.0", &internal);
    for case in 0..5 {
        let temp = tempfile::tempdir().unwrap();
        let mut bytes = bytes.clone();
        let mut limits = archive::Limits::default();
        match case {
            0 => limits.file = 1,
            1 => limits.entries = 1,
            2 => limits.expanded = 512,
            3 => {
                bytes.pop();
            }
            4 => bytes.extend_from_slice(
                &zstd::stream::encode_all(&b"hidden second frame"[..], 1).unwrap(),
            ),
            _ => unreachable!(),
        }
        let path = temp.path().join("archive");
        write(&path, &bytes);
        let payload = temp.path().join("payload");
        fs::create_dir(&payload).unwrap();
        assert!(
            archive::extract(&path, &payload, &format!("0.4.0-{HOST}"), limits).is_err(),
            "accepted case {case}"
        );
    }
}

#[test]
fn stable_refresh_rejects_republication_even_while_release_is_installed() {
    let mut fixture = Fixture::new();
    fixture.install("stable").unwrap();
    let old = fs::read(fixture.final_path("0.4.0").join("toolchain.json")).unwrap();
    fixture.bundle(
        "0.4.0",
        Some(|m| m["compiler_commit"] = json!("c".repeat(40))),
    );
    fixture.publish(2, FUTURE, FUTURE, true);
    rejected(fixture.install("stable"), "republication");
    assert_eq!(
        fs::read(fixture.final_path("0.4.0").join("toolchain.json")).unwrap(),
        old
    );
    // Exact reuse intentionally requires no online observation.
    fixture.install("0.4.0").unwrap();
}

#[test]
fn interrupted_install_child() {
    // Only this test harness consumes these variables; the product has no such
    // hooks or source override. The parent kills this process after persistence.
    let Some(base) = std::env::var_os("QLEISLIUP_UNIT_CRASH_FIXTURE") else {
        return;
    };
    let base = PathBuf::from(base);
    let home = Home {
        path: base.join("home"),
    };
    let source = Source {
        root: fs::read(base.join("initial-root.json")).unwrap(),
        metadata: Url::from_directory_path(base.join("metadata")).unwrap(),
        targets: Url::from_directory_path(base.join("targets")).unwrap(),
        transport: FilesystemTransport,
    };
    run(
        &home,
        &Selector::parse("0.5.0").unwrap(),
        HOST,
        source,
        &|boundary| {
            if boundary == Boundary::Identity {
                write(&base.join("ready"), b"durable");
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn killed_install_retains_old_release_default_and_metadata_then_retries() {
    let mut fixture = Fixture::new();
    fixture.install("0.4.0").unwrap();
    crate::selection::set_default(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST)
        .unwrap();
    fixture.bundle("0.5.0", None);
    fixture.publish(2, FUTURE, FUTURE, true);
    let base = fixture.home.path.parent().unwrap();
    write(&base.join("initial-root.json"), &fixture.root);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "install::tests::interrupted_install_child",
            "--nocapture",
        ])
        .env("QLEISLIUP_UNIT_CRASH_FIXTURE", base)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !base.join("ready").exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("crash fixture exited early: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let ready = base.join("ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "crash fixture did not reach identity persistence");
    assert!(!fixture.final_path("0.5.0").exists());
    assert!(
        fixture
            .home
            .identities()
            .unwrap()
            .contains_key(&format!("0.5.0-{HOST}"))
    );
    assert!(
        matches!(fixture.home.settings().unwrap().default,DefaultValue::Version(v) if v == "0.4.0")
    );
    let list = crate::selection::list(&fixture.home).unwrap();
    assert!(list.contains("0.4.0"));
    assert!(!list.contains("0.5.0"));
    let abandoned: Vec<_> = fs::read_dir(fixture.home.path.join("toolchains/.transactions"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(!abandoned.is_empty());
    let abandoned_work: Vec<_> = fs::read_dir(fixture.home.path.join("metadata/official"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("work-")
        })
        .collect();
    assert!(!abandoned_work.is_empty());
    fixture.install("0.5.0").unwrap();
    assert!(
        abandoned
            .iter()
            .chain(&abandoned_work)
            .all(|path| !path.exists())
    );
    let timestamp: Value =
        serde_json::from_slice(&fs::read(fixture.current().join("timestamp.json")).unwrap())
            .unwrap();
    assert_eq!(timestamp["signed"]["version"], 2);
}

#[derive(Clone, Debug)]
struct LateFailure;
#[async_trait::async_trait]
impl Transport for LateFailure {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let stream = FilesystemTransport.fetch(url.clone()).await?;
        if url.path().ends_with(".tar.zst") {
            let failure = futures_util::stream::once(std::future::ready(Err(
                TransportError::new_with_cause(
                    tough::TransportErrorKind::Other,
                    url,
                    "injected error after all target bytes",
                ),
            )));
            Ok(Box::pin(stream.chain(failure)))
        } else {
            Ok(stream)
        }
    }
}

#[test]
fn a_transport_error_after_all_target_bytes_prevents_extraction_and_publication() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let source = Source {
        root: source.root,
        metadata: source.metadata,
        targets: source.targets,
        transport: LateFailure,
    };
    rejected(
        run(
            &fixture.home,
            &Selector::parse("0.4.0").unwrap(),
            HOST,
            source,
            &|_| Ok(()),
        ),
        "injected error after all target bytes",
    );
    assert!(fixture.home.identities().unwrap().is_empty());
    assert!(!fixture.final_path("0.4.0").exists());
    assert_eq!(
        fs::read_dir(fixture.home.path.join("toolchains/.transactions"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn signed_metadata_lengths_do_not_override_the_absolute_transport_limit() {
    let fixture = Fixture::new();
    let mut targets: Value =
        serde_json::from_slice(&fs::read(fixture.metadata.join("1.targets.json")).unwrap())
            .unwrap();
    targets["signed"]["padding"] = json!("x".repeat(distribution::SMALL_TARGET as usize));
    let bytes = signed(targets["signed"].clone(), &[&fixture.keys[1]]);
    write(&fixture.metadata.join("1.targets.json"), &bytes);
    let mut snapshot: Value =
        serde_json::from_slice(&fs::read(fixture.metadata.join("1.snapshot.json")).unwrap())
            .unwrap();
    snapshot["signed"]["meta"]["targets.json"] = meta(&bytes, 1);
    let bytes = signed(snapshot["signed"].clone(), &[&fixture.keys[1]]);
    write(&fixture.metadata.join("1.snapshot.json"), &bytes);
    let mut timestamp: Value =
        serde_json::from_slice(&fs::read(fixture.metadata.join("timestamp.json")).unwrap())
            .unwrap();
    timestamp["signed"]["meta"]["snapshot.json"] = meta(&bytes, 1);
    write(
        &fixture.metadata.join("timestamp.json"),
        &signed(timestamp["signed"].clone(), &[&fixture.keys[1]]),
    );
    rejected(fixture.install("0.4.0"), "metadata byte limit exceeded");
    assert!(!fixture.final_path("0.4.0").exists());
}

#[test]
fn repeated_refreshes_do_not_accumulate_superseded_delegated_metadata() {
    let fixture = Fixture::new();
    for version in 1..=3 {
        fixture.publish(version, FUTURE, FUTURE, true);
        fixture.install("stable").unwrap();
        let names: Vec<_> = fs::read_dir(fixture.current())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            names.len() <= 7,
            "superseded delegated metadata accumulates after refresh {version}: {names:?}"
        );
    }
    // Snapshot state must still remember delegated versions after compaction.
    fixture.publish(4, FUTURE, FUTURE, true);
    let old_role = fs::read(fixture.metadata.join("2.releases.json")).unwrap();
    let mut snapshot: Value =
        serde_json::from_slice(&fs::read(fixture.metadata.join("4.snapshot.json")).unwrap())
            .unwrap();
    snapshot["signed"]["meta"]["releases.json"] = meta(&old_role, 2);
    let bytes = signed(snapshot["signed"].clone(), &[&fixture.keys[1]]);
    write(&fixture.metadata.join("4.snapshot.json"), &bytes);
    let mut timestamp: Value =
        serde_json::from_slice(&fs::read(fixture.metadata.join("timestamp.json")).unwrap())
            .unwrap();
    timestamp["signed"]["meta"]["snapshot.json"] = meta(&bytes, 4);
    write(
        &fixture.metadata.join("timestamp.json"),
        &signed(timestamp["signed"].clone(), &[&fixture.keys[1]]),
    );
    rejected(fixture.install("stable"), "TUF refresh rejected");
}

#[test]
fn archive_directory_aliases_are_rejected_when_the_filesystem_normalizes_names() {
    for (first, second) in [("data", "DATA"), ("caf\u{e9}", "cafe\u{301}")] {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join(first)).unwrap();
        let aliases = temporary.path().join(second).exists();
        let root = format!("0.4.0-{HOST}");
        let mut builder = tar::Builder::new(Vec::new());
        for directory in [first, second] {
            append(
                &mut builder,
                &format!("{root}/{directory}/"),
                b"",
                0o755,
                tar::EntryType::Directory,
            );
        }
        let bytes = zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap();
        let path = temporary.path().join("archive");
        write(&path, &bytes);
        let payload = temporary.path().join("payload");
        fs::create_dir(&payload).unwrap();
        let result = archive::extract(&path, &payload, &root, archive::Limits::default());
        if aliases {
            rejected(result, "archive");
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn archives_cannot_supply_filesystem_aliases_of_the_installer_receipt() {
    for kind in [tar::EntryType::Regular, tar::EntryType::Directory] {
        let mut fixture = Fixture::new();
        let case_probe = fixture._temp.path().join("case-probe");
        write(&case_probe, b"filesystem probe");
        let aliases = fixture._temp.path().join("CASE-PROBE").exists();
        let name = format!("releases/0.4.0/{HOST}.tar.zst");
        let raw = zstd::stream::decode_all(fixture.contents[&name].as_slice()).unwrap();
        let mut archive = tar::Archive::new(raw.as_slice());
        let mut builder = tar::Builder::new(Vec::new());
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let header = entry.header().clone();
            builder.append(&header, &mut entry).unwrap();
        }
        let path = format!("0.4.0-{HOST}/.QLEISLIUP-RECEIPT.JSON");
        let bytes = if kind.is_file() {
            b"supplied receipt".as_slice()
        } else {
            b""
        };
        append(&mut builder, &path, bytes, 0o755, kind);
        let bytes = zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap();
        fixture.replace_archive("0.4.0", bytes);
        fixture.publish(2, FUTURE, FUTURE, true);
        let result = fixture.install("0.4.0");
        if aliases {
            rejected(result, "installer receipt");
            assert!(!fixture.final_path("0.4.0").exists());
            assert!(fixture.home.identities().unwrap().is_empty());
        } else {
            result.unwrap(); // Distinct names are valid on a case-sensitive filesystem.
        }
    }
}

#[test]
fn nested_target_endpoints_do_not_inherit_metadata_size_limits() {
    let mut fixture = Fixture::new();
    let nested = fixture.metadata.join("targets");
    fs::rename(&fixture.targets, &nested).unwrap();
    fixture.targets = nested;
    let name = format!("releases/0.4.0/{HOST}.tar.zst");
    let raw = zstd::stream::decode_all(fixture.contents[&name].as_slice()).unwrap();
    let mut existing = tar::Archive::new(raw.as_slice());
    let mut builder = tar::Builder::new(Vec::new());
    for entry in existing.entries().unwrap() {
        let mut entry = entry.unwrap();
        let header = entry.header().clone();
        builder.append(&header, &mut entry).unwrap();
    }
    let mut generator = 1_u64;
    let padding: Vec<_> = (0..distribution::SMALL_TARGET + 4096)
        .map(|_| {
            generator ^= generator << 13;
            generator ^= generator >> 7;
            generator ^= generator << 17;
            generator as u8
        })
        .collect();
    append(
        &mut builder,
        &format!("0.4.0-{HOST}/data/padding"),
        &padding,
        0o644,
        tar::EntryType::Regular,
    );
    let bytes = zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap();
    assert!(bytes.len() as u64 > distribution::SMALL_TARGET);
    fixture.replace_archive("0.4.0", bytes);
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("0.4.0").unwrap();
    assert_eq!(
        fs::read(fixture.final_path("0.4.0").join("data/padding")).unwrap(),
        padding
    );
}
