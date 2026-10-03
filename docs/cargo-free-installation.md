# Cargo-free Qleisli installation and version management

## Goal and current boundary

The intended user experience is native toolchain installation and version
management through qleisliup, analogous to a compiler toolchain manager. A user
obtains a verified `qleisliup-init`, installs a prebuilt distribution, and selects
an exact Qleisli version. Rust and Cargo remain implementation build tools;
end users need neither them nor a C/C++ compiler or CMake.

**This production installation path is not available yet.** The manager already
implements TUF verification, transactional installation, exact selection, Unix
proxies, and managed self-update. `Source::official()` currently rejects requests
because production endpoints and the initial trusted root are absent. Publishing
the Rust source package to crates.io does not remove this missing configuration.

Existing valid exact installations and local links work offline. The source
installation described in [README](../README.md) is an optional developer path.
qleisliup never invokes Cargo to install or build Qleisli as a fallback.

## Intended user flow

After verified native bootstrap acquisition and production activation, the
workflow will use the existing CLI below. The exact versions are illustrative;
they are not announcements of available or compatible distributions.

```sh
./qleisliup-init
# Add the printed manager bin directory to PATH.
qleisliup install stable
qleisliup default stable
qleisliup show
qli --help

# Install a second exact release and select it for this repository.
qleisliup install 0.4.0
qleisliup pin 0.4.0
qleisliup sync
qleisliup which

# Select a separately installed version for one invocation.
qleisliup install 0.3.0
qli +0.3.0 --help

# Refresh the available stable distribution, then explicitly change the default.
qleisliup install stable
qleisliup default stable
qleisliup self update
```

Bootstrap creates the manager and six proxies but installs no toolchain. An
install never changes the default; `default stable` resolves the locally observed
stable version and stores that exact identity. A repository pin takes precedence
over the global default, and changing the default does not rewrite that pin.
`self update` updates only the bootstrap-owned manager. Selection and proxy
execution stay offline, and a missing toolchain never triggers an implicit build
or download. Users can retain several installed versions simultaneously.

## Deliverables and ownership

| Deliverable | Required preparation | Current status |
| --- | --- | --- |
| Native bootstrap and manager | Release-mode binaries for macOS ARM64/x86_64 and Linux x86_64 musl; native execution/linkage checks | Both executables build; local runtime checks and CI candidate builds are available. No public bootstrap artifact is supplied here. |
| Exact Qleisli distribution | Native compiler, matching stdlib identity, qargo and its auxiliary executables, truthful checker/verifier bindings, LICENSE and NOTICE | The manager's bundle format and synthetic acceptance tests exist; real upstream bundle validation is pending. |
| Initial bootstrap authenticity | A documented verification method for the first downloaded native executable | Pending; a checksum fetched with an otherwise unverified executable is not a new trust authority. |
| Production TUF repository | Owned metadata/target endpoints, approved initial public root, role custodians/thresholds, expiry and rotation procedures | Unconfigured. Test roots must never be used. |
| Clean-machine acceptance | Actual install, switch, pin, proxy, and update workflows without Rust developer tools | Local fixture checks exist; real distribution acceptance remains pending. |

