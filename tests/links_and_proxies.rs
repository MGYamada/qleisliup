//! Local process tests use synthetic records, not authenticated distributions.
#![cfg(unix)]

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use serde_json::{Value, json};
use tempfile::TempDir;

const TOOLS: [&str; 6] = ["qli", "qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"];
static NATIVE: OnceLock<(TempDir, PathBuf)> = OnceLock::new();

fn native() -> &'static Path {
    &NATIVE
        .get_or_init(|| {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("native-tool");
            let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .args(["--edition=2024", "--crate-name", "proxy_fixture"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proxy_tool.rs"))
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            (temporary, path)
        })
        .1
}

struct Fixture {
    _temporary: TempDir,
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
    proxies: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let home = root.join("home");
        let project = root.join("project");
        let proxies = root.join("proxies");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&proxies).unwrap();
        for tool in TOOLS {
            symlink(env!("CARGO_BIN_EXE_qleisliup"), proxies.join(tool)).unwrap();
        }
        Self {
            _temporary: temporary,
            root,
            home,
            project,
            proxies,
        }
    }

    fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .current_dir(&self.project)
            .env("QLEISLIUP_HOME", &self.home)
            .env("HOME", self.root.join("user"))
            .env_remove("QLEISLIUP_TOOLCHAIN")
            .env_remove("QLEISLIUP_FIXTURE_EXIT")
            .env("HTTPS_PROXY", "http://127.0.0.1:1")
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("ALL_PROXY", "http://127.0.0.1:1");
        command
    }

    fn manager(&self, args: &[&str]) -> Command {
        let mut command = self.command(Path::new(env!("CARGO_BIN_EXE_qleisliup")));
        command.args(args);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.manager(args).output().unwrap()
    }

    fn proxy(&self, tool: &str) -> Command {
        self.command(&self.proxies.join(tool))
    }

    fn build(&self, name: &str, complete: bool) -> PathBuf {
        let root = self.root.join(name);
        fs::create_dir_all(root.join("bin")).unwrap();
        for tool in if complete { &TOOLS[1..] } else { &TOOLS[1..2] } {
            fs::copy(native(), root.join("bin").join(tool)).unwrap();
        }
        root
    }

    fn link(&self, name: &str, root: &Path) {
        let output = self
            .manager(&["toolchain", "link", name])
            .arg(root)
            .output()
            .unwrap();
        passed(&output);
        assert!(text(&output).contains("local, unauthenticated"));
    }

    fn release(&self, version: &str) -> PathBuf {
        let host = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "aarch64-apple-darwin"
        } else if cfg!(target_os = "macos") {
            "x86_64-apple-darwin"
        } else {
            "x86_64-unknown-linux-musl"
        };
        let name = format!("{version}-{host}");
        let root = self.home.join("toolchains").join(&name);
        fs::create_dir_all(root.join("bin")).unwrap();
        for tool in &TOOLS[1..] {
            fs::copy(native(), root.join("bin").join(tool)).unwrap();
        }
        let manifest = json!({"schema":1,"qleisli":version,"std":version,"std_kind":"embedded",
            "qargo":"0.1.5","qargo_checker_qleisli":"0.2.1","host":host,
            "verifier":{"kind":"embedded-rust"},"compiler_commit":"c".repeat(40),"verifier_commit":"d".repeat(40)});
        write_json(&root.join("toolchain.json"), &manifest);
        let identity = json!({"manifest_target":format!("releases/{version}/manifest.json"),
            "manifest_sha256":"a".repeat(64),"artifact_target":format!("releases/{version}/{host}.tar.zst"),
            "artifact_sha256":"b".repeat(64),"artifact_size":1234});
        let mut receipt = manifest.clone();
        receipt.as_object_mut().unwrap().remove("compiler_commit");
        receipt.as_object_mut().unwrap().remove("verifier_commit");
        receipt
            .as_object_mut()
            .unwrap()
            .extend(identity.as_object().unwrap().clone());
        receipt["authenticated"] = json!(true);
        write_json(&root.join(".qleisliup-receipt.json"), &receipt);
        let identities_path = self.home.join("identities.json");
        let mut identities = if identities_path.exists() {
            read_json(&identities_path)
        } else {
            json!({"schema":1,"releases":{}})
        };
        identities["releases"][name] = identity;
        write_json(&identities_path, &identities);
        fs::write(root.join("LICENSE"), "test license").unwrap();
        fs::write(root.join("NOTICE"), "test notice").unwrap();
        root
    }
}

