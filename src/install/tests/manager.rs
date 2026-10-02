//! Stage 4 uses the same isolated TUF repository as toolchain installation.
use super::*;
use crate::manager::{self as lifecycle, Boundary as Step, Mode};
use std::os::unix::fs::{MetadataExt, symlink};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

const PROXIES: [&str; 6] = ["qli", "qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"];
type NativeCache = BTreeMap<(String, String), Vec<u8>>;
static NATIVE: OnceLock<Mutex<NativeCache>> = OnceLock::new();

fn native(version: &str, behavior: &str) -> Vec<u8> {
    let mut cache = NATIVE
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    cache
        .entry((version.into(), behavior.into()))
        .or_insert_with(|| {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("manager");
            let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .args(["--edition=2024", "--crate-name", "manager_fixture"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manager_tool.rs"))
                .arg("-o")
                .arg(&path)
                .env("QLEISLIUP_FIXTURE_MANAGER_VERSION", version)
                .env("QLEISLIUP_FIXTURE_MANAGER_BEHAVIOR", behavior)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            fs::read(path).unwrap()
        })
        .clone()
}
fn host() -> &'static str {
    crate::identity::current_host().unwrap()
}
fn manager_path(fixture: &Fixture) -> PathBuf {
    fixture.home.path.join("bin/qleisliup")
}
fn update_mode(fixture: &Fixture, version: &str) -> Mode {
    Mode::Update {
        running: manager_path(fixture),
        version: ExactVersion::parse(version).unwrap(),
    }
}
fn manager_release(fixture: &mut Fixture, version: &str, bytes: Vec<u8>) {
    let target = format!("qleisliup/{version}/{}/qleisliup", host());
    let manifest = json!({"schema":1,"qleisliup":version,"artifacts":{host():{
        "target":target,"sha256":digest(&bytes),"size":bytes.len()
    }}});
    fixture.contents.insert(target, bytes);
    fixture.contents.insert(
        format!("qleisliup/{version}/manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    );
    fixture.contents.insert(
        "channels/qleisliup-stable.json".into(),
        serde_json::to_vec(&json!({"schema":1,"qleisliup":version})).unwrap(),
    );
}
fn refresh(fixture: &mut Fixture, version: &str, metadata_version: u64) {
    manager_release(fixture, version, native(version, "valid"));
    fixture.publish(metadata_version, FUTURE, FUTURE, true);
}
fn apply(fixture: &Fixture, mode: Mode) -> Result<ExactVersion> {
    lifecycle::run(&fixture.home, host(), mode, fixture.source(), &|_| Ok(()))
}
fn bootstrapped() -> Fixture {
    let mut fixture = Fixture::new();
    refresh(&mut fixture, "0.1.0", 1);
    apply(&fixture, Mode::Bootstrap).unwrap();
    fixture
}
fn contents(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let bytes = if fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
            {
                fs::read_link(&path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec()
            } else {
                fs::read(&path).unwrap()
            };
            (name, bytes)
        })
        .collect()
}

#[test]
fn bootstrap_publishes_a_complete_manager_and_six_relative_proxies_only() {
    let mut fixture = Fixture::new();
    let shell = fixture.home.path.parent().unwrap().join("shell.rc");
    write(&shell, b"untouched shell settings");
    refresh(&mut fixture, "0.1.0", 1);
    let version = apply(&fixture, Mode::Bootstrap).unwrap();
    assert_eq!(version.to_string(), "0.1.0");
    assert_eq!(
        fs::read(manager_path(&fixture)).unwrap(),
        native("0.1.0", "valid")
    );
    for proxy in PROXIES {
        assert_eq!(
            fs::read_link(fixture.home.path.join("bin").join(proxy)).unwrap(),
            Path::new("qleisliup")
        );
        let output = Command::new(fixture.home.path.join("bin").join(proxy))
            .arg("--version")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"qleisliup 0.1.0\n");
    }
    assert!(!fixture.home.path.join("toolchains").exists());
    assert!(!fixture.home.path.join("settings.json").exists());
    assert!(!fixture.home.path.join("links.json").exists());
    assert!(fixture.home.identities().unwrap().is_empty());
    assert_eq!(fixture.home.manager_identities().unwrap().len(), 1);
    assert_eq!(fs::read(shell).unwrap(), b"untouched shell settings");
}

#[test]
fn self_update_preserves_proxies_and_toolchain_state_and_same_version_checks_republication() {
    let mut fixture = bootstrapped();
    fixture.install("stable").unwrap();
    crate::selection::set_default(&fixture.home, &ExactVersion::parse("0.4.0").unwrap(), HOST)
        .unwrap();
    let settings = fs::read(fixture.home.path.join("settings.json")).unwrap();
    let releases = fixture.home.identities().unwrap();
    let marker = fs::read(fixture.home.path.join("bin/.qleisliup-managed.json")).unwrap();
    let inodes: Vec<_> = PROXIES
        .iter()
        .map(|name| {
            fs::symlink_metadata(fixture.home.path.join("bin").join(name))
                .unwrap()
                .ino()
        })
        .collect();
    let old_inode = fs::metadata(manager_path(&fixture)).unwrap().ino();
    refresh(&mut fixture, "0.2.0", 2);
    apply(&fixture, update_mode(&fixture, "0.1.0")).unwrap();
    assert_ne!(
        fs::metadata(manager_path(&fixture)).unwrap().ino(),
        old_inode
    );
    assert_eq!(
        fs::read(manager_path(&fixture)).unwrap(),
        native("0.2.0", "valid")
    );
    assert_eq!(
        fs::read(fixture.home.path.join("settings.json")).unwrap(),
        settings
    );
    assert_eq!(fixture.home.identities().unwrap(), releases);
    assert_eq!(
        fs::read(fixture.home.path.join("bin/.qleisliup-managed.json")).unwrap(),
        marker
    );
    assert_eq!(fixture.home.manager_identities().unwrap().len(), 2);
    for (name, inode) in PROXIES.iter().zip(inodes) {
        assert_eq!(
            fs::symlink_metadata(fixture.home.path.join("bin").join(name))
                .unwrap()
                .ino(),
            inode
        );
    }
    assert_eq!(fixture.home.stable().unwrap().to_string(), "0.4.0");
    let now = fs::metadata(manager_path(&fixture)).unwrap().ino();
    apply(&fixture, update_mode(&fixture, "0.2.0")).unwrap();
    assert_eq!(fs::metadata(manager_path(&fixture)).unwrap().ino(), now);
    let requests = fixture.requests.load(Ordering::Relaxed);
    rejected(
        apply(&fixture, update_mode(&fixture, "0.1.0")),
        "differs from its recorded",
    );
    assert_eq!(fixture.requests.load(Ordering::Relaxed), requests);
    manager_release(&mut fixture, "0.2.0", native("0.2.0", "exit"));
    fixture.publish(3, FUTURE, FUTURE, true);
    rejected(
        apply(&fixture, update_mode(&fixture, "0.2.0")),
        "republication",
    );
}

#[test]
fn unsupported_external_and_occupied_destinations_fail_before_fetch_or_state_creation() {
    let fixture = Fixture::new();
    assert!(
        lifecycle::run(
            &fixture.home,
            "aarch64-unknown-linux-musl",
            Mode::Bootstrap,
            fixture.source(),
            &|_| Ok(())
        )
        .is_err()
    );
    let external = Mode::Update {
        running: fixture._temp.path().join("external"),
        version: ExactVersion::parse("0.1.0").unwrap(),
    };
    rejected(apply(&fixture, external), "original installation method");
    assert!(!fixture.home.path.exists());
    write(&fixture.home.path.join("bin/unrelated"), b"keep me");
    rejected(apply(&fixture, Mode::Bootstrap), "absent bin destination");
    assert_eq!(
        fs::read(fixture.home.path.join("bin/unrelated")).unwrap(),
        b"keep me"
    );
    assert_eq!(fixture.requests.load(Ordering::Relaxed), 0);
    assert!(!fixture.home.path.join(".mutation-lock").exists());
}

#[test]
fn ownership_rejects_modified_symlink_hardlink_or_misdirected_installations_offline() {
    for case in 0..6 {
        let fixture = bootstrapped();
        let path = manager_path(&fixture);
        match case {
            0 => write(&path, b"modified"),
            1 => {
                let saved = fixture.home.path.join("saved");
                fs::rename(&path, &saved).unwrap();
                symlink(&saved, &path).unwrap();
            }
            2 => fs::hard_link(&path, fixture.home.path.join("outside")).unwrap(),
            3 => write(
                &fixture.home.path.join("bin/.qleisliup-managed.json"),
                br#"{"schema":1,"host":"wrong","authenticated":true}"#,
            ),
            4 => {
                let proxy = fixture.home.path.join("bin/qli");
                fs::remove_file(&proxy).unwrap();
                symlink("../external", &proxy).unwrap();
            }
            5 => fs::remove_file(fixture.home.path.join("bin/.qleisliup-managed.json")).unwrap(),
            _ => unreachable!(),
        }
        let requests = fixture.requests.load(Ordering::Relaxed);
        assert!(
            apply(&fixture, update_mode(&fixture, "0.1.0")).is_err(),
            "accepted case {case}"
        );
        assert_eq!(fixture.requests.load(Ordering::Relaxed), requests);
    }
}

#[test]
fn managed_executable_path_aliases_allow_self_update() {
    for case in 0..3 {
        let mut fixture = bootstrapped();
        let manager = manager_path(&fixture);
        let running = match case {
            0 => {
                let alias = fixture._temp.path().join("entry");
                symlink(&manager, &alias).unwrap();
                alias
            }
            1 => {
                let alias = fixture._temp.path().join("alias-bin");
                symlink(manager.parent().unwrap(), &alias).unwrap();
                alias.join("qleisliup")
            }
            _ => {
                let alias = manager.with_file_name("QLEISLIUP");
                if !alias.exists() {
                    continue; // This filesystem preserves case distinctions.
                }
                alias
            }
        };
        refresh(&mut fixture, "0.2.0", 2);
        apply(
            &fixture,
            Mode::Update {
                running,
                version: ExactVersion::parse("0.1.0").unwrap(),
            },
        )
        .unwrap();
        assert_eq!(fs::read(manager).unwrap(), native("0.2.0", "valid"));
    }
}

#[test]
fn every_prepublication_failure_preserves_the_old_manager_and_allows_retry() {
    for failed in [
        Step::Metadata,
        Step::Channel,
        Step::Manifest,
        Step::Download,
        Step::Verify,
        Step::Identity,
        Step::Publish,
    ] {
        let mut fixture = bootstrapped();
        let bin = fixture.home.path.join("bin");
        let before = contents(&bin);
        refresh(&mut fixture, "0.2.0", 2);
        rejected(
            lifecycle::run(
                &fixture.home,
                host(),
                update_mode(&fixture, "0.1.0"),
                fixture.source(),
                &|step| {
                    if step == failed {
                        Err(Error::operational("injected manager failure"))
                    } else {
                        Ok(())
                    }
                },
            ),
            "injected manager failure",
        );
        assert_eq!(contents(&bin), before, "changed manager at {failed:?}");
        let output = Command::new(manager_path(&fixture))
            .arg("--version")
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"qleisliup 0.1.0\n");
        apply(&fixture, update_mode(&fixture, "0.1.0")).unwrap();
        assert_eq!(
            fs::read(manager_path(&fixture)).unwrap(),
            native("0.2.0", "valid")
        );
    }
}