The distribution boundary remains the
[specified complete bundle](specification.md#distribution-and-internal-manifests).
A separately available qargo binary archive or compiler executable does not by
itself satisfy that contract. qargo's linked checker can differ from the selected
standalone compiler and must be recorded and tested truthfully. Upstream changes
and bundle production require their own work; this repository does not depend on
or mutate sibling checkouts.

For the first production distribution, choose the exact Qleisli/std release and
coordinate a qargo release with that linked checker, following the upstream
[compatibility plan](https://github.com/MGYamada/qargo/blob/main/docs/toolchains.md#qleisliup-compatibility-plan).
A visible checker mismatch remains representable for development but does not
complete this release-alignment milestone. No production compiler version is
selected by the illustrative CLI examples above.

Native binaries must be produced and exercised on the three initial hosts.
Linux distribution binaries use static musl linkage; macOS binaries must use the
declared deployment target and only supported runtime dependencies. File format
or successful Rust compilation alone is insufficient evidence. Windows and
Linux ARM64 remain deferred.

## Proposed distribution layout

Use stable HTTPS metadata and target prefixes backed by retained object storage.
The client needs fixed URLs, not a search for the latest GitHub release. The
following are logical paths from the existing contract; no real origin or
production object is created by this plan:

| Area | Logical contents | Publication rule |
| --- | --- | --- |
| Metadata prefix | Numbered roots, timestamp, snapshot, targets and delegated roles | Retain every root needed for sequential rotation; enforce expiry and consistent snapshots. |
| Target prefix: toolchains | `releases/<version>/manifest.json` and `releases/<version>/<host>.tar.zst` | Each exact identity is immutable. Stage and validate all declared hosts before promoting the channel. |
| Target prefix: manager | `qleisliup/<version>/manifest.json` and `qleisliup/<version>/<host>/qleisliup` | Independent manager versioning and immutable targets. |
| Target prefix: channels | `channels/stable.json` and `channels/qleisliup-stable.json` | Authenticate each change with TUF; toolchain and manager promotion are independent. |
| Initial bootstrap delivery | Host-specific native `qleisliup-init` and its verification material | Verify the first executable through a separately documented release identity before execution. |

Maintain offline root keys with a reviewed quorum. Separate toolchain and
manager release signing authority from the online timestamp/snapshot refresh
service. Named custodians, actual thresholds, expiry intervals, storage access,
and the first public root must be approved before activation; this preparation
does not assign them or generate keys. Release promotion must publish the signed
snapshot and timestamp only after the referenced immutable targets and metadata
are present. Rehearse rotation and recovery against retained metadata first.

The first bootstrap executable must embed the approved initial public root and
fixed endpoints in a reviewed build. Its download may be hosted separately from
TUF, but its verification identity must be established independently of the
downloaded bytes. An unsigned download script or a checksum beside a binary does
not replace that initial verification. The exact verification method remains an
activation decision; do not advertise a runnable download command until it exists.

Keep `Source::official()` closed until the approved public configuration and
live repository pass acceptance. Do not add an end-user endpoint/root override
or a Cargo fallback to make an unconfigured client appear operational.

## Preparation available in this repository

Maintainers can build and inspect the native manager candidates:

```sh
cargo fetch --locked
cargo build --release --frozen --bins
sh scripts/check_cargo_free.sh target/release/qleisliup
```

The check runs the existing CLI smoke harness with a restricted PATH. Its
`cargo`, `rustc`, `rustdoc`, and `rustup` entries log invocation and fail, and no
inherited PATH entry can provide the real developer tools. The harness verifies
help/version, local registration and compiler proxies, and truthful rejection
of requests that lack production configuration. It uses a small shell fixture
for the selected compiler. It does not exercise real Qleisli semantics, provide
production authentication, or prove successful network installation.

CI builds and checks release candidates on its existing compiler/host matrix.
It does not publish them. The check script and design documents are included in
the crates.io source package so the same check can run from an extracted source
archive. Release uploads, signing ceremonies, and production metadata are
separate operations.

## Production acceptance scenarios

These are planned acceptance checks, not completed results:

1. On each clean supported host with no Cargo/rustc/rustup/C/C++ toolchain, verify
   the native bootstrap out of band and install the managed manager/proxies.
2. Authenticate and install two real exact toolchain releases. Verify actual
   compiler and qargo behavior, embedded resources, and the linked checker
   identity without a build, source checkout, or developer-tool invocation.
3. Change the exact default, apply a repository pin, use a leading selector,
   and run `sync` from a fresh repository. Check precedence and that installation
   never changes the default or another repository's pin.
4. Disconnect the network. Run both compiler aliases and qargo through the
   existing selection; inspect list/show/which without a metadata refresh.
5. Reconnect, install a newer stable distribution, and update the owned manager.
   Check that old exact releases, pins, and the default remain intact until
   explicitly changed, and that partial downloads do not replace working state.
6. Reject invalid/expired metadata, changed release identities, unsupported
   hosts, and incompatible binaries. Exercise key rotation and old-client
   recovery with the actual release infrastructure.

Record host/linkage evidence and observed results before advertising this path
as available. Initial trust and metadata expiry cannot be bypassed to turn the
current fixture implementation into a public installer.
