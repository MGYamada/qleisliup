# qleisliup 0.1.1 implementation plan

## Delivery boundary

The current delivery is **Stage 4: bootstrap and manager self-update**, following
offline selection, exact defaults/pins, local links, Unix proxies, and TUF
transactional toolchain installation, together
with the adopted [specification](specification.md), development files, tests,
and CI. Production endpoints/trust remain unconfigured. The Cargo package
version is 0.1.1, with source packaging prepared for crates.io. Registry publication
is separate and has not been performed as part of this preparation. The package
includes no prebuilt manager binary or available Qleisli distribution.

Keep delivery in this order: **local selection → links and proxies → TUF and
transactional installation → bootstrap and self-update**. Each later stage
requires an explicit follow-up implementation task. Do not change Qleisli or
qargo as part of local development.

The next requested delivery goal is **Cargo-free end-user installation and
version management**. The native client implementation is present; native
distribution and production activation are tracked below as Stage 5. Source
publication to crates.io remains an optional installation channel.

## Stage 0: design and scaffold

**Completed startup baseline.** The original executable used the standard
library alone; Stage 1 adds fixed parsing/filesystem dependencies.

- Keep one Cargo package, Rust edition 2024, MSRV 1.85, Apache-2.0, and unsafe
  code forbidden. Track Cargo.lock. Do not add dependency placeholders, a public
  Rust library, empty subsystem modules, proxies, or a bootstrap executable.
- Implement help/version and explicit rejection of unsupported invocations.
  Derive runtime version output from Cargo.toml. No manager state or network I/O.
- Keep README, help, changelog, and specification synchronized about implemented
  behavior. Describe later CLI/format examples as planned and illustrative.
- CI builds, checks formatting, denies clippy warnings, and runs the CLI smoke
  check on Rust 1.85.0 and stable on all three initial hosts. CI does not publish.

Acceptance: build/format/lint checks pass; help/version succeed; lifecycle and
invalid invocations fail with status 2 and stderr diagnostics. The smoke check
uses an isolated manager home and confirms no manager state is created.

### Startup validation, 2026-10-02

Local validation on macOS ARM64 (`aarch64-apple-darwin`):

| Compiler | Build | Formatting | Clippy, warnings denied | CLI smoke |
| --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | Passed | Passed | Passed |

At the startup baseline, Cargo metadata confirmed one executable package and no
dependencies. Local documentation links, the illustrative release JSON,
source/document whitespace,
and shell syntax were checked. The CI YAML was parsed and its six compiler/host
combinations checked for assigned runners. Linux and macOS x86_64 execution,
and the remote CI workflow itself were not run locally. This table records only
the help/version scaffold; Stage 1 validation is recorded separately below.

## Stage 1: local selection and inspection

**Completed.** Internal exact-version and selector types,
repository declaration parsing/discovery, home/state loading, and a local
resolver return the chosen identity, source, and path. The package remains a
single binary at this checkpoint, with no qargo-facing library API. Show/list/which inspect without
executing tools; leading selectors apply to show/which. Exact defaults require
an existing complete release. Pin writes only in the current directory and
does not require an installation.

Fixed semver, serde/serde_json, toml, and Unix rustix dependencies implement the
parsing and filesystem boundary; tempfile is test-only. Track the locked graph
and keep Rust 1.85 support. Metadata reads are bounded, schemas are closed,
and symlink/special-file records fail. Atomic writes use synced temporary files,
rename, and process-scoped home/destination-directory locks. Malformed existing
settings/declarations are preserved on validation failure.

At this stage, stable resolution failed until Stage 3 supplied authenticated
channel handling; manually writing a channel record could not enable it.
Existing local registrations were inspectable; Stage 2 added registration and
process dispatch below.
Release inspection validates inventory and manifest/receipt/remembered identity
agreement, without claiming current integrity or re-authenticating distributions.

