# Rust, Lean, and Python environment boundaries

## Adopted policy and delivery status

Resolve Rust and Lean implementation dependencies when producing the native
distribution. Resolve Python application dependencies in a separate environment
owned by each consuming project. qleisliup manages authenticated Qleisli
toolchains; it is not a general language environment or package resolver.

This policy extends [Stage 5](implementation-plan.md#stage-5-cargo-free-end-user-distribution).
The native manager and its local runtime checks exist. A compatible real
Qleisli/qargo bundle, a distributable Lean kernel, and a jointly tested Python
wheel/environment remain upstream delivery and acceptance work. Production
endpoints and trust remain unconfigured. No version in an example chooses an
upstream release or asserts compatibility.

| Layer | End-user environment | Build and dependency owner |
| --- | --- | --- |
| Rust compiler and qargo implementation | Native executables and their supported runtime dependencies | Upstream CI: an exact Rust compiler, tracked Cargo.lock, and fixed build inputs |
| Lean kernel, when supported by the selected release | Native kernel plus every required non-system runtime library/data file | Upstream CI: lean-toolchain and Lake manifests; proof development remains separate |
| Python connection package | A project-local virtual environment | Python project: uv, an exact Python version, exact connection wheel, and locked dependencies |
| Qleisli installation and selection | qleisliup and its native proxies | qleisliup: TUF verification, exact identities, and explicit lifecycle operations |

The promise of installation through qleisliup alone covers the native CLI.
Python connections use a separately distributed wheel/PyPI package. TUF
authentication of a compiler does not authenticate a Python interpreter,
wheel, package index, or project environment. Neither dependency locks nor
installation receipts establish mathematical correctness.

## Native distribution requirements

Rust developers use rustup/Cargo outside installed tool operations. Record the
exact release-build compiler, source commits, dependency locks, build commands,
host, and runtime linkage with each candidate. Preserve MSRV and stable checks;
the moving stable test job is not an exact release-build identity. Normal native
operations must not invoke cargo, rustc, rustdoc, or rustup to repair a missing
installation.

Build any supported Lean kernel in upstream CI and package all required runtime
files. Ordinary execution must not call elan, lake, lean, Python, a C/C++
compiler, or CMake, fetch Mathlib, or rebuild proofs. Proof development uses its
own exact Lean/Mathlib dependencies and Lake manifests; these are not user
installation prerequisites. Execute the relocated bundle without the original
checkout, .lake directory, compiler caches, or build-machine library paths.
Use supported system libraries only and preserve the declared minimum OS and
CPU requirements. Linux musl compatibility must be established for the entire
bundle, including the Lean executable, not inferred from the Rust binaries.

The selected release must describe the actual verifier and its compatibility
with the compiler. A native Lean executable does not itself transfer acceptance
authority from Rust or complete the upstream verification migration. Existing
explicit kernel selection must fail if that kernel is missing, incompatible,
rejects an input, or returns malformed output; it must not silently skip checking.
The manager does not select a backend by searching PATH or invent a new kernel
protocol. The current manifest supports embedded Rust or an external verifier;
do not describe dual checking inaccurately to fit it. If a future arrangement
cannot be represented truthfully, specify its upstream contract and a separately
versioned manifest change before distributing it.

Keep all required native files in the existing immutable release bundle. No
optional-component installer, language proxy, cross-language lockfile, or new
manager command is introduced here. Qargo orchestration must not acquire Cargo,
Lake, uv, or arbitrary host build hooks. Upstream changes belong to separately
scoped work in the owning repositories, not dependencies on sibling checkouts.

## Python project workflow

The Python connection wheel remains separate from the compiler bundle. For the
first supported combinations, produce the CLI and Python wheel from the same
Qleisli release and verify them together. Publish a compatibility matrix for
Qleisli, the wheel, exact tested Python versions, hosts, and optional extras.
Matching version labels alone do not establish compatibility.

Official sample projects must commit .python-version with a full Python patch
version, pyproject.toml with an exact connection-package requirement, and
uv.lock. CI also fixes its uv version and records actual interpreter/platform
identities. Do not introduce guessed pins or a placeholder lockfile in this
manager repository. The sample projects and Python compatibility CI belong to
the upstream Python package work.

Environment preparation is explicit. The following template assumes a future
non-package sample project with committed locks, published compatible wheels,
an exact qleisli-toolchain.toml, and working production distribution. It is not
an installation recipe available from this release:

```sh
qleisliup sync
uv sync --locked --no-build
```

uv may obtain the requested interpreter and wheels during this preparation.
Use binary wheels for native dependencies such as PyQIR. Test with a clean cache
so a locally built cached wheel cannot masquerade as a published supported one.
The sample has no build system, editable requirements, or workspace packages:
`--no-build` is not a general prohibition on first-party/editable builds. Missing
wheels make the optional feature unsupported on that Python/host combination;
do not fall back to a local LLVM or other native dependency build. Basic
connections and QIR extras have separate acceptance results.

Normal execution uses the prepared interpreter and an explicitly resolved
compiler. Run this template from the pinned sample project:

```sh
(
    set -eu
    unset QLEISLIUP_TOOLCHAIN
    QLEISLI_BIN=$(qleisliup which qleisli)
    export QLEISLI_BIN
    .venv/bin/python example.py
)
```

Unsetting the selection override in this subshell ensures the repository pin is
used without changing the parent shell. The assignment is separate from export
so a failed lookup stops execution. `which` returns the absolute compiler path
offline; the Python package already accepts QLEISLI_BIN or
`Client(executable=...)`. Resolve the path at launch instead of committing a
machine-specific home path. Do not parse human-readable `show` output or assume
a compiler identity command that upstream has not provided.

The prepared environment must contain the compatible wheel; qleisliup does not
inspect or synchronize it. A selected Lean kernel is passed explicitly through
the upstream `lean_kernel`/`--lean-kernel` interface using the corresponding
release's documented path. There is no `qleisliup which` kernel selector.
Do not use ordinary `uv run` for this execution template: it can automatically
lock and synchronize before running. Environment upgrades remain explicit,
reviewed changes to the Python lock and Qleisli pin together when required.

See the upstream [Python connection contract](https://github.com/MGYamada/Qleisli/blob/main/python/README.md),
[uv locking and synchronization](https://docs.astral.sh/uv/concepts/projects/sync/),
[Python selection](https://docs.astral.sh/uv/concepts/python-versions/), and
[uv build controls](https://docs.astral.sh/uv/reference/cli/#uv-sync--no-build).

## Validation and delivery order

The reusable maintainer helper accepts an absolute command and preserves its
arguments, streams, working directory, and exit status:

```sh
sh scripts/check_runtime_guard.sh
sh scripts/check_cargo_free.sh target/release/qleisliup
# For a separately prepared, trusted native bundle scenario:
sh scripts/without_developer_tools.sh /absolute/path/to/native-scenario
```

The helper replaces PATH with a small Unix utility allowlist and traps common
Rust, Lean, Python, and native build-tool names. A logged invocation fails the
scenario even if the invoked command's failure was swallowed. The guard checks
its own rejection behavior, argument transport, exit propagation, and exclusion
of inherited PATH entries. It is not a sandbox: absolute tool paths, libraries,
network access, and other inherited environment settings are not blocked. It
does not authenticate the command supplied to it. Use clean hosts and separate
network isolation for complete acceptance. Python application scenarios run in
their own prepared environment, not under a guard that deliberately traps Python.

Delivery order and required evidence:

1. Maintain this policy, the manager's restricted-PATH checks, and the existing
   CLI/pin contract in qleisliup. These do not require an upstream bundle.
2. Validate the full real Rust-built compiler/qargo bundle on macOS ARM64,
   macOS x86_64, and Linux x86_64 musl without developer tools. Record exact
   component identities, linked checker alignment, host linkage, real operations,
   and offline proxy/inspection behavior.
3. Add a Lean kernel only after the upstream release meets its distribution and
   compatibility gates. Validate relocated execution, an unrelated kernel first
   on PATH, absent build caches, and missing/rejecting/incompatible kernel errors.
4. In separately scoped upstream Python work, produce the wheel, locked sample
   projects, and compatibility CI. Recreate two isolated projects from locks;
   test a conflicting PATH compiler and selection override; disconnect networking
   after preparation; reject unsupported combinations and missing native wheels.

Keep the result of each scenario and its actual host/tool identities. The local
manager fixtures do not complete steps 2-4, and bundling/authentication does not
establish a proof claim. Public release, signing, and production activation
remain separate operations.
