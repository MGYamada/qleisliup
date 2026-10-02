//! Test-only local records exercise inspection; they are not authenticated releases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

const PIN: &str = "qleisli-toolchain.toml";

struct Fixture {
    _temporary: TempDir,
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
}

fn host() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-musl"
    }
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn passed(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(output),
        stderr(output)
    );
    assert!(output.stderr.is_empty(), "{}", stderr(output));
}

fn failed(output: &Output, status: i32, diagnostic: &str) {
    assert_eq!(output.status.code(), Some(status), "{}", stderr(output));
    assert!(output.stdout.is_empty(), "{}", stdout(output));
    assert!(stderr(output).contains(diagnostic), "{}", stderr(output));
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let home = root.join("manager");
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        Self {
            _temporary: temporary,
            root,
            home,
            project,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_qleisliup"));
        command
            .args(args)
            .current_dir(&self.project)
            .env("QLEISLIUP_HOME", &self.home)
            .env("HOME", self.root.join("user"))
            .env_remove("QLEISLIUP_TOOLCHAIN")
            .env(
                "QLEISLIUP_TEST_EXECUTION_MARKER",
                self.root.join("executed"),
            )
            .env("HTTPS_PROXY", "http://127.0.0.1:1");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn pin(&self, version: &str) {
        fs::write(
            self.project.join(PIN),
            format!("[toolchain]\nversion = \"{version}\"\n"),
        )
        .unwrap();
    }

    fn settings(&self, version: &str) {
        fs::create_dir_all(&self.home).unwrap();
        write_json(
            &self.home.join("settings.json"),
            &json!({"schema":1,"default":version}),
        );
    }

    fn release(&self, version: &str) -> PathBuf {
        let name = format!("{version}-{}", host());
        let root = self.home.join("toolchains").join(&name);
        fs::create_dir_all(root.join("bin")).unwrap();
        let manifest = json!({
            "schema":1,"qleisli":version,"std":version,"std_kind":"embedded",
            "qargo":"0.1.5","qargo_checker_qleisli":"0.2.1",
            "host":host(),"verifier":{"kind":"embedded-rust"},
            "compiler_commit":"c".repeat(40),"verifier_commit":"d".repeat(40)
        });
        write_json(&root.join("toolchain.json"), &manifest);
        let identity = json!({
            "manifest_target":format!("releases/{version}/manifest.json"),
            "manifest_sha256":"a".repeat(64),
            "artifact_target":format!("releases/{version}/{}.tar.zst",host()),
            "artifact_sha256":"b".repeat(64),"artifact_size":1234
        });
        let mut receipt = identity.clone();
        for field in [
            "schema",
            "qleisli",
            "std",
            "std_kind",
            "qargo",
            "qargo_checker_qleisli",
            "host",
            "verifier",
        ] {
            receipt[field] = manifest[field].clone();
        }
        receipt["authenticated"] = json!(true);
        write_json(&root.join(".qleisliup-receipt.json"), &receipt);
        let identities_path = self.home.join("identities.json");
        let mut identities = if identities_path.exists() {
            read_json(&identities_path)
        } else {
            json!({"schema":1,"releases":{},"managers":{}})
        };
        identities["releases"][&name] = identity;
        write_json(&identities_path, &identities);
        for tool in ["qleisli", "qargo", "qlippy", "qlifmt", "qlidoc"] {
            executable(&root.join("bin").join(tool));
        }
        fs::write(root.join("LICENSE"), "fixture license").unwrap();
        fs::write(root.join("NOTICE"), "fixture notice").unwrap();
        root
    }

    fn link_record(&self, name: &str) -> PathBuf {
        let root = self.root.join(name);
        fs::create_dir_all(root.join("bin")).unwrap();
        executable(&root.join("bin/qleisli"));
        fs::create_dir_all(&self.home).unwrap();
        write_json(
            &self.home.join("links.json"),
            &json!({"schema":1,"links":{name:root}}),
        );
        root
    }
}

fn executable(path: &Path) {
    fs::write(
        path,
        "#!/bin/sh\ntouch \"$QLEISLIUP_TEST_EXECUTION_MARKER\"\nexit 99\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn help_version_and_read_only_empty_list_do_not_create_state() {
    let fixture = Fixture::new();
    for args in [&["--help"][..], &["--version"], &[], &["list"]] {
        passed(&fixture.run(args));
    }
    assert!(!fixture.home.exists());
    let output = fixture
        .command(&["--help"])
        .env("QLEISLIUP_HOME", "relative")
        .output()
        .unwrap();
    passed(&output);
    failed(&fixture.run(&["show"]), 1, "no toolchain is selected");
    assert!(!fixture.home.exists());
}

#[test]
fn all_four_sources_resolve_in_order_without_executing_the_tools() {
    let fixture = Fixture::new();
    for version in ["0.3.0", "0.4.0", "0.5.0", "0.6.0"] {
        fixture.release(version);
    }
    fixture.settings("0.3.0");
    fixture.pin("0.4.0");
    let output = fixture
        .command(&["+0.6.0", "show"])
        .env("QLEISLIUP_TOOLCHAIN", "0.5.0")
        .output()
        .unwrap();
    passed(&output);
    assert!(stdout(&output).contains("name: 0.6.0\nsource: command-line override"));
    let output = fixture
        .command(&["show"])
        .env("QLEISLIUP_TOOLCHAIN", "0.5.0")
        .output()
        .unwrap();
    passed(&output);
    assert!(stdout(&output).contains("name: 0.5.0\nsource: QLEISLIUP_TOOLCHAIN"));
    let output = fixture.run(&["show"]);
    passed(&output);
    assert!(stdout(&output).contains("name: 0.4.0\nsource: repository declaration"));
    fs::remove_file(fixture.project.join(PIN)).unwrap();
    let output = fixture.run(&["show"]);
    passed(&output);
    assert!(stdout(&output).contains("name: 0.3.0\nsource: global default"));
    assert!(!fixture.root.join("executed").exists());
    assert!(!fixture.home.join(".mutation-lock").exists());
}

#[test]
fn higher_override_ignores_malformed_lower_sources_and_missing_override_never_falls_back() {
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    fixture.settings("0.4.0");
    fs::write(fixture.project.join(PIN), "malformed").unwrap();
    let output = fixture
        .command(&["+0.4.0", "show"])
        .env("QLEISLIUP_TOOLCHAIN", "")
        .output()
        .unwrap();
    passed(&output);
    let output = fixture
        .command(&["show"])
        .env("QLEISLIUP_TOOLCHAIN", "0.4.0")
        .output()
        .unwrap();
    passed(&output);
    failed(
        &fixture.run(&["+0.3.0", "show"]),
        1,
        "run: qleisliup install 0.3.0",
    );
    failed(&fixture.run(&["show"]), 1, "invalid toolchain declaration");
    let output = fixture
        .command(&["show"])
        .env("QLEISLIUP_TOOLCHAIN", "")
        .output()
        .unwrap();
    failed(&output, 1, "invalid toolchain selector");
}

#[test]
fn nearest_declaration_and_pin_use_only_the_current_directory() {
    let fixture = Fixture::new();
    let root = fixture.release("0.4.0");
    fixture.pin("0.4.0");
    fixture.settings("0.4.0");
    let settings_before = fs::read(fixture.home.join("settings.json")).unwrap();
    let nested = fixture.project.join("nested/child");
    fs::create_dir_all(&nested).unwrap();
    let output = fixture
        .command(&["which"])
        .current_dir(&nested)
        .output()
        .unwrap();
    passed(&output);
    assert_eq!(
        stdout(&output).trim(),
        root.join("bin/qleisli").to_str().unwrap()
    );
    let output = fixture
        .command(&["pin", "0.5.0-rc.1+build.001"])
        .current_dir(&nested)
        .output()
        .unwrap();
    passed(&output);
    assert!(
        fs::read_to_string(nested.join(PIN))
            .unwrap()
            .contains("0.5.0-rc.1+build.001")
    );
    assert!(
        fs::read_to_string(fixture.project.join(PIN))
            .unwrap()
            .contains("0.4.0")
    );
    let output = fixture
        .command(&["show"])
        .current_dir(&nested)
        .output()
        .unwrap();
    failed(&output, 1, "run: qleisliup sync");
    assert_eq!(
        settings_before,
        fs::read(fixture.home.join("settings.json")).unwrap()
    );
}

#[test]
fn repository_versions_are_exact_and_the_schema_is_closed() {
    let fixture = Fixture::new();
    for value in [
        "stable", "dev", "^0.4", ">=0.4.0", "0.4", "v0.4.0", " 0.4.0", "0.04.0",
    ] {
        fixture.pin(value);
        failed(&fixture.run(&["show"]), 1, "exact SemVer");
    }
    for text in [
        "[toolchain]\nversion = 4\n",
        "[toolchain]\n",
        "[toolchain]\nversion = \"0.4.0\"\nprofile = \"full\"\n",
        "[toolchain]\nversion = \"0.4.0\"\n[other]\na = 1\n",
    ] {
        fs::write(fixture.project.join(PIN), text).unwrap();
        failed(&fixture.run(&["show"]), 1, "invalid toolchain declaration");
    }
    assert!(!fixture.home.exists());
}

#[test]
fn prerelease_and_build_metadata_keep_their_full_installed_identity() {
    let fixture = Fixture::new();
    let root = fixture.release("0.4.0-rc.1+build.001");
    fixture.release("0.4.0-rc.1+build.002");
    passed(&fixture.run(&["default", "0.4.0-rc.1+build.001"]));
    let output = fixture.run(&["which", "qli"]);
    passed(&output);
    assert_eq!(
        stdout(&output).trim(),
        root.join("bin/qleisli").to_str().unwrap()
    );
    assert_eq!(
        read_json(&fixture.home.join("settings.json"))["default"],
        "0.4.0-rc.1+build.001"
    );
}

#[test]
fn show_reports_qargo_checker_difference_and_only_historical_authentication() {
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    fixture.pin("0.4.0");
    let output = fixture.run(&["show"]);
    passed(&output);
    let text = stdout(&output);
    assert!(text.contains("qargo: 0.1.5\nqargo checker Qleisli: 0.2.1"));
    assert!(text.contains("embedded Rust (no external protocol)"));
    assert!(text.contains("recorded at installation; not revalidated"));
    assert!(text.contains("qargo checking uses its linked checker"));
    assert!(!fixture.root.join("executed").exists());
}

#[test]
fn default_requires_a_complete_installed_release_and_preserves_old_settings_on_failure() {
    let fixture = Fixture::new();
    failed(&fixture.run(&["default", "0.3.0"]), 1, "not installed");
    assert!(!fixture.home.exists());
    fixture.release("0.4.0");
    passed(&fixture.run(&["default", "0.4.0"]));
    let before = fs::read(fixture.home.join("settings.json")).unwrap();
    failed(&fixture.run(&["default", "0.3.0"]), 1, "not installed");
    assert_eq!(
        before,
        fs::read(fixture.home.join("settings.json")).unwrap()
    );
    assert_eq!(
        read_json(&fixture.home.join("settings.json")),
        json!({"schema":1,"default":"0.4.0"})
    );
    fixture.pin("invalid");
    passed(
        &fixture
            .command(&["default", "0.4.0"])
            .env("QLEISLIUP_TOOLCHAIN", "")
            .output()
            .unwrap(),
    );
}

#[test]
fn corrupted_or_unsupported_state_is_not_overwritten() {
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    for state in [
        "not JSON",
        "{\"schema\":2,\"default\":null}",
        "{\"schema\":1}",
        "{\"schema\":1,\"default\":\"stable\"}",
        "{\"schema\":1,\"default\":null,\"other\":true}",
    ] {
        fs::write(fixture.home.join("settings.json"), state).unwrap();
        let output = fixture.run(&["default", "0.4.0"]);
        assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
        assert_eq!(
            fs::read_to_string(fixture.home.join("settings.json")).unwrap(),
            state
        );
    }
}

#[test]
fn stable_rejects_an_invalid_channel_record() {
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    write_json(
        &fixture.home.join("channels.json"),
        &json!({"schema":1,"stable":{"version":"0.4.0","target":"channels/stable.json","sha256":"a".repeat(64)}}),
    );
    for args in [&["+stable", "show"][..], &["default", "stable"]] {
        failed(&fixture.run(args), 1, "TUF-authenticated channel history");
    }
    assert!(!fixture.home.join("settings.json").exists());
}

#[test]
fn stable_cache_selection_and_default_freeze_are_offline() {
    // Synthetic local history tests CLI consistency; signed install fixtures are
    // tested independently inside install::tests.
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    fixture.release("0.5.0");
    let history = |version: &str| json!({"schema":1,"channels":{"stable":{"version":version,"target":"channels/stable.json","sha256":"a".repeat(64),"authenticated":true}}});
    write_json(&fixture.home.join("channels.json"), &history("0.4.0"));
    passed(&fixture.run(&["default", "stable"]));
    write_json(&fixture.home.join("channels.json"), &history("0.5.0"));
    let output = fixture.run(&["+stable", "show"]);
    passed(&output);
    assert!(stdout(&output).contains("qleisli: 0.5.0"));
    let output = fixture.run(&["show"]);
    passed(&output);
    assert!(stdout(&output).contains("qleisli: 0.4.0"));
    let before = fs::read(fixture.home.join("settings.json")).unwrap();
    passed(&fixture.run(&["install", "0.5.0"]));
    assert_eq!(
        fs::read(fixture.home.join("settings.json")).unwrap(),
        before
    );
    assert!(!fixture.home.join("metadata").exists());
    write_json(&fixture.home.join("channels.json"), &history("0.6.0"));
    failed(&fixture.run(&["default", "stable"]), 1, "not installed");
    assert_eq!(
        fs::read(fixture.home.join("settings.json")).unwrap(),
        before
    );
}

#[test]
fn sync_uses_nearest_pin_ignoring_cli_environment_and_global_default() {
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    fixture.pin("0.4.0");
    fs::write(
        fixture.home.join("settings.json"),
        "malformed unused default",
    )
    .unwrap();
    let output = fixture
        .command(&["+missing-local", "sync"])
        .env("QLEISLIUP_TOOLCHAIN", "invalid override!")
        .output()
        .unwrap();
    passed(&output);
    assert!(stdout(&output).contains("repository toolchain available: 0.4.0"));
    assert!(!fixture.home.join("metadata").exists());
    let nested = fixture.project.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join(PIN), "[toolchain]\nversion = \"0.5.0\"\n").unwrap();
    let output = fixture
        .command(&["+0.4.0", "sync"])
        .current_dir(&nested)
        .output()
        .unwrap();
    failed(&output, 1, "production distribution is not configured");
    assert!(
        fixture
            .home
            .join("toolchains")
            .join(format!("0.4.0-{}", host()))
            .is_dir()
    );
    fs::remove_file(fixture.project.join(PIN)).unwrap();
    failed(&fixture.run(&["sync"]), 1, "requires a repository");
}

#[test]
fn invalid_home_and_invalid_command_syntax_never_fall_back_or_create_state() {
    let fixture = Fixture::new();
    for value in ["", "relative"] {
        let output = fixture
            .command(&["list"])
            .env("QLEISLIUP_HOME", value)
            .output()
            .unwrap();
        failed(&output, 1, "absolute path");
    }
    for args in [
        &["pin", "stable"][..],
        &["pin", "^0.4"],
        &["default", "dev"],
        &["which", "../../other"],
        &["show", "extra"],
        &["+0.4.0", "pin", "0.4.0"],
        &["--version", "extra"],
    ] {
        let output = fixture.run(args);
        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    }
    assert!(!fixture.home.exists());
}

#[test]
fn pin_refuses_to_replace_a_malformed_existing_declaration() {
    let fixture = Fixture::new();
    fs::write(
        fixture.project.join(PIN),
        "[toolchain]\nversion = \"stable\"\n",
    )
    .unwrap();
    failed(&fixture.run(&["pin", "0.4.0"]), 1, "exact SemVer");
    assert!(
        fs::read_to_string(fixture.project.join(PIN))
            .unwrap()
            .contains("stable")
    );
    assert!(!fixture.home.exists());
}

#[test]
fn list_is_deterministic_and_reports_registered_links_without_inventing_authentication() {
    let fixture = Fixture::new();
    fixture.release("0.10.0");
    fixture.release("0.9.0");
    fixture.link_record("dev");
    let first = fixture.run(&["list"]);
    passed(&first);
    let second = fixture.run(&["list"]);
    passed(&second);
    assert_eq!(first.stdout, second.stdout);
    let text = stdout(&first);
    assert!(text.find("0.9.0-").unwrap() < text.find("0.10.0-").unwrap());
    assert!(text.contains("dev (linked, local, unauthenticated; registration only)"));
    assert!(!fixture.home.join(".mutation-lock").exists());
}

#[test]
fn compiler_only_registration_is_inspectable_but_cannot_borrow_other_tools() {
    let fixture = Fixture::new();
    let root = fixture.link_record("dev");
    let output = fixture.run(&["+dev", "show"]);
    passed(&output);
    assert!(stdout(&output).contains("authentication: unauthenticated"));
    assert!(stdout(&output).contains("component versions: unknown"));
    let output = fixture.run(&["+dev", "which", "qli"]);
    passed(&output);
    assert_eq!(
        stdout(&output).trim(),
        root.join("bin/qleisli").to_str().unwrap()
    );
    failed(&fixture.run(&["+dev", "which", "qargo"]), 1, "qargo");
    assert!(!fixture.root.join("executed").exists());
}

#[test]
fn release_inspection_rejects_receipt_manifest_and_ledger_mismatches() {
    let fixture = Fixture::new();
    let root = fixture.release("0.4.0");
    fixture.pin("0.4.0");
    let manifest_path = root.join("toolchain.json");
    let original = read_json(&manifest_path);
    let mut invalid = original.clone();
    invalid["std"] = json!("0.3.0");
    write_json(&manifest_path, &invalid);
    failed(
        &fixture.run(&["show"]),
        1,
        "Qleisli/std version or host mismatch",
    );
    write_json(&manifest_path, &original);
    let receipt_path = root.join(".qleisliup-receipt.json");
    let original_receipt = read_json(&receipt_path);
    let mut invalid = original_receipt.clone();
    invalid["authenticated"] = json!(false);
    write_json(&receipt_path, &invalid);
    failed(&fixture.run(&["show"]), 1, "receipt does not match");
    let mut invalid = original_receipt.clone();
    invalid["artifact_sha256"] = json!("e".repeat(64));
    write_json(&receipt_path, &invalid);
    failed(&fixture.run(&["show"]), 1, "remembered release identity");
    write_json(&receipt_path, &original_receipt);
    fs::remove_file(&receipt_path).unwrap();
    failed(&fixture.run(&["show"]), 1, "required metadata is missing");
}

#[test]
fn incomplete_or_nonexecutable_inventory_is_not_an_installed_release() {
    let fixture = Fixture::new();
    let root = fixture.release("0.4.0");
    fixture.pin("0.4.0");
    fs::remove_file(root.join("bin/qlidoc")).unwrap();
    failed(&fixture.run(&["which"]), 1, "qlidoc");
    failed(&fixture.run(&["list"]), 1, "qlidoc");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        executable(&root.join("bin/qlidoc"));
        fs::set_permissions(root.join("bin/qleisli"), fs::Permissions::from_mode(0o644)).unwrap();
        failed(&fixture.run(&["which"]), 1, "not executable");
    }
}