| Acceptance scenario | Required result |
| --- | --- |
| Exact versions, including valid prerelease/build metadata | Preserve their complete identity. |
| Stable/range/linked/shortened version in a repository declaration | Reject; no interpretation as a release constraint. |
| All four selection sources present | Leading override wins; then test each source after removing higher priorities. |
| Empty override, malformed nearest declaration, missing selected release | Fail at the selected source without fallback. |
| Nested directories and declarations | Choose nearest declaration; search to filesystem root. |
| `pin` in a nested directory | Write only there; do not change an ancestor declaration or global default. |
| Invalid or unsupported state schema; invalid QLEISLIUP_HOME | Fail explicitly. |
| Read-only inspection or help/version | No state creation, network requests, or tool execution. |
| Exact default with malformed selection overrides | Use the explicitly requested installed release; do not interpret unrelated selection sources. |
| Missing/inconsistent manifest, receipt, remembered identity, or required executable | Reject the release; preserve the old default. |
| qargo checker differs from the selected compiler | Display both identities and the independent checker boundary. |
| Embedded/external verifier records | Require only the actual external executable/protocol; embedded Rust has no external protocol. |
| Existing compiler-only local registration | Inspect as linked/local/unauthenticated; a missing requested tool fails. |
| Oversized metadata, symlink destination, or non-UTF-8 override | Reject explicitly without executing a tool. |
| Concurrent pin/default writers | Leave complete valid files, remove owned temporary files, and release locks after process exit. |
| Stable selection without an authentication implementation | Fail even when a local channel-shaped fixture exists. |

### Stage 1 validation, 2026-10-02

The suite contains 3 identity unit tests and 21 CLI integration tests. Fixtures
live in isolated temporary homes; executable markers detect accidental tool
execution. Synthetic local receipts test consistency, not TUF authentication.
CI now fetches locked dependencies, then builds, tests, lints, and runs the smoke
check offline on both Rust versions for all three initial hosts.

Local validation on macOS ARM64 (`aarch64-apple-darwin`):

| Compiler | Build | Tests | Formatting | Clippy, warnings denied | CLI smoke |
| --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 24 passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 24 passed | Passed | Passed | Passed |

These build/test/lint/smoke commands used the locked dependency graph offline.
The package remains a single executable. Local document links, illustrative JSON,
source/document whitespace, shell syntax, and the CI compiler/host matrix were
also checked. Linux and macOS x86_64 execution and remote CI have not been run
locally; the workflow is configured to check those hosts.

At this checkpoint process dispatch was deferred to Stage 2. Network installation,
root rotation, archive extraction, bootstrap, and self-update remain deferred.

## Stage 2: local links and Unix proxies

**Completed local process stage.** Link/unlink validate and persist canonical
local registrations with atomic locked updates. Compiler-only builds are valid;
optional manifests must be consistent, and all links remain unauthenticated.
Duplicate registrations fail until unlinked; unlink also removes broken records
without deleting the build directory.

Multicall symlink dispatch selects qli/qleisli/qargo/qlippy/qlifmt/qlidoc and
uses the absolute tool path with nix execve. Preserve OS argument bytes, PID,
streams, cwd, and tool exit/signal status. Pass the resolved toolchain/home and
prepend its bin for nested calls; qargo's same-release auxiliaries remain siblings.
Reject missing tools, symlinks/special files, recursive manager identities,
and exec failures without another toolchain, PATH search, or implicit shell.
At this stage, no command created proxy symlinks automatically; development links
could reference the build output. Stage 4 adds managed bootstrap below.

| Acceptance scenario | Required result |
| --- | --- |
| Compiler-only local link | Compiler runs; missing qargo/auxiliary tools fail without PATH fallback. |
| Link without a manifest | Display local/unauthenticated and unknown component versions. |
| `qli` and `qleisli` on the same selection | Execute the same compiler file. |
| Leading override, arguments with spaces/non-UTF-8 bytes, forwarded flags | Remove only the selector and preserve the remaining arguments. |
| Stdin/stdout/stderr, working directory, exit codes, signals | Observe the selected process's behavior without an intermediary shell. |
| Nested proxy call from a selected tool | Retain the parent's resolved selection. |
| Missing/broken tool, or link recursively targeting the manager | Fail explicitly; never recurse or borrow another tool. |
| qargo's embedded checker differs from standalone compiler | Display both identities; make no claim that proxying changes qargo checking. |
| All proxy invocations with a blocked network transport | Work or fail locally; make zero download/refresh requests. |
| Concurrent distinct/duplicate registrations and unlinks | Preserve independent names; exactly one duplicate registration succeeds. |
| Invalid or symlink link state; invalid build/manifest | Fail and preserve existing records; no registration is published. |
| Unset/empty/non-UTF-8 PATH; bin path containing `:` | Preserve representable PATH bytes; reject an unrepresentable bin before exec. |
| Executable text without a shebang or missing interpreter | Report exec failure; never reinterpret text through a shell. |

