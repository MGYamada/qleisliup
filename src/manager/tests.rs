//! Native process probes exercise descendants without production trust changes.
use super::*;

fn probe_with_helper(behavior: &str, expected_error: Option<&str>) {
    let temporary = tempfile::tempdir().unwrap();
    let executable = temporary.path().join("manager");
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--edition=2024", "--crate-name", "manager_probe_fixture"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manager_tool.rs"))
        .arg("-o")
        .arg(&executable)
        .env("QLEISLIUP_FIXTURE_MANAGER_VERSION", "0.2.0")
        .env("QLEISLIUP_FIXTURE_MANAGER_BEHAVIOR", behavior)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = verify_version(
        &executable,
        &ExactVersion::parse("0.2.0").unwrap(),
        temporary.path(),
        Duration::from_secs(2),
    );
    // The candidate waits for this marker before hanging/exiting/printing, so
    // this regression cannot pass merely because no helper was ever started.
    assert!(temporary.path().join("helper.ready").exists());
    fs::write(temporary.path().join("helper.release"), b"probe returned").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !temporary.path().join("helper.survived").exists(),
        "{behavior} left its helper running after the probe returned"
    );
    match expected_error {
        Some(message) => assert!(result.unwrap_err().to_string().contains(message)),
        None => result.unwrap(),
    }
}

#[test]
fn rejected_version_probes_terminate_their_helpers() {
    for (behavior, diagnostic) in [
        ("helper-hang", "timed out"),
        ("helper-noise", "output limit"),
        ("helper-exit", "failed or reported"),
        ("helper-version", "failed or reported"),
    ] {
        probe_with_helper(behavior, Some(diagnostic));
    }
}

#[test]
fn successful_version_probes_also_terminate_their_helpers() {
    probe_with_helper("helper-valid", None);
}