#[test]
fn external_verifier_paths_and_protocol_are_validated() {
    let fixture = Fixture::new();
    let root = fixture.release("0.4.0");
    fixture.pin("0.4.0");
    let path = root.join("toolchain.json");
    let mut manifest = read_json(&path);
    manifest["verifier"] = json!({"kind":"external","protocol":0,"path":"bin/verifier"});
    write_json(&path, &manifest);
    failed(&fixture.run(&["show"]), 1, "protocol must be positive");
    manifest["verifier"] = json!({"kind":"external","protocol":3,"path":"../../outside"});
    write_json(&path, &manifest);
    failed(&fixture.run(&["show"]), 1, "confined relative path");
    manifest["verifier"] = json!({"kind":"external","protocol":3,"path":"bin/verifier"});
    write_json(&path, &manifest);
    let receipt_path = root.join(".qleisliup-receipt.json");
    let mut receipt = read_json(&receipt_path);
    receipt["verifier"] = manifest["verifier"].clone();
    write_json(&receipt_path, &receipt);
    failed(&fixture.run(&["show"]), 1, "bin/verifier");
    executable(&root.join("bin/verifier"));
    let output = fixture.run(&["show"]);
    passed(&output);
    assert!(stdout(&output).contains("external, protocol 3"));
    assert!(!fixture.root.join("executed").exists());
}