Use temporary test executables to check process transport. Do not invoke real
package management or compilation through a proxy test. Upstream identity JSON
and an external captured-source checker protocol remain separate prerequisites
for a future qargo migration, not gates for these process tests.

### Stage 2 validation, 2026-10-02

Local validation on macOS ARM64 (`aarch64-apple-darwin`), with the locked
dependency graph and no network during build/check execution:

| Compiler | Build | Tests | Formatting | Clippy, warnings denied | CLI smoke |
| --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 40 passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 40 passed | Passed | Passed | Passed |

The total comprises 3 identity unit tests, 21 selection/state integration tests,
and 16 link/process integration tests. The latter compile a standalone native
fixture, exercise all six symlink names, and verify byte-preserving arguments,
PID/streams/cwd/status, SIGTERM, nested PATH selection, sibling tools, concurrent
registration, and failure paths. The CLI smoke check separately registers a
compiler-only script build, forwards tool flags through both compiler aliases,
and unlinks without deleting the build.

A seventeenth link/process case uses a non-UTF-8 directory name and is Linux-only
because macOS filenames require Unicode. It is configured for CI and has not
been run locally; arbitrary non-UTF-8 process arguments/PATH bytes were checked
on macOS. The fixed nix 0.29.0 dependency and its locked graph compile on Rust
1.85.0. Local document links, JSON examples, whitespace, shell syntax, and the
six-job CI matrix were checked. Linux, macOS x86_64, and remote CI execution
remain unperformed locally. TUF/install/bootstrap/self-update cases remain
deferred to the following stages.

## Stage 3: TUF and transactional release installation

**Current delivery, implemented with isolated fixtures.** Use tough 0.24.0,
locked dependencies, enabled expiry, immutable datastore generations, and a
persisted trusted root. No production endpoint/root or trust override is enabled.

Integrate tough after validating its complete dependency graph on Rust 1.85 and
the host matrix. Pin/lock the adopted dependencies and document their actual
build requirements. Keep expiration enforcement enabled. Use a persistent
metadata datastore and persist the rotated trusted root; never restart each
refresh from temporary state or an old embedded root while ignoring saved trust.

Build isolated test TUF repositories with fixture-only keys. Inject
transport at the internal boundary so expiry, rotation, and failure tests do not
depend on live production services. Test trust roots must be unreachable from
the production build configuration. No production signing keys are generated
or configured in the executable. tough uses the real clock and persisted last
observed time; deterministic dates in 2000/2100 cover expiry and clock rollback
without disabling `Safe` or introducing a production clock override.

Implement authenticated channel/manifest/artifact lookup, preserved identities,
safe tar.zst extraction, internal inventory validation, receipts, state locking,
and same-filesystem publication. Add install/sync/uninstall and stable-aware
default selection. Already installed exact releases stay usable offline.

| Acceptance scenario | Required result |
| --- | --- |
| `sync` with a CLI/environment selection override | Target the nearest repository declaration and ignore overrides. |
| Stable default followed by a newer stable install | Resolve/store the original exact installed version; later installs do not change the default. |
| Valid root rotation across multiple intermediate roots | Accept sequentially using the old/new threshold rules; persist updated trust. |
| Invalid signature, missing delegation, skipped root version | Reject without publishing an installation. |
| Expired metadata, timestamp/snapshot rollback, mix-and-match metadata | Reject using TUF; do not disable expiry to recover. |
| Repeated refresh after process restart | Retain metadata rollback protection and channel high-water marks. |
| New signed channel points to an older software version | Reject the semantic downgrade; allow an explicit exact older release install. |
| Changed release manifest/artifact under a known identity, including after uninstall | Reject republication; keep the remembered digest. |
| Incorrect size/digest or error at the end of the download stream | Never unpack or publish the unverified bytes. |
| Qleisli/std mismatch, wrong host, false component arrangement, absent inventory | Reject before final rename. |
| Embedded stdlib/verifier bundle | Accept without requiring a nonexistent std directory, verifier executable, or protocol. |
| External verifier bundle | Require its declared executable and actual protocol fields. |
| Absolute/traversal paths, links, special files, duplicates, decompression-limit violations | Reject without writing outside staging. |
| Payload file/directory aliases of the installer receipt | Reject using the extraction filesystem's naming rules before identity persistence or publication. |
| Concurrent installs or interrupted state updates | Serialize mutation and expose only complete final installations. |
| Failure at each transaction boundary | Preserve existing releases/defaults; clean owned temporary data where possible. |
| Crash after identity persistence but before rename | Retry the same authenticated identity safely; no half-installed entry. |
| Receipt inspection or local file modification | Report past authentication; do not claim current integrity or correctness. |