fn write_json(path: &Path, record: &Value) {
    fs::write(path, serde_json::to_vec_pretty(record).unwrap()).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn passed(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn failed(output: &Output, code: i32, message: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}

fn field(output: &Output, name: &str, value: &OsStr) {
    assert!(
        text(output)
            .lines()
            .any(|line| line == format!("{name}:{}", hex(value.as_bytes()))),
        "{}",
        text(output)
    );
}

#[test]
fn compiler_only_registration_canonicalizes_relative_paths_and_unlink_preserves_the_build() {
    let fixture = Fixture::new();
    let build = fixture.build("local build", false);
    symlink(&build, fixture.project.join("alias")).unwrap();
    fixture.link("dev", Path::new("alias"));
    assert_eq!(
        read_json(&fixture.home.join("links.json"))["links"]["dev"],
        build.to_str().unwrap()
    );
    let output = fixture.run(&["+dev", "show"]);
    passed(&output);
    assert!(text(&output).contains("kind: linked, local\nauthentication: unauthenticated"));
    assert!(text(&output).contains("component versions: unknown"));
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap(),
        1,
        "already registered",
    );
    failed(
        &fixture.proxy("qargo").arg("+dev").output().unwrap(),
        1,
        "qargo",
    );
    passed(&fixture.run(&["toolchain", "unlink", "dev"]));
    assert!(build.join("bin/qleisli").is_file());
    assert_eq!(
        read_json(&fixture.home.join("links.json"))["links"],
        json!({})
    );
    failed(
        &fixture.run(&["toolchain", "unlink", "dev"]),
        1,
        "not registered",
    );
    fixture.link("dev", &build);
}

#[test]
fn invalid_names_builds_and_manifests_do_not_create_registrations() {
    let fixture = Fixture::new();
    let build = fixture.build("build", false);
    for name in [
        "", "stable", "beta", "nightly", "0.4.0", "../dev", "dev/name", "開発",
    ] {
        failed(
            &fixture
                .manager(&["toolchain", "link", name])
                .arg(&build)
                .output()
                .unwrap(),
            2,
            "invalid local name",
        );
    }
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg("missing")
            .output()
            .unwrap(),
        1,
        "missing",
    );
    fs::set_permissions(build.join("bin/qleisli"), fs::Permissions::from_mode(0o644)).unwrap();
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap(),
        1,
        "not executable",
    );
    fs::set_permissions(build.join("bin/qleisli"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(build.join("toolchain.json"), "{}").unwrap();
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap(),
        1,
        "invalid JSON",
    );
    failed(
        &fixture.run(&["toolchain", "unlink", "dev"]),
        1,
        "not registered",
    );
    assert!(!fixture.home.exists());
}

#[test]
fn corrupted_and_symlink_link_records_are_preserved() {
    let fixture = Fixture::new();
    let build = fixture.build("build", false);
    fs::create_dir(&fixture.home).unwrap();
    let path = fixture.home.join("links.json");
    for record in [
        "not JSON",
        "{\"schema\":2,\"links\":{}}",
        "{\"schema\":1,\"links\":{},\"extra\":true}",
    ] {
        fs::write(&path, record).unwrap();
        assert!(
            !fixture
                .manager(&["toolchain", "link", "dev"])
                .arg(&build)
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(
            !fixture
                .run(&["toolchain", "unlink", "dev"])
                .status
                .success()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), record);
    }
    fs::remove_file(&path).unwrap();
    let outside = fixture.root.join("outside.json");
    fs::write(&outside, "external contents").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(
        !fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "external contents");
}

