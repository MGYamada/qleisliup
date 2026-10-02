# Changelog

Product versions are independent of distribution and state schema versions.

## 0.1.0 - 2026-10-03

- Start a std-only Rust 2024 executable with Rust 1.85 as the MSRV, tracked
  Cargo.lock, and unsafe code forbidden.
- Implement help and version output; reject management commands explicitly.
- Adopt the exact, immutable, authenticated, explicit, independent toolchain
  lifecycle contract and document the ordered implementation stages.
- Add build, formatting, lint, and CLI smoke checks for the initial host matrix.
- Establish Apache-2.0 licensing and repository working guidelines.
- Implement Stage 1 offline selection with complete exact SemVer identities,
  closed repository declarations, nearest-parent discovery, and explicit
  override/environment/repository/default precedence without fallback.
- Add show/list/which, exact installed-release defaults, and atomic pin/settings
  writes with process-scoped mutation locks and symlink/special-file rejection.
- Validate local internal manifests, receipts, remembered release identities,
  complete inventories, and external verifier arrangements without executing tools.
- Distinguish qargo's linked checker and installation-time authentication history;
  keep stable resolution dependent on authenticated channel history.
- Add parsing/filesystem dependencies and isolated local selection/mutation tests;
  extend CI to fetch locked dependencies and run tests before offline validation.
- Implement Stage 2 canonical local link/unlink registration with atomic locked
  updates; accept compiler-only builds and keep them visibly unauthenticated.
- Dispatch qli/qleisli/qargo/qlippy/qlifmt/qlidoc through Unix symlinks using
  absolute-path execve; preserve OS arguments, streams, PID, cwd, and tool status.
- Propagate the selected toolchain/home and prepend its bin for nested calls;
  reject missing tools, recursive manager targets, and implicit shell fallback.
- Add a fixed nix process dependency, native process fixtures, concurrent link
  mutation tests, and registration/proxy CLI smoke coverage.

- Implement Stage 3 with tough 0.24.0, enabled expiry checks, bounded metadata
  transport, persistent rotated roots/rollback state, and atomic datastore snapshots.
- Add authenticated exact/stable installs, override-independent repository sync,
  offline stable inspection/default freezing, and removal with retained identities.
- Verify target streams to completion before bounded tar.zst extraction; reject
  links, special/duplicate/traversal paths, privileged modes, and supplied receipts.
- Validate component manifests/inventory, persist immutable identities, and publish
  complete installations by exclusive same-filesystem rename under the home lock.
- Add signed fixture repositories, rotation/expiry/rollback/republication tests,
  transaction failure and process-kill/retry coverage; preserve unconfigured
  production endpoints/root and test-only keys outside the production binary.
- Compact superseded delegated metadata after validating the working store;
  retain signed snapshot version floors and delegated rollback rejection.
- Apply metadata transport caps by the repository-load phase, allowing target
  endpoints beneath the metadata URL to serve larger authenticated artifacts.
- Reject archive directory aliases caused by filesystem case folding or Unicode
  normalization instead of merging distinct archive paths.
- Reject oversized link registry writes before replacement, preserving the
  readable prior registry on failure; add regressions for all four review fixes.

- Implement Stage 4 with a native qleisliup-init in the same Cargo package and
  the shared private core; keep qleisliup as the default Cargo run target.
- Authenticate the independent manager channel/manifest/executable with tough;
  remember immutable host/version identities and reject channel/current downgrades.
- Stage and version-check a native manager before atomic publication; bootstrap
  publishes the manager, ownership marker, and six proxies as one bin directory.
- Require bootstrap ownership, matching manager identity, and intact proxies for
  self-update; refuse external installations and replace only the managed executable.
- Persist manager identities before replacement and report post-publication errors
  truthfully; preserve prior managers on pre-publication failures and safe retries.
- Add signed manager fixtures, ownership/timeout/republication/concurrency tests,
  process-kill/retry checks, public bootstrap/update CLI tests, and two-binary smoke
  coverage. Production endpoints/root remain unconfigured and fixture keys test-only.
- Canonicalize both manager paths during ownership checks; accept invocation
  aliases of the owned executable while retaining destination, hardlink, and
  authenticated identity validation.
- Terminate version-probe process groups on success and failure, keeping the
  leader waitable until signalling; handle macOS's exited-group behavior only
  after confirming that the group has disappeared. Add native helper regressions.
- Reject archive file/directory aliases of the installer receipt using exclusive
  destination creation; add signed archive and public invocation regressions.

This GitHub source release includes no prebuilt binaries or production TUF
configuration. The package is not published to crates.io.