Exercise archive inventory and host executable compatibility independently of
Rust build success. Validate real bundles with their pinned upstream components
before announcing compatibility. The current qargo 0.1.5 Linux ARM64 limitation
does not become supported through this installer.

### Stage 3 validation, 2026-10-02

Local validation on macOS ARM64 (`aarch64-apple-darwin`), using the locked graph
offline after explicitly fetching dependencies:

| Compiler | Build | Tests | Formatting | Clippy, warnings denied | CLI smoke |
| --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 64 passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 64 passed | Passed | Passed | Passed |

The total is 25 unit tests (including the subprocess fixture entry), 23 local
selection/CLI tests, and 16 link/process tests. Internal TUF fixtures use test-only
Ed25519 keys, consistent snapshots, separate delegated release/channel roles, and
synthetic bundles; no live service or production signing ceremony is involved.

Performed checks include signed installs and historical receipts; embedded and
external arrangements; qargo checker differences; exact offline reuse; stable
high-water marks/default freezing; CLI/environment-independent nearest-pin sync;
sequential root rotation persisted even after a later failed refresh; signature,
expiry, timestamp/snapshot rollback, backward clock, delegation, and metadata
mix-and-match rejection; corrupt cached state; immutable identities after removal;
final digest/length and late transport errors; signed metadata above the absolute
transport limit; unsafe paths/links/special files/duplicates/privileged modes;
individual extraction bounds, truncated frames, and hidden trailing payloads;
manifest/host/binding/inventory mismatches; concurrent installs; eight injected
transaction boundaries; and an actual killed subprocess after durable identity
persistence, followed by retry with the old release/default intact.

The CLI smoke check confirms unconfigured new installs fail with status 1 and
produce no success output or home, while help/version, local registration, and
both compiler proxy names continue working. The final filesystem pass also syncs
new directory entries/ancestors, so durable records do not depend on an unsynced
home or metadata parent. Local document links, JSON examples, whitespace, shell
syntax, and the six-job CI YAML/matrix passed. CI installs native Linux build
prerequisites for aws-lc/zstd and checks both compilers on the initial hosts.

Linux/macOS x86_64 and remote CI execution remain unperformed locally. The
Linux-only filename case remains configured for CI. Real compiler/qargo bundle
compatibility, live HTTPS behavior, production keys/thresholds/endpoints, storage
fault/power-loss testing, independent security review, bootstrap, and self-update
are not established by these fixture tests. Abandoned private transactions may
remain after process death; inspection ignores them. No release/tag/publication
or sibling-repository change was made.

### Stage 3 review validation, 2026-10-02

The [local review](review-stage3.md) reproduced and corrected four P2 defects:
superseded delegated cache accumulation, artifact limits selected by overlapping
URL prefixes, archive directory aliases, and oversized link registry writes.
Regression checks include delegated rollback after cache compaction and
byte-for-byte preservation of the prior registry after a rejected write.

Final checks ran on macOS ARM64 with the unchanged locked dependency graph:

| Compiler | Build | Tests | Formatting | Clippy, warnings denied | CLI smoke |
| --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 68 passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 68 passed | Passed | Passed | Passed |