#[test]
fn bootstrap_failure_never_publishes_partial_bin_and_can_retry() {
    for failed in [
        Step::Metadata,
        Step::Channel,
        Step::Manifest,
        Step::Download,
        Step::Verify,
        Step::Identity,
        Step::Publish,
    ] {
        let mut fixture = Fixture::new();
        refresh(&mut fixture, "0.1.0", 1);
        assert!(
            lifecycle::run(
                &fixture.home,
                host(),
                Mode::Bootstrap,
                fixture.source(),
                &|step| {
                    if step == failed {
                        Err(Error::operational("injected bootstrap failure"))
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        assert!(
            !fixture.home.path.join("bin").exists(),
            "partial bin at {failed:?}"
        );
        apply(&fixture, Mode::Bootstrap).unwrap();
    }
}

#[test]
fn authenticated_invalid_managers_and_manifest_bindings_preserve_the_old_executable() {
    for case in 0..10 {
        let mut fixture = bootstrapped();
        let before = contents(&fixture.home.path.join("bin"));
        let bytes = match case {
            0 => b"#!/bin/sh\necho qleisliup 0.2.0\n".to_vec(),
            1 => native("0.1.0", "valid"),
            2 => native("0.2.0", "noise"),
            3 => native("0.2.0", "exit"),
            9 => native("0.2.0", "hang"),
            _ => native("0.2.0", "valid"),
        };
        manager_release(&mut fixture, "0.2.0", bytes);
        if (4..9).contains(&case) {
            let name = "qleisliup/0.2.0/manifest.json";
            let mut manifest: Value = serde_json::from_slice(&fixture.contents[name]).unwrap();
            match case {
                4 => manifest["qleisliup"] = json!("0.3.0"),
                5 => manifest["artifacts"][host()]["target"] = json!("qleisliup/other/manager"),
                6 => manifest["artifacts"][host()]["sha256"] = json!("f".repeat(64)),
                7 => manifest["artifacts"][host()]["size"] = json!(128 * 1024 * 1024 + 1),
                8 => manifest["artifacts"] = json!({}),
                _ => unreachable!(),
            }
            fixture
                .contents
                .insert(name.into(), serde_json::to_vec(&manifest).unwrap());
        }
        fixture.publish(2, FUTURE, FUTURE, true);
        assert!(
            apply(&fixture, update_mode(&fixture, "0.1.0")).is_err(),
            "accepted case {case}"
        );
        assert_eq!(contents(&fixture.home.path.join("bin")), before);
        assert_eq!(fixture.home.manager_identities().unwrap().len(), 1);
    }
}

#[test]
fn manager_channel_downgrade_and_expired_tuf_metadata_preserve_the_manager() {
    let mut fixture = bootstrapped();
    refresh(&mut fixture, "0.2.0", 2);
    apply(&fixture, update_mode(&fixture, "0.1.0")).unwrap();
    let before = contents(&fixture.home.path.join("bin"));
    refresh(&mut fixture, "0.1.0", 3);
    rejected(
        apply(&fixture, update_mode(&fixture, "0.2.0")),
        "channel rollback",
    );
    fs::remove_file(fixture.home.path.join("channels.json")).unwrap();
    rejected(
        apply(&fixture, update_mode(&fixture, "0.2.0")),
        "manager downgrade",
    );
    refresh(&mut fixture, "0.3.0", 4);
    fixture.publish(4, PAST, PAST, true);
    rejected(
        apply(&fixture, update_mode(&fixture, "0.2.0")),
        "TUF refresh rejected",
    );
    assert_eq!(contents(&fixture.home.path.join("bin")), before);
}

#[test]
fn remembered_manager_identity_rejects_republication_after_interrupted_publication() {
    let mut fixture = bootstrapped();
    let before = contents(&fixture.home.path.join("bin"));
    refresh(&mut fixture, "0.2.0", 2);
    assert!(
        lifecycle::run(
            &fixture.home,
            host(),
            update_mode(&fixture, "0.1.0"),
            fixture.source(),
            &|step| {
                if step == Step::Identity {
                    Err(Error::operational("interrupted"))
                } else {
                    Ok(())
                }
            }
        )
        .is_err()
    );
    assert_eq!(fixture.home.manager_identities().unwrap().len(), 2);
    manager_release(&mut fixture, "0.2.0", native("0.2.0", "exit"));
    fixture.publish(3, FUTURE, FUTURE, true);
    rejected(
        apply(&fixture, update_mode(&fixture, "0.1.0")),
        "republication",
    );
    assert_eq!(contents(&fixture.home.path.join("bin")), before);
}

#[derive(Clone, Debug)]
struct LateManagerFailure;
#[async_trait::async_trait]
impl Transport for LateManagerFailure {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let stream = FilesystemTransport.fetch(url.clone()).await?;
        if url.path().ends_with("/qleisliup") {
            Ok(Box::pin(stream.chain(futures_util::stream::once(
                std::future::ready(Err(TransportError::new_with_cause(
                    tough::TransportErrorKind::Other,
                    url,
                    "late manager transport failure",
                ))),
            ))))
        } else {
            Ok(stream)
        }
    }
}

#[test]
fn late_manager_stream_errors_do_not_execute_or_publish_the_target() {
    let mut fixture = bootstrapped();
    let before = contents(&fixture.home.path.join("bin"));
    refresh(&mut fixture, "0.2.0", 2);
    let source = fixture.source();
    let source = Source {
        root: source.root,
        metadata: source.metadata,
        targets: source.targets,
        transport: LateManagerFailure,
    };
    rejected(
        lifecycle::run(
            &fixture.home,
            host(),
            update_mode(&fixture, "0.1.0"),
            source,
            &|_| Ok(()),
        ),
        "late manager transport failure",
    );
    assert_eq!(contents(&fixture.home.path.join("bin")), before);
    assert_eq!(fixture.home.manager_identities().unwrap().len(), 1);
}

#[test]
fn interrupted_manager_child() {
    let Some(base) = std::env::var_os("QLEISLIUP_UNIT_MANAGER_CRASH") else {
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
    lifecycle::run(
        &home,
        host(),
        Mode::Update {
            running: home.path.join("bin/qleisliup"),
            version: ExactVersion::parse("0.1.0").unwrap(),
        },
        source,
        &|step| {
            if step == Step::Identity {
                write(&base.join("manager-ready"), b"durable");
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
fn killed_update_retains_old_manager_and_retries_with_persisted_identity() {
    let mut fixture = bootstrapped();
    let before = contents(&fixture.home.path.join("bin"));
    refresh(&mut fixture, "0.2.0", 2);
    let base = fixture.home.path.parent().unwrap();
    write(&base.join("initial-root.json"), &fixture.root);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "install::tests::manager::interrupted_manager_child",
            "--nocapture",
        ])
        .env("QLEISLIUP_UNIT_MANAGER_CRASH", base)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !base.join("manager-ready").exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("manager child exited early: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let ready = base.join("manager-ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "manager child did not reach durable identity");
    // Abandoned private staging is permitted; final manager/proxies remain old.
    assert_eq!(
        fs::read(manager_path(&fixture)).unwrap(),
        before["qleisliup"]
    );
    for proxy in PROXIES {
        assert_eq!(
            fs::read_link(fixture.home.path.join("bin").join(proxy)).unwrap(),
            Path::new("qleisliup")
        );
    }
    assert_eq!(fixture.home.manager_identities().unwrap().len(), 2);
    apply(&fixture, update_mode(&fixture, "0.1.0")).unwrap();
    assert_eq!(
        fs::read(manager_path(&fixture)).unwrap(),
        native("0.2.0", "valid")
    );
}

#[test]
fn concurrent_updates_publish_once_and_reject_a_stale_running_version() {
    let mut fixture = bootstrapped();
    refresh(&mut fixture, "0.2.0", 2);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut children = Vec::new();
    for _ in 0..2 {
        let home = fixture.home.clone();
        let source = fixture.source();
        let mode = update_mode(&fixture, "0.1.0");
        let barrier = barrier.clone();
        children.push(std::thread::spawn(move || {
            barrier.wait();
            lifecycle::run(&home, host(), mode, source, &|_| Ok(()))
        }));
    }
    let results: Vec<_> = children
        .into_iter()
        .map(|child| child.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        fs::read(manager_path(&fixture)).unwrap(),
        native("0.2.0", "valid")
    );
    assert_eq!(fixture.home.manager_identities().unwrap().len(), 2);
    apply(&fixture, update_mode(&fixture, "0.2.0")).unwrap();
}

#[test]
fn failure_after_rename_reports_publication_and_needs_no_version_pointer_repair() {
    let mut fixture = bootstrapped();
    refresh(&mut fixture, "0.2.0", 2);
    rejected(
        lifecycle::run(
            &fixture.home,
            host(),
            update_mode(&fixture, "0.1.0"),
            fixture.source(),
            &|step| {
                if step == Step::Published {
                    Err(Error::operational("injected completion failure"))
                } else {
                    Ok(())
                }
            },
        ),
        "manager 0.2.0 was published",
    );
    assert_eq!(
        fs::read(manager_path(&fixture)).unwrap(),
        native("0.2.0", "valid")
    );
    apply(&fixture, update_mode(&fixture, "0.2.0")).unwrap();
}
