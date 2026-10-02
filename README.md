# qleisliup

Qleisli toolchain lifecycle management.

**Exact. Immutable. Authenticated. Explicit. Independent.**

## Current status

Version **0.1.0** implements **Stages 1–4: offline selection, local links,
Unix proxies, TUF authentication, transactional toolchain installation,
bootstrap, and manager self-update**.
The manager supports help/version, `list`, `show`, `which`, `default`, `pin`,
`toolchain link/unlink`, `install`, `uninstall`, `sync`, and `self update`.
The same Cargo package also builds the native `qleisliup-init` bootstrap.
Running the manager
without arguments prints help.

**Production distribution URLs and the initial trusted root are not configured.**
New installs, bootstrap, and updates fail closed with exit status 1. Test repositories and keys are
internal to the test harness; the CLI has no trust or endpoint override.
Already installed exact releases can be reused offline. Externally installed
managers refuse self-replacement. Version 0.1.0 is a GitHub source release;
production toolchain distribution and prebuilt manager artifacts are not included.
The package is not published to crates.io.

Rust **1.85** or newer is required; the Rust implementation edition is **2024**.
The implementation uses semver, serde/serde_json, toml, Unix rustix filesystem
operations, nix for direct execve, tough 0.24.0 for TUF, and bounded tar/zstd
handling. Native aws-lc and zstd dependencies require a C/C++ toolchain and CMake;
macOS builds use Xcode command-line tools, and Linux musl builds need musl-tools.
Dependency versions are fixed and Cargo.lock is tracked.

## Build and inspect

On a fresh checkout, first fetch the locked Rust implementation dependencies:
`cargo fetch --locked`. Subsequent builds/checks can use `--frozen` offline.

```sh
cargo build --frozen
cargo run --frozen -- --help
cargo run --frozen -- --version
```

The version command prints `qleisliup 0.1.0`. Cargo.toml is the version source;
the manager's version is independent of Qleisli and qargo versions.

## Local selection and pinning

Repositories declare an exact language distribution in `qleisli-toolchain.toml`:

```toml
[toolchain]
version = "0.4.0"
```

The version is illustrative and does not announce an available distribution.
`stable`, version constraints, and local toolchain names are forbidden in
repository declarations. Valid prerelease and build metadata are preserved.

```sh
qleisliup pin 0.4.0
qleisliup show
qleisliup which
qleisliup +0.3.0 show
qleisliup list
qleisliup default 0.4.0
```

`pin` atomically writes only the current directory's declaration; it does not
install a release. Inspection requires an existing complete toolchain and valid
local manifest/receipt/remembered identity records. A fresh home therefore permits
pinning and an empty list, while show/which/default report missing installations.
Receipt and channel state are installer records, not user-maintained declarations.

Selection order is leading `+selector` (show/which/proxies), `QLEISLIUP_TOOLCHAIN`, the
nearest declaration searched through parent directories, then global default.
Invalid or unavailable selections fail without fallback. `which` prints a path
without running it; qli and qleisli resolve to the same compiler file.

State lives in `$HOME/.qleisliup`; `QLEISLIUP_HOME` selects another absolute
directory. Inspection does not create it. Pin/default mutations create a home
lock as needed; link/unlink also serialize state changes through that lock.
Default stores an installed exact version; `default stable` freezes the exact
version from a local authenticated observation. Symlink destinations
and malformed existing records are rejected.

## Adopted distribution direction

Normal proxy dispatch and inspection stay offline; installation is explicit.
Distributions bind matching Qleisli/stdlib versions, an independently versioned
qargo bundle, and the actual verifier arrangement. TUF authenticates downloaded
bytes; local linked toolchains remain visibly unauthenticated.

`qli` and `qleisli` select the same compiler. qargo currently uses its own
linked Qleisli checker; selecting a distribution will not change that checker.
qleisliup does not provide a qargo integration library or interpret Qleisli
semantics. No production distribution URL or trust root is configured.

The initial target platforms are macOS ARM64, macOS x86_64, and Linux x86_64
musl. Windows and Linux ARM64 are deferred. CI checks Stages 1–4 on the three
initial platforms; it does not publish binaries.

## Installation boundary

```sh
qleisliup install 0.4.0
qleisliup install stable
qleisliup sync
qleisliup default stable
qleisliup uninstall 0.4.0
```

The versions remain illustrative. New installs and channel refreshes require
production trust configuration established in a separate task. `sync` targets
the nearest exact repository pin and ignores environment and leading CLI
selection overrides. Install/sync never change the default; an existing exact
installation is reused without network access. Stable inspection/default use
local authenticated history, and fail when that history or installation is absent.

The client retains rotated roots, rollback metadata, channel high-water marks,
and remembered manifest/artifact identities. It verifies the whole target before
bounded extraction, rejects unsafe archive entries, validates the complete
inventory, writes a receipt, then publishes by an exclusive atomic rename.
Archive entries that alias the installer receipt on the extraction filesystem
are rejected, including case aliases on macOS.
Uninstall preserves authentication/identity history and refuses the global default.
A receipt describes authentication at installation; it does not revalidate local
bytes or establish mathematical correctness. Real upstream bundles and the
production security configuration still require separate validation.