The total is 28 unit tests, 23 selection/CLI tests, and 17 link/process tests.
Both formatting checks include the standalone native proxy fixture. Document
whitespace/local links, shell syntax, and `git diff --check` also passed.
All prior Stage 3 tests were rerun; the host, production-service, power-loss, and
independent-audit limitations above still apply.

## Stage 4: bootstrap and manager self-update

**Implemented with unconfigured production trust.** The single Cargo package
builds qleisliup and qleisliup-init from the shared private core. No public
integration library is introduced, and qleisliup remains the default run target.
Bootstrap authenticates the independent manager channel/manifest/native target,
verifies a staged executable and its version, then atomically publishes the
manager, six proxy links, and immutable ownership marker as one bin directory.
It requires an absent bin destination and leaves shell files and selection alone.

Self-update checks the managed path, ownership marker, hardlink count, local
digest/recorded running version, and proxy destinations; refuses external or stale
executables; authenticates the manager channel and immutable identities; and
rejects downgrade/republication. It stages within bin, persists authenticated
identities before rename, and replaces only the manager file. Existing TUF state,
toolchain records, releases, pins, defaults, links, proxies, and ownership marker
are preserved. Same-version refresh still checks the authenticated manifest.
Errors after publication report the changed state truthfully; interrupted
pre-publication transactions preserve the old manager and can retry.

Production trust/endpoints remain a separate prerequisite; there is no CLI or
environment fixture source/root override. The initial bootstrap authenticity
method and production embedded root must be established through the release
prerequisites below before these commands can perform live installations.

| Acceptance scenario | Required result |
| --- | --- |
| Bootstrap into a fresh managed home | Install the authenticated manager and six proxy symlinks; no toolchain install. |
| Existing unrelated command at a destination | Refuse to overwrite it; leave it usable. |
| Unsupported host | Fail before downloading or changing state. |
| Invalid manager target, download/interruption/staging failure | Keep the old manager and its proxies usable. |
| Valid self-update | Replace only the manager; proxy links still work; pins/defaults/releases remain unchanged. |
| Self-update proposes an older manager | Reject downgrade. |
| Cargo/package-manager-owned executable | Refuse self-replacement and name the owning installation method when known. |
| Managed executable invoked through a symlink or filesystem case alias | Accept its canonical path while retaining destination and identity checks. |
| Version probe starts helper processes | Terminate the probe group on success, timeout, excess output, abnormal exit, or version mismatch. |
| Install/sync of any Qleisli release | Never update qleisliup implicitly. |
| Same-version self-update after republication | Authenticate the manifest and reject changed remembered identity. |
| Concurrent updates by old processes | Publish once; reject the stale running version without modifying the winner. |
| Killed update after identity persistence | Keep the old manager usable and retry with the remembered candidate identity. |
| Completion error after executable rename | Report publication; the new version works with the existing marker and identity ledger. |
| Unconfigured production source or invalid public CLI | Fail without success output or creating manager state. |

### Stage 4 validation, 2026-10-02

Local checks ran on macOS ARM64 (`aarch64-apple-darwin`) with the unchanged
locked dependency graph, offline:

| Compiler | Build, both binaries | Tests | Formatting | Clippy, warnings denied | CLI smoke, both binaries |
| --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 127 passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 127 passed | Passed | Passed | Passed |

There are 85 named cases: 42 shared unit cases (including two subprocess fixture
entrypoints), 23 selection/CLI cases, 17 link/process cases, and 3 public lifecycle
CLI cases. Cargo runs the shared 42-case unit suite in both binary harnesses,
giving 127 executions. After a test-only string-formatting lint correction, stable
format/clippy and the affected lifecycle CLI target were rerun successfully;
Rust 1.85.0 then passed its complete suite. Both standalone native fixtures were
format-checked. No Rust dependency was added or updated for Stage 4.

Signed fixture checks cover fresh bootstrap; six relative symlinks and untouched
shell state; absent-bin and unsupported-host boundaries; external, modified,
hardlinked, symlinked, or misdirected manager rejection; independent manager and
toolchain channels; same-version refresh and republication; channel/current
downgrade; manifest version/host/path/digest/size/inventory bindings; expired TUF
state; native executable/version/exit/output/timeout rejection; late target stream
errors; seven pre-publication failure points for both lifecycle operations;
concurrent updates and stale-process rejection; an actual process kill after
durable identity persistence and safe retry; and truthful errors after executable
rename with the new identity immediately usable. Previous Stage 1–3 checks were
also rerun in both harnesses.