#[test]
fn oversized_registration_preserves_the_readable_previous_registry() {
    let fixture = Fixture::new();
    let root = fixture.build("bounded-registry", false);
    fs::create_dir_all(&fixture.home).unwrap();
    let mut registrations = serde_json::Map::new();
    // Fixed-width keys let us build a near-limit registry without repeatedly
    // serializing an ever-growing megabyte of state.
    registrations.insert(format!("dev{:01024}", 0), json!(root));
    let one = serde_json::to_vec_pretty(&json!({"schema":1,"links":registrations}))
        .unwrap()
        .len();
    registrations.insert(format!("dev{:01024}", 1), json!(root));
    let two = serde_json::to_vec_pretty(&json!({"schema":1,"links":registrations}))
        .unwrap()
        .len();
    let count = 2 + (1024 * 1024 - 16384 - two) / (two - one);
    for index in 2..count {
        registrations.insert(format!("dev{index:01024}"), json!(root));
    }
    let before = serde_json::to_vec_pretty(&json!({"schema":1,"links":registrations})).unwrap();
    assert!(before.len() < 1024 * 1024);
    assert!(before.len() > 1024 * 1024 - 32768);
    fs::write(fixture.home.join("links.json"), &before).unwrap();
    let name = "z".repeat(32768);
    let output = fixture.run(&["toolchain", "link", &name, root.to_str().unwrap()]);
    failed(&output, 1, "1 MiB limit");
    assert_eq!(fs::read(fixture.home.join("links.json")).unwrap(), before);
    passed(&fixture.run(&[&format!("+dev{:01024}", 0), "which"]));
}

#[test]
fn optional_link_manifests_require_the_declared_std_and_external_verifier() {
    let fixture = Fixture::new();
    let release = fixture.release("0.4.0");
    let build = fixture.build("build", false);
    let mut manifest = read_json(&release.join("toolchain.json"));
    manifest["std_kind"] = json!("directory");
    write_json(&build.join("toolchain.json"), &manifest);
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap(),
        1,
        "std",
    );
    fs::create_dir(build.join("std")).unwrap();
    fixture.link("dev", &build);
    let output = fixture.run(&["+dev", "show"]);
    passed(&output);
    assert!(text(&output).contains("qargo checker Qleisli: 0.2.1"));
    assert!(text(&output).contains("independently of the selected compiler"));
    assert!(text(&output).contains("authentication: unauthenticated"));
    failed(
        &fixture.proxy("qargo").arg("+dev").output().unwrap(),
        1,
        "qargo",
    );
    passed(&fixture.run(&["toolchain", "unlink", "dev"]));
    manifest["verifier"] = json!({"kind":"external","protocol":3,"path":"backends/verifier"});
    write_json(&build.join("toolchain.json"), &manifest);
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap(),
        1,
        "backends",
    );
    fs::create_dir(build.join("backends")).unwrap();
    fs::copy(native(), build.join("backends/verifier")).unwrap();
    fixture.link("dev", &build);
    let output = fixture.run(&["+dev", "show"]);
    passed(&output);
    assert!(text(&output).contains("verifier: external, protocol 3, backends/verifier"));
}

