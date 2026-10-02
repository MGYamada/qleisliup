# qleisliup working guidelines

- Read README.md, docs/specification.md, and docs/implementation-plan.md before
  changing behavior. User instructions take precedence.
- Stages 1–4 implement offline selection/inspection, exact default/pin writes,
  local links, Unix proxies, TUF/transactional installation, bootstrap, and manager
  self-update. Production
  endpoints and initial trust remain unconfigured; fixture keys are test-only.
  Follow the ordered stages; add later lifecycle work only when requested. Mark
  planned interfaces separately and never report unimplemented operations as successful.
- qleisliup owns toolchain lifecycle only. Do not add Qleisli semantics, qrate
  resolution, a qargo integration library, or dependencies on sibling checkouts.
- Use a single Cargo package, Rust edition 2024, MSRV 1.85, and forbid unsafe
  code. Use established parsing/filesystem libraries with fixed versions; preserve
  the MSRV for the locked dependency graph. Track Cargo.lock and ignore build output.
- Repository pins are exact SemVer. Keep selection precedence and explicit
  installation boundaries; proxy execution and inspection must stay offline.
- Preserve installed release identities. TUF is the distribution acceptance
  authority; use tough with the persistent datastore. Do not add bespoke signing,
  disable expiry enforcement, or embed test keys as production trust anchors.
- Record embedded stdlib/verifier arrangements truthfully. qargo's linked checker
  version may differ from the selected compiler. An install receipt records past
  authentication, not current integrity or mathematical evidence.
- Stage and verify before publication; never overwrite an installed release.
  Compiler-only local links remain visibly local and unauthenticated.
- Specifications, public docs, diagnostics, comments, and examples are English.
  Keep implementation status synchronized across README, help, and design docs.
- Cargo.toml is the product version source. Preserve Apache-2.0, LICENSE, NOTICE,
  and Masahiko G. Yamada's copyright. Record actual changes in CHANGELOG.md.
- Run build, tests, fmt --check, clippy with warnings denied, and the CLI smoke check on
  Rust 1.85.0 and stable for Rust changes. Run relevant checks for later stages;
  distinguish performed checks from deferred acceptance scenarios.
- Do not change sibling repositories or create releases, tags, signing keys,
  distribution metadata, or registry publications as part of local development.
