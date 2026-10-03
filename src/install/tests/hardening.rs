use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn metadata_pointer_and_cleanup_require_the_same_canonical_generation_names() {
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path();
    let generations = state.join("generations");
    fs::create_dir(&generations).unwrap();
    for name in [
        "state-",
        "state-a",
        "state-1234567",
        "state-foo-bar",
        "state-abc_de",
        "state-あいうえおか",
        "../outside",
    ] {
        let pointer = serde_json::to_vec(&json!({"schema":1,"generation":name})).unwrap();
        fs::write(state.join("current.json"), &pointer).unwrap();
        rejected(
            distribution::current_generation(state),
            "invalid metadata generation",
        );
        rejected(
            crate::cleanup::metadata_generations(state),
            "invalid metadata generation",
        );
        assert_eq!(fs::read(state.join("current.json")).unwrap(), pointer);
    }
    for name in ["state-aBcD01", "state-000000", "state-ZZZzzz"] {
        let active = generations.join(name);
        fs::create_dir(&active).unwrap();
        fs::write(active.join("keep"), b"active").unwrap();
        fs::write(
            state.join("current.json"),
            serde_json::to_vec(&json!({"schema":1,"generation":name})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            distribution::current_generation(state).unwrap().as_deref(),
            Some(name)
        );
        crate::cleanup::metadata_generations(state).unwrap();
        assert_eq!(fs::read(active.join("keep")).unwrap(), b"active");
    }
}

#[test]
fn state_temporary_reclamation_preserves_authenticated_bundle_files() {
    let mut fixture = Fixture::new();
    let name = format!("releases/0.4.0/{HOST}.tar.zst");
    let raw = zstd::stream::decode_all(fixture.contents[&name].as_slice()).unwrap();
    let mut archive = tar::Archive::new(raw.as_slice());
    let mut builder = tar::Builder::new(Vec::new());
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let header = entry.header().clone();
        builder.append(&header, &mut entry).unwrap();
    }
    let names = [
        ".qleisliup-123-0.tmp",
        ".qleisliup-state-ABCDEFGHIJKLMNOP.tmp",
    ];
    for name in names {
        append(
            &mut builder,
            &format!("0.4.0-{HOST}/{name}"),
            b"authenticated data",
            0o644,
            tar::EntryType::Regular,
        );
    }
    fixture.replace_archive(
        "0.4.0",
        zstd::stream::encode_all(builder.into_inner().unwrap().as_slice(), 1).unwrap(),
    );
    fixture.publish(2, FUTURE, FUTURE, true);
    fixture.install("0.4.0").unwrap();
    // The same names in persistent state are abandoned manager files; the
    // release payload remains outside state-file cleanup's namespace.
    for parent in [
        &fixture.home.path,
        &fixture.home.path.join("metadata/official"),
    ] {
        for name in names {
            fs::write(parent.join(name), b"abandoned").unwrap();
        }
    }
    fixture.install("stable").unwrap();
    for name in names {
        assert!(!fixture.home.path.join(name).exists());
        assert!(
            !fixture
                .home
                .path
                .join("metadata/official")
                .join(name)
                .exists()
        );
        assert_eq!(
            fs::read(fixture.final_path("0.4.0").join(name)).unwrap(),
            b"authenticated data"
        );
    }
}

#[test]
fn lifecycle_rejects_unsafe_existing_owned_directories_before_network_access() {
    let fixture = Fixture::new();
    fixture.install("0.4.0").unwrap();
    let state = fixture.home.path.join("metadata/official");
    let generation = distribution::current_generation(&state).unwrap().unwrap();
    for relative in [
        "".to_owned(),
        "metadata".into(),
        "metadata/official".into(),
        "metadata/official/generations".into(),
        format!("metadata/official/generations/{generation}"),
        "toolchains".into(),
        "toolchains/.transactions".into(),
    ] {
        let path = fixture.home.path.join(relative);
        let original = fs::metadata(&path).unwrap().permissions();
        for mode in [0o770, 0o707] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            fixture.requests.store(0, Ordering::SeqCst);
            rejected(
                fixture.install("stable"),
                "must not be group- or world-writable",
            );
            assert_eq!(fixture.requests.load(Ordering::SeqCst), 0);
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                mode
            );
        }
        fs::set_permissions(path, original).unwrap();
    }
    fixture.install("stable").unwrap();
}

#[test]
fn inspection_rejects_unsafe_home_and_installed_release_directories() {
    let fixture = Fixture::new();
    fixture.install("0.4.0").unwrap();
    for relative in [
        "".to_owned(),
        "toolchains".into(),
        format!("toolchains/0.4.0-{HOST}"),
        format!("toolchains/0.4.0-{HOST}/bin"),
    ] {
        let path = fixture.home.path.join(relative);
        let original = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
        rejected(
            crate::selection::list(&fixture.home),
            "must not be group- or world-writable",
        );
        fs::set_permissions(path, original).unwrap();
    }
}