#[test]
fn concurrent_registrations_and_unlinks_preserve_independent_names() {
    let fixture = Fixture::new();
    let build = fixture.build("build", false);
    let mut children = Vec::new();
    for index in 0..8 {
        children.push(
            fixture
                .manager(&["toolchain", "link", &format!("dev{index}")])
                .arg(&build)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        passed(&child.wait_with_output().unwrap());
    }
    assert_eq!(
        read_json(&fixture.home.join("links.json"))["links"]
            .as_object()
            .unwrap()
            .len(),
        8
    );
    let mut children = Vec::new();
    for _ in 0..2 {
        children.push(
            fixture
                .manager(&["toolchain", "link", "same"])
                .arg(&build)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    let results: Vec<_> = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|result| result.status.success())
            .count(),
        1
    );
    failed(
        results
            .iter()
            .find(|result| !result.status.success())
            .unwrap(),
        1,
        "already registered",
    );
    let mut children = Vec::new();
    for index in 0..8 {
        children.push(
            fixture
                .manager(&["toolchain", "unlink", &format!("dev{index}")])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        passed(&child.wait_with_output().unwrap());
    }
    assert_eq!(
        read_json(&fixture.home.join("links.json"))["links"],
        json!({"same":build})
    );
    assert!(fs::read_dir(&fixture.home).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[test]
fn all_six_proxy_names_execute_their_selected_absolute_tool() {
    let fixture = Fixture::new();
    let build = fixture.build("bundle", true);
    fixture.link("dev", &build);
    let state_before = fs::read(fixture.home.join("links.json")).unwrap();
    for tool in TOOLS {
        let output = fixture
            .proxy(tool)
            .args(["+dev", "argument with spaces"])
            .output()
            .unwrap();
        passed(&output);
        let target = if tool == "qli" { "qleisli" } else { tool };
        field(&output, "argv0", build.join("bin").join(target).as_os_str());
        field(&output, "QLEISLIUP_TOOLCHAIN", OsStr::new("dev"));
        field(&output, "QLEISLIUP_HOME", fixture.home.as_os_str());
        field(&output, "cwd", fixture.project.as_os_str());
    }
    assert_eq!(
        fs::read(fixture.home.join("links.json")).unwrap(),
        state_before
    );
    assert!(!fixture.home.join("settings.json").exists());
}

#[test]
fn exec_preserves_pid_os_arguments_streams_cwd_and_exit_status() {
    let fixture = Fixture::new();
    let build = fixture.build("build", false);
    fixture.link("dev", &build);
    let raw = OsString::from_vec(vec![b'a', 0xff, b'b']);
    let original_path = OsString::from_vec(vec![b'/', b'x', 0xff, b':', b'/', b'y']);
    let mut command = fixture.proxy("qli");
    command
        .args(["+dev", "__echo", "", "spaces ; $() `literal`", "+stable"])
        .arg(&raw)
        .env("PATH", &original_path)
        .env("QLEISLIUP_FIXTURE_EXIT", "37")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"input\0\xff\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(37));
    assert_eq!(output.stderr, b"fixture stderr\0\xff\n");
    assert!(text(&output).contains(&format!("pid:{pid}\n")));
    assert!(text(&output).contains("stdin:696e70757400ff0a\n"));
    let expected: Vec<_> = [
        OsStr::new("__echo"),
        OsStr::new(""),
        OsStr::new("spaces ; $() `literal`"),
        OsStr::new("+stable"),
        raw.as_os_str(),
    ]
    .iter()
    .map(|arg| format!("arg:{}", hex(arg.as_bytes())))
    .collect();
    assert_eq!(
        text(&output)
            .lines()
            .filter(|line| line.starts_with("arg:"))
            .collect::<Vec<_>>(),
        expected
    );
    let mut expected_path = build.join("bin").into_os_string();
    expected_path.push(":");
    expected_path.push(original_path);
    field(&output, "PATH", &expected_path);
}

#[test]
fn tool_help_version_and_non_utf8_first_arguments_are_forwarded() {
    let fixture = Fixture::new();
    fixture.link("dev", &fixture.build("build", false));
    for flag in ["--help", "--version", "-V"] {
        let output = fixture
            .proxy("qleisli")
            .args(["+dev", flag])
            .output()
            .unwrap();
        passed(&output);
        field(&output, "arg", OsStr::new(flag));
        assert!(!text(&output).contains("qleisliup 0.1.0"));
    }
    let raw = OsString::from_vec(vec![0xff, b'+']);
    let output = fixture
        .proxy("qli")
        .env("QLEISLIUP_TOOLCHAIN", "dev")
        .arg(&raw)
        .output()
        .unwrap();
    passed(&output);
    field(&output, "arg", &raw);
    passed(
        &fixture
            .proxy("qli")
            .env("QLEISLIUP_TOOLCHAIN", "dev")
            .output()
            .unwrap(),
    );
}

#[test]
fn malformed_leading_selectors_fail_and_higher_overrides_ignore_lower_errors() {
    let fixture = Fixture::new();
    fixture.link("dev", &fixture.build("build", false));
    fs::write(fixture.project.join("qleisli-toolchain.toml"), "malformed").unwrap();
    for value in ["+", "+../dev", "+^0.4"] {
        failed(
            &fixture
                .proxy("qli")
                .arg(value)
                .env("QLEISLIUP_TOOLCHAIN", "dev")
                .output()
                .unwrap(),
            2,
            "invalid toolchain selector",
        );
    }
    failed(
        &fixture
            .proxy("qli")
            .arg(OsString::from_vec(vec![b'+', 0xff]))
            .output()
            .unwrap(),
        2,
        "must be valid UTF-8",
    );
    passed(
        &fixture
            .proxy("qli")
            .arg("+dev")
            .env("QLEISLIUP_TOOLCHAIN", "invalid/name")
            .output()
            .unwrap(),
    );
    passed(
        &fixture
            .proxy("qli")
            .env("QLEISLIUP_TOOLCHAIN", "dev")
            .output()
            .unwrap(),
    );
    failed(
        &fixture
            .proxy("qli")
            .arg("+absent")
            .env("QLEISLIUP_TOOLCHAIN", "dev")
            .output()
            .unwrap(),
        1,
        "not registered",
    );
    failed(
        &fixture
            .proxy("qli")
            .env("QLEISLIUP_TOOLCHAIN", "")
            .output()
            .unwrap(),
        1,
        "invalid toolchain selector",
    );
}

#[test]
fn proxies_apply_all_four_selection_sources_without_authentication_refresh() {
    let fixture = Fixture::new();
    for version in ["0.3.0", "0.4.0", "0.5.0", "0.6.0"] {
        fixture.release(version);
    }
    write_json(
        &fixture.home.join("settings.json"),
        &json!({"schema":1,"default":"0.3.0"}),
    );
    fs::write(
        fixture.project.join("qleisli-toolchain.toml"),
        "[toolchain]\nversion = \"0.4.0\"\n",
    )
    .unwrap();
    fs::write(
        fixture.home.join("channels.json"),
        "invalid remote-channel state",
    )
    .unwrap();
    for (args, environment, expected) in [
        (vec!["+0.6.0"], Some("0.5.0"), "0.6.0"),
        (vec![], Some("0.5.0"), "0.5.0"),
        (vec![], None, "0.4.0"),
    ] {
        let mut command = fixture.proxy("qli");
        command.args(args);
        if let Some(value) = environment {
            command.env("QLEISLIUP_TOOLCHAIN", value);
        }
        let output = command.output().unwrap();
        passed(&output);
        field(&output, "QLEISLIUP_TOOLCHAIN", OsStr::new(expected));
    }
    fs::remove_file(fixture.project.join("qleisli-toolchain.toml")).unwrap();
    let output = fixture.proxy("qli").output().unwrap();
    passed(&output);
    field(&output, "QLEISLIUP_TOOLCHAIN", OsStr::new("0.3.0"));
    assert!(!fixture.home.join(".mutation-lock").exists());
}

#[test]
fn nested_proxy_retains_the_selected_toolchain_and_qargo_finds_sibling_tools() {
    let fixture = Fixture::new();
    let build = fixture.build("build", true);
    fixture.link("dev", &build);
    let nested = fixture.project.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(
        nested.join("qleisli-toolchain.toml"),
        "[toolchain]\nversion = \"0.9.0\"\n",
    )
    .unwrap();
    let output = fixture
        .proxy("qargo")
        .args(["+dev", "__nested"])
        .env("QLEISLIUP_TOOLCHAIN", "wrong")
        .env("QLEISLIUP_FIXTURE_PROXY", "qli")
        .env(
            "PATH",
            std::env::join_paths([
                fixture.proxies.as_path(),
                Path::new("/bin"),
                Path::new("/usr/bin"),
            ])
            .unwrap(),
        )
        .env("QLEISLIUP_FIXTURE_CWD", &nested)
        .output()
        .unwrap();
    passed(&output);
    field(&output, "QLEISLIUP_TOOLCHAIN", OsStr::new("dev"));
    field(&output, "argv0", build.join("bin/qleisli").as_os_str());
    field(&output, "cwd", nested.as_os_str());
    let output = fixture
        .proxy("qargo")
        .args(["+dev", "__sibling"])
        .output()
        .unwrap();
    passed(&output);
    field(&output, "argv0", build.join("bin/qlippy").as_os_str());
}

#[test]
fn signal_termination_belongs_to_the_execed_tool() {
    let fixture = Fixture::new();
    fixture.link("dev", &fixture.build("build", false));
    let output = fixture
        .proxy("qli")
        .args(["+dev", "__signal"])
        .output()
        .unwrap();
    assert_eq!(output.status.signal(), Some(15));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn missing_tools_exec_failures_and_plain_text_never_fall_back_or_run_a_shell() {
    let fixture = Fixture::new();
    let build = fixture.build("build", false);
    fixture.link("dev", &build);
    let fallback = fixture.root.join("fallback");
    fs::create_dir(&fallback).unwrap();
    let marker = fixture.root.join("executed");
    let plain_text = "printf 'unexpected shell execution' > \"$QLEISLIUP_FIXTURE_MARKER\"\n";
    for tool in [fallback.join("qargo"), build.join("bin/qargo")] {
        fs::write(&tool, plain_text).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    failed(
        &fixture
            .proxy("qargo")
            .arg("+dev")
            .env("PATH", &fallback)
            .env("QLEISLIUP_FIXTURE_MARKER", &marker)
            .output()
            .unwrap(),
        1,
        "cannot execute selected tool",
    );
    assert!(!marker.exists());
    fs::remove_file(build.join("bin/qargo")).unwrap();
    failed(
        &fixture
            .proxy("qargo")
            .arg("+dev")
            .env("PATH", &fallback)
            .output()
            .unwrap(),
        1,
        "qargo",
    );
    fs::write(build.join("bin/qargo"), "#!/nonexistent/interpreter\n").unwrap();
    fs::set_permissions(build.join("bin/qargo"), fs::Permissions::from_mode(0o755)).unwrap();
    failed(
        &fixture.proxy("qargo").arg("+dev").output().unwrap(),
        1,
        "cannot execute selected tool",
    );
}

#[test]
fn recursive_hardlinks_and_tool_symlinks_are_rejected() {
    let fixture = Fixture::new();
    let recursive = fixture.root.join("recursive");
    fs::create_dir_all(recursive.join("bin")).unwrap();
    fs::hard_link(
        env!("CARGO_BIN_EXE_qleisliup"),
        recursive.join("bin/qleisli"),
    )
    .unwrap();
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&recursive)
            .output()
            .unwrap(),
        1,
        "recursive dispatch",
    );
    assert!(!fixture.home.exists());
    let build = fixture.build("build", false);
    fixture.link("dev", &build);
    fs::hard_link(env!("CARGO_BIN_EXE_qleisliup"), build.join("bin/qargo")).unwrap();
    failed(
        &fixture.proxy("qargo").arg("+dev").output().unwrap(),
        1,
        "recursive dispatch",
    );
    fs::remove_file(build.join("bin/qargo")).unwrap();
    symlink(env!("CARGO_BIN_EXE_qleisliup"), build.join("bin/qargo")).unwrap();
    failed(
        &fixture.proxy("qargo").arg("+dev").output().unwrap(),
        1,
        "not a symlink",
    );
}

#[test]
fn stable_missing_releases_and_broken_links_fail_locally_with_the_required_remedy() {
    let fixture = Fixture::new();
    failed(
        &fixture.proxy("qli").arg("+stable").output().unwrap(),
        1,
        "TUF-authenticated",
    );
    failed(
        &fixture.proxy("qli").arg("+0.4.0").output().unwrap(),
        1,
        "qleisliup install 0.4.0",
    );
    fs::write(
        fixture.project.join("qleisli-toolchain.toml"),
        "[toolchain]\nversion = \"0.4.0\"\n",
    )
    .unwrap();
    failed(&fixture.proxy("qli").output().unwrap(), 1, "qleisliup sync");
    assert!(!fixture.home.exists());
    let build = fixture.build("build", false);
    fixture.link("dev", &build);
    fs::rename(&build, fixture.root.join("moved")).unwrap();
    failed(
        &fixture.proxy("qli").arg("+dev").output().unwrap(),
        1,
        "repair its directory",
    );
    passed(&fixture.run(&["toolchain", "unlink", "dev"]));
    assert!(fixture.root.join("moved/bin/qleisli").exists());
}

#[test]
fn path_prepend_handles_unset_and_empty_path_and_rejects_unrepresentable_bin_paths() {
    let fixture = Fixture::new();
    let build = fixture.build("build", false);
    fixture.link("dev", &build);
    let output = fixture
        .proxy("qli")
        .arg("+dev")
        .env_remove("PATH")
        .output()
        .unwrap();
    passed(&output);
    field(&output, "PATH", build.join("bin").as_os_str());
    let output = fixture
        .proxy("qli")
        .arg("+dev")
        .env("PATH", "")
        .output()
        .unwrap();
    passed(&output);
    let mut expected = build.join("bin").into_os_string();
    expected.push(":");
    field(&output, "PATH", &expected);
    let colon = fixture.build("colon:build", false);
    fixture.link("colon", &colon);
    failed(
        &fixture.proxy("qli").arg("+colon").output().unwrap(),
        1,
        "cannot prepend toolchain bin to PATH",
    );
}

#[test]
// macOS filenames require valid Unicode; Linux exercises the JSON path boundary.
#[cfg(target_os = "linux")]
fn non_utf8_link_paths_are_rejected_before_state_creation() {
    let fixture = Fixture::new();
    let build = fixture.root.join(OsString::from_vec(vec![b'd', 0xff]));
    fs::create_dir_all(build.join("bin")).unwrap();
    fs::copy(native(), build.join("bin/qleisli")).unwrap();
    failed(
        &fixture
            .manager(&["toolchain", "link", "dev"])
            .arg(&build)
            .output()
            .unwrap(),
        1,
        "must be UTF-8",
    );
    assert!(!fixture.home.exists());
}