The public CLI tests and smoke check confirm help/version work without state,
invalid syntax fails with 2, missing production configuration fails with 1 and
no success output/state creation, and external self-update uses the original
installation-method diagnostic. A synthetic managed copy of the real manager
fails closed without production trust while its compiler proxy remains usable.
Local document links/JSON/whitespace, shell syntax, Cargo package/binary identity,
and the six-job CI matrix were checked separately.

Linux/macOS x86_64 and remote CI execution remain unperformed locally. Actual
production URLs/root, bootstrap download authenticity, signing/rotation ceremony,
real host CPU/linkage compatibility, mount-point execution, power-loss/storage
fault testing, and independent security review remain separate release gates.
Killed processes can leave private staging; automatic cleanup is deferred.
No sibling checkout, release/tag/key ceremony, distribution publication, or
registry publication was changed.

### Stage 4 review validation, 2026-10-03

The [local review corrections](review-stage4.md) cover two P2 defects (manager
invocation aliases and surviving version-probe helpers) and one P3 defect
(archive aliases of the reserved installer receipt). Regression tests failed
against the previous implementation, then passed after correction. Process
cleanup also handles macOS's zombie-only group result without accepting a
live or inaccessible group. The existing rustix dependency enables its process
feature; dependency versions and Cargo.lock are unchanged.

Final checks ran on macOS ARM64 with the locked graph, offline:

| Compiler | Build, both binaries | Tests | Formatting | Clippy, warnings denied | CLI smoke, both binaries |
| --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 135 passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 135 passed | Passed | Passed | Passed |

There are 89 named cases: 46 shared unit cases, 23 selection/CLI cases, 17
link/process cases, and 3 public lifecycle CLI cases. Both binary harnesses run
the 46-case unit suite, giving 135 executions. Four new unit cases cover signed
updates through invocation aliases, signed file/directory receipt aliases, failed
probe helper cleanup, and successful probe helper cleanup. The public managed
CLI case also checks invocation aliases. Formatting includes both standalone
native fixtures. An initial lint invocation selected Homebrew's clippy alongside
the MSRV compiler; the final MSRV lint check explicitly selected clippy 0.1.85
and passed, followed by the smoke check.

Local documentation links/anchors, whitespace, shell syntax, and lockfile
stability were checked. Linux/macOS x86_64, live production distribution, real
bundles, power-loss/storage faults, and an independent security audit remain
unperformed locally. Process-group cleanup covers inherited helpers and is not
a sandbox for authenticated code intentionally creating a different group or
session. Production trust/endpoints remain unconfigured; no sibling repository
or publication operation was changed.

## Version 0.1.1 packaging validation, 2026-10-03

Prepared the crates.io source package with an explicit include list, registry
metadata, Cargo installation/proxy instructions, and a
[publishing guide](publishing.md). Runtime behavior and dependency versions are
unchanged. The proxy test now derives the manager version from Cargo.toml, and
the smoke script reads only the package section of normalized Cargo manifests.

Local checks ran on macOS ARM64 (`aarch64-apple-darwin`):

| Compiler | Build, both binaries | Tests | Formatting | Clippy, warnings denied | CLI smoke, both binaries | Package verification |
| --- | --- | --- | --- | --- | --- | --- |
| Rust 1.85.0 | Passed | 135 passed | Passed | Passed | Passed | Passed |
| Rust 1.98.1, installed Homebrew stable release | Passed | 135 passed | Passed | Passed | Passed | Passed through publish dry-run |

Builds, tests, formatting, and lints used the locked graph offline. Initial
offline packaging attempts required uncached registry lookups; online
`cargo package --locked --allow-dirty` and
`cargo publish --dry-run --locked --allow-dirty --registry crates-io` then passed,
including compilation of the extracted package. The dry-run explicitly aborted
before upload. CI now verifies source packaging with `--locked` on its existing
six compiler/host jobs, permitting registry lookups for this step.

