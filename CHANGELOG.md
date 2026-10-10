# Changelog

Product versions are independent of distribution and state schema versions.

## 0.1.5 - 2026-10-11

- Expand README's independence notice to include qleisliup and qleisliup-init,
  use lowercase qargo consistently, and replace the opening tagline with it.
- Add README's attribution that Rust and Cargo are trademarks of the Rust
  Foundation.
- Update the Cargo package and both executable version outputs to 0.1.5;
  synchronize current documentation and add release preparation notes.
- Keep runtime behavior, dependency versions, and state formats unchanged.

## 0.1.4 - 2026-10-09

- Update the Cargo package and both executable version outputs to 0.1.4.
- Synchronize current documentation and add version preparation notes; retain
  historical release records.
- Replace README's current status with a temporary development suspension
  notice at 0.1.4 while the maintainer focuses on Qleisli itself; remove all
  subsequent README sections.
- Preserve optional source installation and native build prerequisites in a
  dedicated guide; update current references and pin historical README links
  to their corresponding release tags.
- Keep runtime behavior, dependency versions, and state formats unchanged.

## 0.1.3 - 2026-10-07

- Update the Cargo package and both executable version outputs to 0.1.3.
- Synchronize current documentation and add version preparation notes; retain
  historical release records.
- Clarify in the README that Qleisli, Qargo, qlippy, qlifmt, and qlidoc are
  independent projects without affiliation, endorsement, or sponsorship from
  the Rust Project or the Rust Foundation.
- Keep runtime behavior, dependency versions, and state formats unchanged.

## 0.1.2 - 2026-10-03

- Adopt separate Rust/Lean distribution-time dependency resolution and
  project-local Python environments, with separate wheel/PyPI delivery, exact
  Python locks, explicit compiler handoff, and upstream compatibility gates.
- Extend the restricted-PATH native runtime check to Lean, Python, environment
  managers, and native build tools; expose a reusable maintainer command guard
  and check that swallowed tool failures still reject a scenario.
- Include the guard and its checks in source packages and the existing six-job
  CI matrix. Real bundles, native Lean delivery, Python compatibility CI, and
  production trust remain separate acceptance work.
- Create owned home/state/staging directories with mode 0700 regardless of a
  permissive umask; reject existing group/world-writable owned directories,
  including during offline state and release inspection (#7).
- Randomize exclusive state-write temporary names and reclaim bounded batches
  of crash-left current and legacy temporaries under the mutation lock, without
  following links or touching committed records (#8).
- Share the exact six-character ASCII alphanumeric generation-name rule between
  TUF pointer loading, staging allocation, and stale cleanup; preserve active
  generation protection by filesystem identity (#9).
- Exclude macOS Finder metadata from the explicit source package include list.

## 0.1.1 - 2026-10-03

- Update tar to 0.4.45 to address RUSTSEC-2026-0067 and RUSTSEC-2026-0068;
  retain the manager's own bounded extraction and archive-entry restrictions.
- Prepare crates.io publication with package keywords, categories, documentation
  metadata, and a crates.io-only publication setting.
- Define the source package contents explicitly, including LICENSE, NOTICE,
  the locked dependency graph, documentation, tests, and native test fixtures.
- Document Cargo installation, explicit proxy setup, external-manager updates,
  and package validation; retain unconfigured production distribution and trust.
- Add source package verification to the existing compiler/host CI matrix.
- Derive the proxy version assertion from Cargo.toml and make the CLI smoke
  check read the package version correctly from Cargo-normalized manifests.
- Make Cargo-free native installation and exact version management the explicit
  end-user goal; document the remaining artifact, trust, and clean-host work.
- Add native release candidate builds to CI and a restricted-PATH runtime smoke
  check that rejects invocation of Rust developer tools during local operations.
- Normalize authenticated archive executables to mode 0755 and data files to
  0644; retain rejection of privileged modes and missing executable bits (#2).
- Resolve `default stable` again under the home mutation lock before validating
  and saving its exact version, avoiding a concurrent channel-update race (#3).
- Reclaim bounded abandoned install/removal, metadata, bootstrap, and update
  staging under the home lock, preserving committed state and rejecting unsafe
  filesystem substitutions (#4).

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