#[test]
fn bounded_metadata_rejects_oversized_files() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.home).unwrap();
    fs::write(
        fixture.home.join("settings.json"),
        vec![b' '; 1024 * 1024 + 1],
    )
    .unwrap();
    failed(&fixture.run(&["show"]), 1, "1 MiB limit");
}

#[cfg(unix)]
#[test]
fn symlink_pin_and_settings_destinations_are_never_followed_or_replaced() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = fixture.root.join("outside");
    fs::write(&outside, "untouched").unwrap();
    symlink(&outside, fixture.project.join(PIN)).unwrap();
    assert_eq!(fixture.run(&["pin", "0.4.0"]).status.code(), Some(1));
    assert_eq!(fs::read_to_string(&outside).unwrap(), "untouched");
    assert!(
        fs::symlink_metadata(fixture.project.join(PIN))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fixture.release("0.4.0");
    symlink(&outside, fixture.home.join("settings.json")).unwrap();
    assert_eq!(fixture.run(&["default", "0.4.0"]).status.code(), Some(1));
    assert_eq!(fs::read_to_string(&outside).unwrap(), "untouched");
}

#[cfg(unix)]
#[test]
fn release_binary_symlinks_and_non_utf8_overrides_fail_locally() {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let root = fixture.release("0.4.0");
    fixture.pin("0.4.0");
    let path = root.join("bin/qleisli");
    fs::remove_file(&path).unwrap();
    symlink(root.join("bin/qargo"), &path).unwrap();
    failed(&fixture.run(&["which"]), 1, "not a symlink");
    let output = fixture
        .command(&["show"])
        .env(
            "QLEISLIUP_TOOLCHAIN",
            std::ffi::OsString::from_vec(vec![0xff]),
        )
        .output()
        .unwrap();
    failed(&output, 1, "must be valid UTF-8");
}

#[test]
fn concurrent_defaults_and_pins_leave_complete_valid_files_and_release_the_lock() {
    let fixture = Fixture::new();
    fixture.release("0.4.0");
    fixture.release("0.5.0");
    let mut children = Vec::new();
    for index in 0..12 {
        let version = if index % 2 == 0 { "0.4.0" } else { "0.5.0" };
        let operation = if index % 3 == 0 { "pin" } else { "default" };
        let child = fixture
            .command(&[operation, version])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        children.push(child);
    }
    for child in children {
        passed(&child.wait_with_output().unwrap());
    }
    let settings = read_json(&fixture.home.join("settings.json"));
    assert_eq!(settings["schema"], 1);
    assert!(matches!(
        settings["default"].as_str(),
        Some("0.4.0" | "0.5.0")
    ));
    let pin = fs::read_to_string(fixture.project.join(PIN)).unwrap();
    assert!(matches!(
        pin.as_str(),
        "[toolchain]\nversion = \"0.4.0\"\n" | "[toolchain]\nversion = \"0.5.0\"\n"
    ));
    passed(&fixture.run(&["default", "0.4.0"]));
    for parent in [&fixture.home, &fixture.project] {
        assert!(fs::read_dir(parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
}
