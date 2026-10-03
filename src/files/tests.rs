use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::{Command, Stdio};

#[test]
fn existing_managed_directories_reject_shared_write_permissions_without_chmod() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("home");
    fs::create_dir(&home).unwrap();
    for mode in [0o770, 0o707, 0o777, 0o1777] {
        fs::set_permissions(&home, fs::Permissions::from_mode(mode)).unwrap();
        assert!(create_home(&home).is_err());
        assert!(create_directory(&home).is_err());
        assert!(Lock::home(&home).is_err());
        assert!(!home.join(".mutation-lock").exists());
        assert_eq!(
            fs::metadata(&home).unwrap().permissions().mode() & 0o7777,
            mode
        );
    }
    for mode in [0o700, 0o750, 0o755] {
        fs::set_permissions(&home, fs::Permissions::from_mode(mode)).unwrap();
        create_home(&home).unwrap();
        create_directory(&home).unwrap();
    }
}

#[test]
fn state_write_reclaims_only_reserved_files_in_bounded_batches() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path();
    let _lock = Lock::directory(path).unwrap();
    let untouched = [
        ".qleisliup-state-short.tmp",
        ".qleisliup-state-ABCDEFGHIJKLMNOP.tmp.backup",
        ".qleisliup-001-0.tmp",
        ".qleisliup-0-0.tmp",
        ".qleisliup-1-not-a-counter.tmp",
        "current.json",
        "identities.json",
        "links.json",
        "qleisli-toolchain.toml",
    ];
    for name in untouched {
        fs::write(path.join(name), b"preserved").unwrap();
    }
    for counter in 0..40 {
        fs::write(
            path.join(format!(".qleisliup-123-{counter}.tmp")),
            b"abandoned",
        )
        .unwrap();
    }
    let random = path.join(".qleisliup-state-ABCDEFGHIJKLMNOP.tmp");
    fs::write(&random, b"abandoned").unwrap();
    replace(path, "settings.json", b"new", 0o600).unwrap();
    let stale_count = || {
        fs::read_dir(path)
            .unwrap()
            .filter(|e| state_temporary(e.as_ref().unwrap().file_name().as_encoded_bytes()))
            .count()
    };
    assert_eq!(stale_count(), 9);
    replace(path, "settings.json", b"newer", 0o600).unwrap();
    assert_eq!(stale_count(), 0);
    for name in untouched {
        assert_eq!(fs::read(path.join(name)).unwrap(), b"preserved");
    }
    assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"newer");
}

#[test]
fn state_temporary_cleanup_rejects_links_and_special_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let outside = temporary.path().join("outside");
    fs::write(&outside, b"outside").unwrap();
    let path = temporary.path().join("project");
    fs::create_dir(&path).unwrap();
    let _lock = Lock::directory(&path).unwrap();
    let destination = path.join("qleisli-toolchain.toml");
    fs::write(&destination, b"committed").unwrap();
    let stale = path.join(".qleisliup-1-0.tmp");
    symlink(&outside, &stale).unwrap();
    assert!(replace(&path, "qleisli-toolchain.toml", b"new", 0o644).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"outside");
    assert_eq!(fs::read(&destination).unwrap(), b"committed");
    assert!(stale.is_symlink());
    fs::remove_file(&stale).unwrap();
    fs::create_dir(&stale).unwrap();
    assert!(replace(&path, "qleisli-toolchain.toml", b"new", 0o644).is_err());
    assert!(stale.is_dir());
}

#[test]
fn state_temporary_scan_limit_preserves_committed_state() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path();
    let _lock = Lock::directory(path).unwrap();
    fs::write(path.join("settings.json"), b"committed").unwrap();
    fs::write(path.join(".qleisliup-1-0.tmp"), b"stale").unwrap();
    for i in 0..4096 {
        fs::write(path.join(format!("unrelated-{i}")), b"").unwrap();
    }
    assert!(replace(path, "settings.json", b"new", 0o600).is_err());
    assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"committed");
    assert_eq!(fs::read(path.join(".qleisliup-1-0.tmp")).unwrap(), b"stale");
}

#[test]
fn interrupted_state_write_child() {
    let Some(path) = std::env::var_os("QLEISLIUP_UNIT_STATE_CRASH") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let _lock = Lock::directory(&path).unwrap();
    replace_with(&path, "settings.json", b"uncommitted", 0o600, true, || {
        fs::write(path.join("ready"), b"ready").unwrap();
        loop {
            std::thread::park();
        }
    })
    .unwrap();
}

#[test]
fn killed_state_writer_keeps_committed_state_and_restart_reclaims_temporary() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path();
    fs::write(path.join("settings.json"), b"committed").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "files::tests::interrupted_state_write_child",
            "--nocapture",
        ])
        .env("QLEISLIUP_UNIT_STATE_CRASH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !path.join("ready").exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("state writer exited early: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let ready = path.join("ready").exists();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "state writer never reached the rename boundary");
    let abandoned: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| state_temporary(p.file_name().unwrap().as_encoded_bytes()))
        .collect();
    assert_eq!(abandoned.len(), 1);
    assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"committed");
    let _lock = Lock::directory(path).unwrap();
    replace(path, "settings.json", b"restarted", 0o600).unwrap();
    assert!(!abandoned[0].exists());
    assert_eq!(fs::read(path.join("settings.json")).unwrap(), b"restarted");
}

#[test]
fn project_pin_directory_can_be_shared_writable() {
    let temporary = tempfile::tempdir().unwrap();
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o770)).unwrap();
    let _lock = Lock::directory(temporary.path()).unwrap();
    replace(temporary.path(), "qleisli-toolchain.toml", b"pin", 0o644).unwrap();
}