## Bootstrap and manager update

```sh
target/debug/qleisliup-init --help
target/debug/qleisliup-init --version
# After production trust/endpoints are established separately:
qleisliup-init
qleisliup self update
```

Bootstrap authenticates the independent manager stable channel, its manifest,
and a native executable target. It verifies the complete download and checks
the declared version in a separate process group, terminating probe helpers on
both success and failure, before atomically publishing `<home>/bin` with the
manager,
six relative proxy symlinks, and a bootstrap ownership marker. The bin destination
must be absent; existing commands are preserved. It installs no toolchain and
does not modify shell configuration. Add the printed bin directory to PATH yourself.

Self-update requires the executable at the managed path, matching authenticated
identity history, the ownership marker, and intact proxy links. It rejects
downgrades/republication. Symlink or case aliases used to invoke that same managed
executable are accepted after canonical path comparison. It atomically replaces
only `bin/qleisliup` after staged verification. Proxies, toolchains, pins, links,
and defaults stay in place. A same
version still authenticates the current manifest. Failures before rename preserve
the old manager; errors after rename report that publication occurred. Cargo or
package-manager installations must use their original installation method.

The manager identity ledger is durable before rename, so a replacement needs no
mutable version-pointer repair. This state records past authentication and relies
on the local filesystem owner. Real host/linkage compatibility, power-loss testing,
bootstrap authenticity, and production trust configuration remain release gates.

## Local links and proxies

Register a local build containing executable `bin/qleisli`:

```sh
qleisliup toolchain link dev /absolute/path/to/local/toolchain
qleisliup +dev show
qleisliup +dev which qli
qleisliup toolchain unlink dev
```

Input paths may be relative; the stored path is canonical and UTF-8 for JSON.
A compiler-only build is sufficient. Optional tools and an internal manifest
are validated when needed; a supplied manifest must be valid. Linked builds
are always local and unauthenticated. Duplicate names require unlinking first;
unlink removes the registration and keeps the build directory.

To try the proxies from this checkout, create development symlinks after building:

```sh
mkdir -p target/proxies
for tool in qli qleisli qargo qlippy qlifmt qlidoc; do
    ln -s ../debug/qleisliup "target/proxies/$tool"
done
target/debug/qleisliup toolchain link dev /absolute/path/to/local/toolchain
target/proxies/qli +dev --version
```

Add the absolute `target/proxies` directory to PATH for nested proxy calls.
Bootstrap installs managed proxy links. The development links above stay in
ignored build output and no shell configuration is changed.
Proxy dispatch recognizes the symlink name, consumes only a leading selector,
and directly execs the selected absolute file. OS arguments, standard I/O,
working directory, PID, and tool exit/signal status are preserved. The selected
bin is prepended to PATH, and the resolved toolchain/home are passed to nested
calls. Missing tools, recursive manager targets, and exec failures are errors;
no other toolchain, PATH executable, or implicit shell is used as fallback.

## Design and development

- [Specification](docs/specification.md): CLI, selection, state, distribution,
  transactions, and trust boundaries. Planned behavior is marked explicitly.
- [Implementation plan](docs/implementation-plan.md): ordered delivery stages,
  acceptance scenarios, and production prerequisites.
- [Stage 3 review](docs/review-stage3.md): reproduced findings, fixes, and scope.
- [Stage 4 review](docs/review-stage4.md): manager and receipt boundary fixes.
- [Working guidelines](AGENTS.md): repository development rules.
- [Changelog](CHANGELOG.md): changes made to this project.

Development checks:

```sh
cargo build --frozen
cargo test --frozen --all-targets
cargo fmt --all --check
rustfmt --edition 2024 --check tests/fixtures/proxy_tool.rs
rustfmt --edition 2024 --check tests/fixtures/manager_tool.rs
cargo clippy --frozen --all-targets -- -D warnings
sh scripts/check_cli.sh target/debug/qleisliup
git diff --check
```

Run these checks with both Rust 1.85.0 and stable. Build output is ignored.
Integration tests use isolated temporary homes and synthetic local records.
Internal transaction tests generate isolated signed TUF repositories, authenticate
toolchain bundles and manager executables, check attack/failure paths, and kill
test processes before publication to verify safe retry. Both executable harnesses
exercise the shared private core; there is no public integration library.
Production binaries contain no fixture
keys. Process tests compile a small native
fixture and exercise symlink proxies, byte-preserving transport, nested selection,
and signals. Live production downloads and real bundle compatibility remain
unverified; see the recorded checks and prerequisites in the implementation plan.

## License

Copyright 2026 Masahiko G. Yamada. Licensed under [Apache-2.0](LICENSE).
See [NOTICE](NOTICE) for attribution.
