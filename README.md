# qleisliup

Qleisli toolchain lifecycle management.

**Exact. Immutable. Authenticated. Explicit. Independent.**

**Qleisli, Qargo, qlippy, qlifmt, and qlidoc are independent projects and are not affiliated with, endorsed by, or sponsored by the Rust Project or the Rust Foundation.**

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

The maintainer has decided to temporarily suspend qleisliup development
to focus on developing Qleisli itself. qleisliup is therefore on hold
at version **0.1.4**.
