# Optional source installation with Cargo

Source installation is an optional developer path for qleisliup 0.1.4.
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

From crates.io:

```sh
cargo install qleisliup --version 0.1.4 --locked
qleisliup --version
qleisliup-init --version
```

From a checkout of this version:

```sh
cargo install --path . --locked
qleisliup --version
qleisliup-init --version
```

Both executables report version 0.1.4. Cargo installs `qleisliup` and
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
for tool in qli qleisli qargo qlippy qlifmt qlidoc; do
    ln -s qleisliup "$cargo_bin/$tool"
done
qleisliup toolchain link dev /absolute/path/to/local/toolchain
qli +dev --version
```

Ensure that directory is on PATH. If Cargo was installed with a custom `--root`,
use its `bin` directory instead. The local toolchain must contain executable
`bin/qleisli`; compiler-only local builds are accepted and remain unauthenticated.
See the [local link contract](specification.md#local-development-links).