At this packaging checkpoint, both archives contained 41 files. LICENSE, NOTICE, the complete Cargo.lock,
executable entry points, native fixtures, and smoke script matched the checkout.
All test targets compiled from the extracted stable package, and 21 focused
cases passed: 17 link/proxy cases, 3 public manager CLI cases, and the signed
bootstrap case exercising the packaged native manager fixture. The normalized
manifest smoke check passed. A release-profile Cargo install from the extracted
source into an isolated directory also passed the complete smoke check and
reported version 0.1.1 for both installed executables.

Local documentation links/examples, shell syntax, whitespace, and unchanged
dependency lock entries were checked. The source was uncommitted for these
preparation checks; publication still requires a clean release candidate.
Linux/macOS x86_64 execution, remote CI, registry ownership/upload authorization,
and the production distribution/security gates below remain unverified here.
No registry upload, GitHub release, tag, or production distribution was created.

## Stage 5: Cargo-free end-user distribution

**Requested direction; production distribution remains planned.** Deliver a
verified native bootstrap plus prebuilt Qleisli toolchains for the initial three
hosts. End users must not need Cargo, rustc, Rust headers/libraries, a C/C++
compiler, or CMake. See the [delivery plan](cargo-free-installation.md).

Current preparation builds both release-mode manager executables and checks
local CLI/link/proxy behavior with a restricted PATH whose Rust developer tool
names are failure traps. This is configured for both Rust compilers on each CI
host. Test fixtures do not establish real toolchain installation or production
authentication. No release upload or production trust configuration is added.

Remaining delivery work, in order:

1. Validate independently produced native Qleisli/qargo bundles, exact component
   identities, embedded stdlib/verifier arrangements, and real host linkage.
2. Establish the production TUF authority, endpoint ownership, initial public
   root, rotation/expiry policy, and verified bootstrap acquisition method.
3. Configure the client with those approved public inputs and stage signed
   toolchain and manager targets through the separate release process.
4. On clean hosts without Rust developer tools, verify bootstrap, install,
   exact/default/project selection, offline execution, updates, and rejection
   of invalid distributions before making the end-user path available.

No download URL, signing identity, or initial compiler bundle version is inferred
from examples. Upstream source/build changes belong to their own repositories.

### Preparation validation, 2026-10-03

On macOS ARM64, Rust 1.85.0 and Homebrew stable Rust 1.98.1 both passed locked,
offline release builds of qleisliup/qleisliup-init and the restricted-PATH runtime
smoke check. A negative check confirmed that invoking Cargo and swallowing its
failure still fails the wrapper. Both compiler builds produced ARM64 Mach-O
executables; inspection of the stable binaries found only system dynamic library
dependencies. This is local linkage evidence, not minimum-OS validation.

Both compilers also verified the updated 43-file source archive. Its extracted
runtime check script passed against the corresponding native release candidates,
including reading Cargo's normalized manifest. Documentation links/examples,
shell syntax, CI YAML and its six-job matrix, and whitespace checks passed.
The prior 135-execution Rust suites remain the code-validation checkpoint above;
this follow-up changed documentation, packaging, shell checks, and CI only.

No real compiler bundle, production trust root, endpoint, signature, or hosted
bootstrap was introduced. Other-host native execution, remote CI, successful
live installation/update without Rust, and clean-machine upstream compatibility
remain future acceptance work.

## Production prerequisites and release boundary

Operational release readiness requires completion of Stages 1–4 and independent
review of distribution/security behavior. Production deployment additionally
requires named signing-key custodians, role thresholds, expiration/refresh
schedules, an initial root ceremony, retained root-rotation history, immutable
target storage, fixed metadata/target endpoints, and an out-of-band bootstrap
verification method. These require a separate production configuration task;
missing configuration must fail closed rather than silently use fixtures.

Validate Cargo-free bundles on macOS ARM64/x86_64 and Linux x86_64 musl, including
CPU identity, linkage, the real qargo checker binding, and install/exec behavior.
Plan Windows and Linux ARM64 only in a later scope. A source build or a receipt
does not establish distribution readiness or a mathematical guarantee.

Record checks actually performed separately from these future acceptance cases.
Packaging, tagging, pushing, and public publication are distinct operations.
This implementation plan neither creates a release nor authorizes publication.
