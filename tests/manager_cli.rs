//! Public lifecycle CLI boundaries. Local records here are synthetic consistency
//! fixtures; signed bootstrap/update transactions are covered by internal tests.
#![cfg(unix)]

use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::{Command, Output};

fn run(executable: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(executable)
        .args(args)
        .env("QLEISLIUP_HOME", home)
        .env(
            "QLEISLIUP_TOOLCHAIN",
            "invalid selector ignored by manager lifecycle",
        )
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .output()
        .unwrap()
}
fn failed(output: Output, status: i32, diagnostic: &str) {
    assert_eq!(output.status.code(), Some(status));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(diagnostic),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn write_json(path: &Path, value: serde_json::Value) {
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn bootstrap_help_version_and_rejections_do_not_create_state() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("missing");
    let init = Path::new(env!("CARGO_BIN_EXE_qleisliup-init"));
    let version = run(init, &home, &["--version"]);
    assert!(version.status.success());
    assert_eq!(
        version.stdout,
        format!("qleisliup-init {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    let help = run(init, Path::new("relative-invalid-home"), &["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("not configured"));
    failed(
        run(init, &home, &[]),
        1,
        "production distribution is not configured",
    );
    for args in [
        vec!["install"],
        vec!["--help", "extra"],
        vec!["+stable"],
        vec!["--trust-root", "/untrusted"],
    ] {
        failed(run(init, &home, &args), 2, "unsupported arguments");
    }
    assert!(!home.exists());
    let proxy = temporary.path().join("qli");
    symlink(init, &proxy).unwrap();
    let output = run(&proxy, &home, &["--version"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, version.stdout); // Compile-time identity remains bootstrap.
}

#[test]
fn external_self_update_is_operationally_rejected_and_never_creates_a_home() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("missing");
    let manager = Path::new(env!("CARGO_BIN_EXE_qleisliup"));
    failed(
        run(manager, &home, &["self", "update"]),
        1,
        "original installation method",
    );
    for args in [
        vec!["self"],
        vec!["self", "update", "extra"],
        vec!["+stable", "self", "update"],
    ] {
        assert_eq!(run(manager, &home, &args).status.code(), Some(2));
    }
    assert!(!home.exists());
}

#[test]
fn managed_cli_fails_closed_without_production_trust_and_proxies_remain_usable() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let home = base.join("home");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let manager = bin.join("qleisliup");
    fs::copy(env!("CARGO_BIN_EXE_qleisliup"), &manager).unwrap();
    for tool in ["qli", "qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"] {
        symlink("qleisliup", bin.join(tool)).unwrap();
    }
    let host = if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-musl"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };
    write_json(
        &bin.join(".qleisliup-managed.json"),
        json!({"schema":1,"host":host,"authenticated":true}),
    );
    let bytes = fs::read(&manager).unwrap();
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        write!(&mut digest, "{byte:02x}").unwrap();
    }
    let version = env!("CARGO_PKG_VERSION");
    write_json(
        &home.join("identities.json"),
        json!({"schema":1,"releases":{},"managers":{format!("{version}-{host}"):{
            "manifest_target":format!("qleisliup/{version}/manifest.json"),"manifest_sha256":"a".repeat(64),
            "artifact_target":format!("qleisliup/{version}/{host}/qleisliup"),"artifact_sha256":digest,"artifact_size":bytes.len()
        }}}),
    );
    let ledger = fs::read(home.join("identities.json")).unwrap();
    failed(
        run(&manager, &home, &["self", "update"]),
        1,
        "production distribution is not configured",
    );
    let entry = base.join("entry");
    symlink(&manager, &entry).unwrap();
    let alias_bin = base.join("alias-bin");
    symlink(&bin, &alias_bin).unwrap();
    for alias in [entry, alias_bin.join("qleisliup"), bin.join("QLEISLIUP")] {
        if alias.exists() {
            failed(
                run(&alias, &home, &["self", "update"]),
                1,
                "production distribution is not configured",
            );
        }
    }
    assert_eq!(fs::read(&manager).unwrap(), bytes);
    assert_eq!(fs::read(home.join("identities.json")).unwrap(), ledger);
    assert!(!home.join("metadata").exists());
    assert!(!home.join(".mutation-lock").exists());
    let local = base.join("local/bin");
    fs::create_dir_all(&local).unwrap();
    fs::write(
        local.join("qleisli"),
        b"#!/bin/sh\nprintf 'fixture compiler %s\\n' \"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(local.join("qleisli"), fs::Permissions::from_mode(0o755)).unwrap();
    let linked = run(
        &manager,
        &home,
        &[
            "toolchain",
            "link",
            "dev",
            local.parent().unwrap().to_str().unwrap(),
        ],
    );
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = run(&bin.join("qli"), &home, &["+dev", "--version"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"fixture compiler --version\n");
}
