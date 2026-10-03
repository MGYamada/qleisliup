# qleisliup

Qleisli toolchain lifecycle management.

**Exact. Immutable. Authenticated. Explicit. Independent.**

The intended end-user installation requires no Cargo or Rust toolchain:
obtain a verified native `qleisliup-init`, install prebuilt Qleisli toolchains,
and select exact versions through qleisliup. Cargo remains a development/build
tool and an optional way to install the manager from source. This end-user
distribution path is **not available yet**; see the
[Cargo-free installation plan](docs/cargo-free-installation.md).

The native distribution goal also excludes a user-installed Lean or Python
environment. Rust and supported Lean kernels are built by the distributor;
Python connections use separately distributed wheels and project-local uv
environments. See the [environment boundary](docs/environment-dependencies.md)
for ownership, exact dependency records, and pending upstream acceptance work.

## Current status

Version **0.1.2** implements **Stages 1–4: offline selection, local links,
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
managers refuse self-replacement. Version 0.1.2 is a source release for
crates.io and GitHub. Production toolchain
distribution and prebuilt manager artifacts are not included.

Building from source requires Rust **1.85** or newer; the implementation edition
is **2024**. Native manager operations do not require Cargo or rustc.
The implementation uses semver, serde/serde_json, toml, Unix rustix filesystem
operations, nix for direct execve, tough 0.24.0 for TUF, and bounded tar/zstd
handling. Native aws-lc and zstd dependencies require a C/C++ toolchain and CMake;
macOS builds use Xcode command-line tools, and Linux musl builds need musl-tools.
Dependency versions are fixed and Cargo.lock is tracked.

## Optional source installation with Cargo

From a checkout of this version:

```sh
cargo install --path . --locked
qleisliup --version
qleisliup-init --help
```

Install version 0.1.2 from crates.io:

```sh
cargo install qleisliup --version 0.1.2 --locked
```

Cargo installs `qleisliup` and `qleisliup-init`. It does not install Qleisli,
create proxy symlinks, or establish a bootstrap-owned manager. The bootstrap
still requires production distribution configuration. Update a Cargo-installed
manager with `cargo install` for the desired version; `qleisliup self update`
refuses external installations.

To use local toolchains with a default Cargo installation on a supported Unix
host, create the proxy links explicitly (existing commands are not overwritten):

```sh
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
for tool in qli qleisli qargo qlippy qlifmt qlidoc; do
    ln -s qleisliup "$cargo_bin/$tool"
done
qleisliup toolchain link dev /absolute/path/to/local/toolchain
qli +dev --version
```

Ensure that directory is on PATH. If Cargo was installed with a custom `--root`,
use its `bin` directory instead. Local toolchains remain unauthenticated.

## Build and inspect

On a fresh checkout, first fetch the locked Rust implementation dependencies:
`cargo fetch --locked`. Subsequent builds/checks can use `--frozen` offline.

```sh
cargo build --frozen
cargo run --frozen -- --help
cargo run --frozen -- --version
```

The version command prints `qleisliup 0.1.2`. Cargo.toml is the version source;
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
version from a local authenticated observation, reread while holding the mutation
lock. Symlink destinations
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
It also builds native release candidates and exercises local manager operations
with Rust, Lean, Python, and native build tools trapped on a restricted PATH.
This does not establish live toolchain download or bootstrap readiness.

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
Extracted executables use mode 0755 and data files use 0644, regardless of unsafe
ordinary permission bits in the authenticated archive; privileged bits are rejected.
Uninstall preserves authentication/identity history and refuses the global default.
A receipt describes authentication at installation; it does not revalidate local
bytes or establish mathematical correctness. Real upstream bundles and the
production security configuration still require separate validation.

New manager-owned home, state, and staging directories use mode 0700 even with
a permissive umask. Existing owned directories must not be group- or
world-writable; unsafe permissions fail closed and require explicit repair.
Published bundle directories use 0755. This policy does not restrict project
pin directories or unauthenticated local toolchain links.

Lifecycle mutations reclaim bounded amounts of abandoned private staging under
the home lock, including unreferenced metadata generations. Cleanup protects the
committed metadata generation and rejects unexpected symlinks, special files, and
filesystem boundaries. Offline inspection, proxies, and reuse of an existing exact
installation do not trigger cleanup. See the [cleanup contract](docs/specification.md#stale-private-data-cleanup).
State writes also reclaim reserved crash-left temporary files in the destination
directory under the mutation lock. Random exclusive temporary names avoid
collisions with a prior process's PID/counter files. TUF generation pointers use
the same exact six-character suffix rule as generation cleanup.

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
- [Publishing guide](docs/publishing.md): source package contents and preflight checks.
- [v0.1.2 notes](docs/releases/v0.1.2.md): environment policy and runtime checks.
- [v0.1.1 notes](docs/releases/v0.1.1.md): previous source release and lifecycle fixes.
- [Cargo-free installation plan](docs/cargo-free-installation.md): native distribution
  work and clean-machine acceptance scenarios.
- [Environment dependencies](docs/environment-dependencies.md): native Rust/Lean
  distribution, separate Python environments, and explicit compiler handoff.

Development checks:

```sh
cargo build --frozen
cargo test --frozen --all-targets
cargo fmt --all --check
rustfmt --edition 2024 --check tests/fixtures/proxy_tool.rs
rustfmt --edition 2024 --check tests/fixtures/manager_tool.rs
cargo clippy --frozen --all-targets -- -D warnings
sh scripts/check_cli.sh target/debug/qleisliup
cargo build --release --frozen --bins
sh scripts/check_runtime_guard.sh
sh scripts/check_cargo_free.sh target/release/qleisliup
cargo package --locked
git diff --check
```

Run these checks with both Rust 1.85.0 and stable. Build output is ignored.
Package verification may query the crates.io index even after a locked fetch;
the build, test, and lint checks above remain offline.
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
