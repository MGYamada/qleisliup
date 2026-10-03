# Publishing the source package

Version 0.1.1 provides a crates.io source package. This document describes
the release procedure; it does not itself authorize a registry upload. Production
Qleisli distribution, manager artifacts, endpoints, and trust roots remain a
separate task. See the [implementation plan](implementation-plan.md).

## Package contents

Cargo.toml is the product version source and restricts publication to crates.io.
Its explicit include list contains both executable sources, Cargo.lock, README,
CHANGELOG, Apache-2.0 LICENSE, NOTICE, working guidelines, design/release docs,
integration tests, native fixture sources, and both CLI/runtime smoke scripts. Cargo adds
the normalized manifest, original manifest, and VCS information when available.
CI configuration, build output, local manager state, and prebuilt binaries are
not included. There is no public Rust library.

Test sources generate isolated signing keys only when tests run. They do not
provide production trust roots or a runtime endpoint override.

## Preflight

Use a clean checkout of the intended release commit. Verify that Cargo.toml,
Cargo.lock, README, the specification, implementation plan, and release notes
agree on the product version, and that CHANGELOG records the actual changes.
Retain historical version examples and prior release notes.

Run the development checks in [README](../README.md) with Rust 1.85.0 and stable
and inspect the supported-host CI results. Audit the exact Cargo.lock with a
current RustSec database using `cargo audit`; resolve reported vulnerabilities
before publication and record the auditor/database versions. Then inspect and
verify the archive:

```sh
cargo package --list --frozen
cargo package --locked
cargo publish --dry-run --locked --registry crates-io
```

`cargo package` builds the extracted archive to catch missing source files;
`cargo publish --dry-run` performs the upload preparation without uploading.
Packaging may query the crates.io index even after a locked dependency fetch,
so CI uses `--locked` for this step and `--frozen` for builds, tests, and lints.
Do not skip package verification. For review of uncommitted preparation changes,
`--allow-dirty` can be added locally; the actual release candidate must be clean.
Use `--offline` for a cached dry run when registry access is unavailable, and
record that the live registry and account permissions were not checked.

The archive is written to `target/package/qleisliup-0.1.1.crate`. Check that
LICENSE, NOTICE, Cargo.lock, both executable entry points, and native fixtures
are present. Test the extracted source as well:

```sh
cd target/package/qleisliup-0.1.1
cargo test --frozen --all-targets
cargo build --frozen
sh scripts/check_cli.sh target/debug/qleisliup
cargo build --release --frozen --bins
sh scripts/check_cargo_free.sh target/release/qleisliup
```

For a local installation check, use an isolated temporary Cargo `--root` with
`cargo install --path . --locked --offline --root <temporary-directory>`.
Check both installed binaries' help/version output. This does not authenticate
Qleisli distribution or create proxy links, a manager home, or bootstrap ownership.

Actual publication requires separate authorization and registry access. Confirm
crate ownership/name availability, the intended version's availability, and the
final CI results at that time. Tagging and GitHub releases are separate operations.

Cargo behavior is documented in the official [package command](https://doc.rust-lang.org/cargo/commands/cargo-package.html),
[publish command](https://doc.rust-lang.org/cargo/commands/cargo-publish.html), and
[manifest reference](https://doc.rust-lang.org/cargo/reference/manifest.html).
