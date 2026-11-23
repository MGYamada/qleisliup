# Optional source installation with Cargo

Source installation is an optional developer path for qleisliup 0.1.6.
The intended native end-user distribution remains unavailable; see the
[Cargo-free installation plan](cargo-free-installation.md).
The maintainer has temporarily suspended qleisliup development to focus on
Qleisli itself, as recorded in [README](../README.md).

## Build prerequisites

Building from source requires Rust 1.85 or newer. The package uses Rust edition
2024, fixed dependency versions, and a tracked Cargo.lock. Native aws-lc and
zstd dependencies require a C/C++ toolchain and CMake. macOS builds use Xcode
command-line tools; Linux musl builds also require musl-tools.

## Install the manager

Once version 0.1.6 is available on crates.io, install it with:

```sh
cargo install qleisliup --version 0.1.6 --locked
qleisliup --version
qleisliup-init --version
```

From a checkout of this version:

```sh
cargo install --path . --locked
qleisliup --version
qleisliup-init --version
```

Both executables report version 0.1.6. Cargo installs `qleisliup` and
`qleisliup-init`; it does not install Qleisli, create proxy links, or establish
bootstrap ownership. Production endpoints and the initial trusted TUF root
remain unconfigured, so new toolchain downloads and bootstrap fail closed.

A Cargo-installed manager remains externally owned. Update it with
`cargo install` for the desired exact version; `qleisliup self update` refuses
external installations. Development and package checks are documented in the
[publishing guide](publishing.md).

## Use a local toolchain

On a supported Unix host with a default Cargo installation, create the proxy
links explicitly. Existing commands are not overwritten:

```sh
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
for tool in qli qleisli qlirgo qlinties qlifmt qlidoc qargo qlippy; do
    ln -s qleisliup "$cargo_bin/$tool"
done
qleisliup toolchain link dev /absolute/path/to/local/toolchain
qli +dev --version
```

Ensure that directory is on PATH. If Cargo was installed with a custom `--root`,
use its `bin` directory instead. The local toolchain must contain executable
`bin/qleisli`; compiler-only local builds are accepted and remain unauthenticated.
See the [local link contract](specification.md#local-development-links).

### Use the formal release pair

For a Qleisli 0.3.0 and qlirgo 0.2.0 local toolchain, place `qleisli`, the
matching `qleisli-kernel`, and `qlirgo`, `qlinties`, `qlifmt`, and `qlidoc` in
one `bin/` directory. The upstream releases document how to obtain each
component. [Qleisli 0.3.0](https://github.com/MGYamada/Qleisli/releases/tag/v0.3.0) · [qlirgo 0.2.0](https://github.com/MGYamada/qlirgo/releases/tag/v0.2.0)

Place a schema 2 `toolchain.json` next to `bin/`. The following example uses
the macOS ARM64 releases and records the Qleisli commit used for both compiler
and kernel.

```json
{
  "schema": 2,
  "qleisli": "0.3.0",
  "std": "0.3.0",
  "std_kind": "embedded",
  "qlirgo": "0.2.0",
  "qlirgo_checker_qleisli": "0.3.0",
  "verifier": {"kind": "qleisli-kernel", "path": "bin/qleisli-kernel"},
  "compiler_commit": "c0bbd76238a773b3c532611782fe9d46fc2750be",
  "verifier_commit": "c0bbd76238a773b3c532611782fe9d46fc2750be",
  "host": "aarch64-apple-darwin"
}
```

```sh
qleisliup toolchain link formal /absolute/path/to/local/toolchain
qleisliup +formal show
qlirgo +formal check
```

Run `check` from a qrate with an edition `2026` manifest. The proxy uses the
declared kernel when `QLEISLI_KERNEL` is unset. For existing bootstrap installs
with the original six proxies, add the `qlirgo` and `qlinties` symlinks in the
managed `bin` directory after updating the manager. Self-update preserves
existing proxy layouts.
