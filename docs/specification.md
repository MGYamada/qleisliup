# qleisliup 0.1.0 specification

## Status and responsibility

This is the adopted design contract for the first operational manager. **Stages
1–4 implement offline local selection/inspection, exact defaults and pins, local
links, Unix proxies, TUF authentication, transactional installation/removal,
bootstrap, and manager self-update.**
Production distribution endpoints and the initial trusted root are unconfigured;
new installs, bootstrap, and manager refreshes fail closed.
Examples describe formats and
interfaces; Qleisli 0.4.0 and qargo 0.2.1 are illustrative distribution versions,
not announcements of published or compatible artifacts.

qleisliup manages the installation, selection, inspection, and removal of Qleisli
toolchains. It authenticates distribution bytes and manages its own executable.
It does not interpret Qleisli semantics, resolve qrates, build source checkouts,
or certify compiler transformations, verifier correctness, or mathematical proofs.

| Project | Responsibility |
| --- | --- |
| Qleisli | Language implementation, stdlib, and verification authority |
| qleisliup | Toolchain lifecycle and authenticated distribution |
| qargo | Qrate lifecycle and its own checking/tool orchestration |
| qrate | Qleisli package unit |

Separate repositories and product versions permit a jointly tested distribution.
qargo does not link a qleisliup library. Its present implementation links Qleisli
0.2.1 directly; it does not dispatch ordinary checking to a PATH compiler.
Bundling or proxying qargo does not migrate that acceptance authority. A future
external-checker protocol belongs to the upstream projects and is outside this
work. A compiler identity command such as `qli version --json` is an upstream
proposal, not an interface supplied by qleisliup.

Components, profiles, cross-target installation, persistent directory overrides,
beta/nightly channels, a registry, and automatic package resolution are excluded.
Initial distribution hosts are `aarch64-apple-darwin`, `x86_64-apple-darwin`, and
`x86_64-unknown-linux-musl`. Linux x86_64 selects the static musl distribution;
it does not select an archive from the Rust compiler's build target alone.
Windows and Linux ARM64 are deferred.

## Current executable

The binary accepts no arguments, `--help`, or `-h` to print help, and `--version`
or `-V` to print `qleisliup <Cargo package version>`. Each flag must appear alone.
Manager help/version need no home or working-directory lookup. The manager also
implements `list`, `[+selector] show`, `[+selector] which [tool]`,
`default <exact-version|stable>`, `pin <exact-version>`, `toolchain link/unlink`,
`install <exact-version|stable>`, `uninstall <exact-version>`, `[+selector] sync`,
and `self update`. The same Cargo package builds `qleisliup-init` with the shared
private core, without a public Rust library. Cargo run defaults to qleisliup.
Bootstrap accepts no arguments to install, or an isolated help/version flag.
Its executable identity is selected by the Cargo binary name, so renaming it
does not turn it into a manager/proxy.
Invoking a symlink named qli/qleisli/qargo/qlippy/qlifmt/qlidoc dispatches that
tool, including its help/version flags, through the selected toolchain.

Success is exit status 0; operational failures use 1; invalid CLI syntax and
unsupported arguments use 2. Diagnostics go to stderr. Stable selection
requires local authenticated history and an installed exact release. Only explicit
install/sync, bootstrap, and self update may refresh metadata or download; proxy
dispatch and inspection stay offline, and selected tools retain their upstream
behavior. The package uses fixed parsing/filesystem/process/TUF/archive dependencies;
the original help/version-only scaffold used the standard library alone.

## Exact identities and repository declarations

A release identity is the exact SemVer string and distribution host. SemVer
means a complete `MAJOR.MINOR.PATCH`, optionally with valid prerelease and build
metadata. Ranges, shortened versions, a `v` prefix, and surrounding whitespace
are invalid. Build metadata remains part of the identity even though SemVer
precedence ignores it. Qleisli and stdlib must have identical version strings.
qargo has an independent exact version fixed by the distribution.

The user-maintained declaration is `qleisli-toolchain.toml`:

```toml
[toolchain]
version = "0.4.0"
```

Its only supported setting is the string `[toolchain].version`; malformed TOML,
missing/non-string values, and unknown settings fail explicitly. Symlink or
special-file declarations are rejected. `stable`,
`^0.4`, `>=0.4`, and `dev` are invalid here. This file selects the language
distribution; Qargo.toml declares qrate information, including its edition.
Neither file substitutes for the other.

Published release bytes are immutable. Fixes require a new version. The manager
never repairs, overlays, or replaces an installed identity in place. Repeating
an exact install of an existing valid installation is a local no-op. An invalid
installation is an error rather than permission to overwrite it.

## Selection and process dispatch

Selection takes the first applicable source:

1. The first argument `+selector` to `show`/`which`, or to a proxy where
   it is removed before forwarding arguments.
2. A present `QLEISLIUP_TOOLCHAIN` environment variable.
3. The nearest `qleisli-toolchain.toml`, searched from the canonical current
   working directory through its parents to the filesystem root.
4. The exact global default stored in manager settings.

An explicit selector may be an exact version, `stable`, or a registered local
name. A present empty or malformed override is an error. A discovered malformed
declaration is an error. Higher-priority selections do not inspect lower-priority
declarations or settings. Missing installations and failures never trigger
fallback. `sync` accepts and ignores a syntactically valid leading selector;
a leading selector on other manager commands is invalid CLI syntax.

`stable` in inspection or a proxy resolves only through a previously authenticated
local channel observation. It never refreshes metadata or installs a toolchain. An
unknown alias asks for `qleisliup install stable`. A missing repository-selected
release asks for `qleisliup sync`; a missing explicit release asks for
`qleisliup install <version>`. A missing linked directory asks the user to repair
the link. With no selection, ask for an explicit `default` operation.

`which qli` and `which qleisli` print the same selected `bin/qleisli` path.
Process dispatch is implemented on the three supported Unix hosts.

`qli` and `qleisli` map to the selected `bin/qleisli`. The other proxies are
`qargo`, `qlippy`, `qlifmt`, and `qlidoc`, each mapped to the same-named file.
Development proxy entry points are symlinks to the manager; dispatch recognizes
the invoked name from argv[0], rather than the manager's canonical executable filename.
Stage 4 bootstrap creates these entry points in the managed bin directory.

The manager verifies the selected file exists and is executable, rejects a
recursive dispatch back to itself (including a hardlink to the same device/inode),
and invokes its absolute path with Unix `execve` through nix's safe API. It does
not search PATH or retry ENOEXEC through a shell. The remaining arguments,
including non-UTF-8 arguments, current directory,
and stdin/stdout/stderr are preserved. After exec, exit status and signal behavior
are the tool's own. The selected `bin` is prepended to PATH and
`QLEISLIUP_TOOLCHAIN` is set to the resolved exact version or linked name so
nested tool calls retain the choice. The compiler's `--version`/`--help` are
forwarded rather than interpreted as manager flags.
The resolved absolute home is also passed in `QLEISLIUP_HOME`. Existing PATH
entries, including empty entries and non-UTF-8 bytes, are preserved after the
selected bin; an unset PATH becomes that bin alone. A bin path containing `:`
cannot be represented in Unix PATH and fails before execution. Signal masks and
ignored dispositions follow execve inheritance; the selected tool's runtime
installs its own handlers. No advisory mutation lock is acquired during dispatch.

Inspection and ordinary execution are offline. Expired remote metadata does
not prevent execution of an already installed toolchain. Upstream runtime
options, including external backend overrides, remain upstream interfaces;
authentication of a bundle does not authenticate a separately supplied backend.

## Management CLI

Management success uses exit status 0; operational failures use 1; invalid CLI
syntax uses 2. Stages 1–4 implement every command below. Missing production configuration,
authenticated channel history, or selected installation fails with 1. No management
operation invokes Cargo or builds a toolchain.

| Command | Contract |
| --- | --- |
| `install <version\|stable>` | Authenticate and install one release for the current host. May access the network; does not change the default. |
| `uninstall <version>` | Remove that host's release; preserve its remembered identity and authentication metadata. Reject removal of the configured global default until another default is selected. |
| `[+selector] sync` | Read the nearest repository declaration and install its exact release. Ignore CLI/environment selection overrides; no declaration is an error. An already installed valid release needs no network. |
| `default <version\|stable>` | Select an installed exact release now. Resolve stable from local authenticated state and store its exact version; no network or implicit install. |
| `list` | List installed releases and registered links in deterministic order, offline. |
| `[+selector] show` | Inspect the active selection, its source, path, components, and authentication history, offline. |
| `[+selector] which [tool]` | Print the absolute selected executable path, offline. Default tool is qleisli; qli is its alias. Missing tools are errors. |
| `pin <version>` | Atomically create or update the declaration in the current directory using an exact version. No install, parent-directory write, or default change. Refuse a malformed existing declaration or a symlink destination. |
| `toolchain link <name> <path>` | Register a canonical local toolchain root without copying or authenticating it. |
| `toolchain unlink <name>` | Remove the registration only; do not delete its directory. |
| `self update` | Authenticate and replace only a bootstrap-owned qleisliup executable. Never change toolchain contents, pins, or the default. Reject external installations, downgrades, and changed remembered identities. |

`show` identifies the selection source as command line, environment, repository
declaration (with its path), or global default. It displays Qleisli/std/qargo
versions and qargo's linked checker version separately. A verifier is displayed
as embedded Rust or an external backend with its actual protocol. Do not invent
a protocol number for the embedded verifier. Release authentication is described
as an installation-time fact; a link is `linked / local / unauthenticated`.
Machine-readable inspection schemas are deferred; upstream tools do not depend
on the human-readable output.

`list` excludes the real `.transactions` directory, validates every discovered release and fails on malformed directories,
incomplete inventories, or inconsistent local metadata. It sorts releases by
SemVer identity and host, then links by name. Links are listed as registrations
without claiming their directory contents are valid; show/which validate the
selected linked directory. Explicit default writes ignore selection overrides
and repository declarations, require a complete installed release for this host,
and refuse to overwrite malformed existing settings. Pin does not require the
selected release to be installed.

## Local development links

Registration, unlink, inspection, and Unix execution are implemented.

A local name is a nonempty ASCII identifier beginning with a letter and then
containing letters, digits, `_`, or `-`. `stable`, `beta`, and `nightly` are
reserved. Exact release versions cannot be local names. Re-registering an
existing name fails until it is unlinked. Invalid names fail with CLI status 2.
Input paths may be relative or name a directory symlink; registration resolves
the canonical absolute root and stores it as UTF-8 JSON. Unrepresentable canonical
paths fail. Registration validates without running the compiler, and revalidates
under the home mutation lock before atomically updating links.json. Unlink
requires an existing name, removes only its record, and also works after the
build directory has moved or disappeared. Neither operation changes the default
or a repository pin.

The canonical linked root must contain executable `bin/qleisli`. qargo, its
auxiliary tools, and an internal manifest are optional. Missing requested tools
fail without borrowing them from a release or PATH. A link without an internal
manifest has unknown component versions. A supplied manifest must be valid but
cannot confer release authentication. A manifest declaring a directory stdlib
requires std/; an external verifier requires its declared executable. Local
contents may change during development.

Example local workflow (proxy symlinks must already exist):

```sh
qleisliup toolchain link dev /absolute/path/to/build/toolchain
qli +dev check /absolute/path/to/project
```

## Manager state

The default home is `$HOME/.qleisliup`. `QLEISLIUP_HOME`, when present, must be a
nonempty absolute path and replaces that home completely. This enables isolated
tests. An invalid explicit home fails rather than falling back. Read-only
operations do not create a home; pin/default mutations create it and its advisory
lock as needed. The layout below includes managed manager/proxy destinations,
authentication state, downloads, and release receipts. Development
proxy symlinks may live separately, such as in ignored build output.

```text
<home>/
  .mutation-lock
  bin/qleisliup
  bin/{qli,qleisli,qargo,qlippy,qlifmt,qlidoc} -> qleisliup
  bin/.qleisliup-managed.json    immutable bootstrap ownership marker
  bin/.qleisliup-manager-*/      private update staging, when present
  .qleisliup-manager-*/          private bootstrap staging, when present
  settings.json
  links.json
  channels.json
  identities.json
  metadata/official/
    current.json                atomic pointer to committed TUF state
    generations/state-*/        trusted root, roles, and last observed time
    work-*/                     private mutable tough datastore
  toolchains/.transactions/     private same-filesystem install/removal staging
  toolchains/<version>-<host>/
    bin/
    toolchain.json
    LICENSE
    NOTICE
    .qleisliup-receipt.json
```

Local JSON state formats each carry independent `schema: 1`:

- `settings.json` stores `default`, an exact version or null.
- `links.json` stores `links`, a name-to-canonical-path map.
- `channels.json` stores authenticated observations for `stable` and
  `qleisliup-stable`: version, target name, and target SHA-256. Each version also
  serves as the remembered channel high-water mark. Observations have
  `authenticated: true` and are nested under the top-level `channels` map.
- `identities.json` stores `releases`, keyed by `<version>-<host>`. Each value
  contains `manifest_target`, `manifest_sha256`, `artifact_target`,
  `artifact_sha256`, and `artifact_size`. Digests are 64 lowercase hexadecimal
  characters; size is positive. Release targets are exactly
  `releases/<version>/manifest.json` and `releases/<version>/<host>.tar.zst`.
  The optional `managers` map defaults to empty and uses the same identity fields,
  keyed by `<manager-version>-<host>`, with targets
  `qleisliup/<version>/manifest.json` and `qleisliup/<version>/<host>/qleisliup`.
  A manager manifest digest also binds that version across hosts. Uninstall
  preserves both release and manager observations.

Missing initial state means unset/empty state. Existing records require their
defined fields (`settings.default` is an exact version or explicit null) and
reject unknown fields, malformed contents, or unsupported schemas. Metadata
reads require regular files, reject symlinks/special files, and are bounded to
1 MiB per file. State is read only when an operation needs it; a higher-priority
selection does not fail because an unused lower-priority setting is malformed.

State updates use an exclusive temporary file in the destination directory,
sync the file, atomically replace the destination, and sync the directory.
New home/directory entries and their parent inodes are also synced before relying
on durable state stored below them. Identity/channel/link writers reject output above
the metadata size limit before replacing the prior record.
Pin writes the complete supported declaration with mode 0644; settings/links use 0600,
subject to the process umask. Pin replaces comments/formatting rather than
merging unsupported settings. A failure after rename but during directory sync
reports that replacement occurred and durability was not confirmed.

Mutating commands share a process-scoped advisory lock at `.mutation-lock`;
All mutations serialize through this lock. Pin additionally locks its destination
directory so writers using different homes serialize. The lock covers TUF state,
identity checks, downloads, and install/uninstall. Locks release
on process exit; a leftover filename is not itself a held lock. Inspection does
not create state and never treats staging data as installed.

## Distribution and internal manifests

Release metadata and repository declarations have separate purposes. The
distribution client and archive handling in this section are implemented in Stage 3;
offline inspection validates existing local manifests, receipts, and inventories.
The distribution manifest is a JSON TUF target, not a custom signed envelope.
Versioned targets are fixed under `releases/<version>/`; only channel targets
are mutable. Artifacts use tar.zst and include one `<version>-<host>/` root.
Artifacts refer to TUF target names, not arbitrary download URLs.

Illustrative release manifest:

```json
{
  "schema": 1,
  "qleisli": "0.4.0",
  "std": "0.4.0",
  "std_kind": "embedded",
  "qargo": "0.2.1",
  "qargo_checker_qleisli": "0.4.0",
  "verifier": {"kind": "embedded-rust"},
  "compiler_commit": "<full upstream commit ID>",
  "verifier_commit": "<full upstream commit ID>",
  "artifacts": {
    "aarch64-apple-darwin": {
      "target": "releases/0.4.0/aarch64-apple-darwin.tar.zst",
      "sha256": "<64 lowercase hexadecimal digits>",
      "size": 12345678
    }
  }
}
```

The example's qargo checker version is illustrative. A bundle containing the
current qargo 0.1.5 must instead declare its actual linked Qleisli 0.2.1 checker,
even if its standalone compiler has another version. All four qargo executables
share the distribution's qargo version.

The archive's `toolchain.json` carries the same schema and component identity
fields as the release manifest, plus its exact `host`, excluding `artifacts`.
Validation binds it to the requested version, selected host, release manifest,
and authenticated artifact identity. The required inventory is `bin/qleisli`,
`bin/qargo`, `bin/qlippy`, `bin/qlifmt`, `bin/qlidoc`, `toolchain.json`, `LICENSE`,
and `NOTICE`. The compiler and four qargo tools must be executable regular files.
No compiler version-reporting command is assumed by the installer.

`std_kind` is `embedded` or `directory`; a directory bundle also requires `std/`.
The same exact Qleisli/std version invariant applies to both. An external verifier
uses `{"kind":"external","protocol":<positive integer>,"path":"<relative file>"}`
and requires that executable regular file inside the bundle. Embedded Rust has
no external path or wire protocol. Commit fields describe the real upstream
implementations; they are provenance, not correctness evidence. Unsupported
schemas, component arrangements, or missing required fields reject installation.

## TUF authentication and channels

TUF is the sole distribution acceptance authority. Use the Rust `tough` client
for signature thresholds, sequential root rotation, metadata versions/expiry,
delegation, and target length/digest verification. Persist the trusted root and
rollback-detection metadata; keep expiry enforcement enabled. Integration must
validate the locked dependency graph on Rust 1.85 and the supported host matrix.
Stage 3 pins tough 0.24.0 with HTTP support. There is no production embedded root
or endpoint and no user-facing trust/endpoint override. Isolated fixture trust
exists only in the test harness, unreachable from the production executable.

tough's mutable datastore is private to a refresh. Before returning from refresh
(including rejection after authenticated advances), and after each target read,
validate its complete bounded JSON store and copy the replay state to a new
immutable generation: `root.json`, `timestamp.json`, `snapshot.json`,
`targets.json`, and `latest_known_time.json`. For pinned tough 0.24.0, delegated
role bytes are fetched and verified on every load; their version floors remain
in the persisted signed snapshot. Do not accumulate superseded numbered delegated
cache files across refreshes. Sync files/directories, then atomically switch
`current.json`. The pointer is
`{"schema":1,"generation":"state-<generated name>"}`. Restart from that generation's
root, never from an older initial root. Corrupt persisted records fail rather
than allowing tough to ignore malformed optional cached JSON. Once the pointer
is durable, old committed generations can be removed. A killed process may leave
a private work/snapshot directory; it is not an installed release or active trust.

Expiry remains `Safe`. The actual system clock and persisted last-observed time
govern expiry/clock rollback; tests use dates safely in the past/future rather
than disabling enforcement. Metadata limits are 1 MiB per file, 64 MiB per
refresh/persisted store, 2,048 transport requests, 2,048 datastore files, and
1,024 sequential root updates. Signed lengths do not bypass the transport cap.
The transport applies metadata byte caps throughout the eager repository load,
then target-specific bounds apply to target reads. Classification follows that
client phase, so metadata and target endpoints may share a URL prefix. The
request budget includes subsequent target fetches. There are no fixture
clock/environment switches in the production CLI.

The repository uses root, targets, snapshot, and timestamp roles. Target namespaces
are `releases/`, `qleisliup/`, and `channels/`, with separate delegated targets
roles. Preserve intermediate numbered roots for old clients. Consistent snapshots
and signed metadata govern target retrieval; the release manifest's artifact
digest and size must also agree with the selected TUF target.

The authenticated `channels/stable.json` target is
`{"schema":1,"qleisli":"0.4.0"}`. The independent
`channels/qleisliup-stable.json` target is
`{"schema":1,"qleisliup":"0.1.0"}`. Channel versions are ordinary release
SemVer without prerelease or build metadata. Record an authenticated channel
observation before using it, even if the subsequent install fails. Refuse a
version below the persisted high-water mark; equal versions are allowed.
Explicit exact installs of older Qleisli releases remain allowed.

TUF metadata rollback checks and semantic channel downgrade checks are distinct.
Authentication alone does not forbid the publisher from signing different bytes
under a previously used release name. The distribution therefore prohibits
republication, and the manager rejects changes to remembered release identities,
including after uninstall. A previously observed manifest digest also binds
the same version across hosts. Stable refresh checks the authenticated manifest
even when its release is already installed; exact reuse is deliberately offline.
A new home has no knowledge of earlier observations.

Sigstore provenance may support public release auditing. It is not a second
client trust root or an additional install acceptance condition.

## Installation transaction and receipts

Installation follows this sequence under the mutation lock:

1. Resolve the requested exact release, authenticating stable if requested.
2. Refresh and authenticate TUF metadata, then fetch the release manifest target.
3. Check version/std invariants, host support, known identities, and target binding.
4. Download the artifact to private temporary storage. Consume the verification
   stream to successful completion and check exact digest/size before unpacking.
5. Extract into a private staging directory on the toolchains filesystem.
6. Validate the internal manifest and complete required inventory; add the receipt.
7. Atomically persist the known identity, then rename the complete staging directory
   to the absent final path. Sync files and containing directories for durability.

Never unpack while the download stream is still awaiting final verification.
Reject absolute paths, `..`, entries outside the single root, duplicate paths,
symlinks, hardlinks, devices, FIFOs, and other special files. Apply bounded
download/extraction limits, reject privileged permission bits, and do not restore
archive ownership. The payload may not supply an installer-owned receipt.
Before accepting the extracted tree, exclusively create and remove the reserved
receipt destination to reject file or directory aliases using the extraction
filesystem's actual naming rules.
No install hooks or archive-provided programs run during validation. Downloads
are limited to 1 MiB for channel/manifest and 512 MiB for artifacts. Extraction
allows at most 2 GiB of decoded tar data, 512 MiB per file, 100,000 tar entries
and filesystem nodes (including implicit directories), a 128 MiB zstd window,
64 path components, and 4,096 UTF-8 path bytes. Paths must be canonical without
empty/dot/parent components or backslashes. Reject distinct directory spellings
that alias an existing directory on the extraction filesystem, including case
folding and Unicode normalization. GNU/PAX extension records are rejected;
packagers must use ordinary file/directory headers. Directory permissions are
normalized to 0755; file modes may contain only ordinary 0777 permission bits,
subject to platform behavior. Drain the decoder after tar EOF under the same
budget and reject truncation or nonzero trailing/concatenated payloads.

Failures remove this transaction's staging data where possible, preserve existing
releases/defaults, and return failure. Process termination may leave private
temporary data; later mutations may clean abandoned transactions under the lock.
A crash after identity persistence but before rename leaves a remembered identity
with no installation, which is safe to retry. Only complete final directories
with valid receipts count as installed. A final-name collision is validated as
an existing installation or rejected, never overwritten.

The installer-written `.qleisliup-receipt.json` has independent `schema: 1`
and fields `qleisli`, `std`, `std_kind`, `qargo`, `qargo_checker_qleisli`, `host`,
`verifier`, `manifest_target`, `manifest_sha256`, `artifact_target`,
`artifact_sha256`, `artifact_size`, and `authenticated: true`. It accompanies the
release before publication. Offline inspection requires this record, checks component
agreement with the internal manifest and exact identity agreement with the
remembered ledger, and validates the required inventory. It does not download,
rehash installed bytes, or revalidate TUF signatures. Tests use isolated local
fixtures to exercise record consistency; separate internal tests authenticate
isolated signed repositories and install their test bundles.

Uninstall first validates the installation and rejects the current global default,
then exclusively renames the release into private removal staging, syncs the
toolchains directory, and deletes the staging data. Keep identities, channel/TUF
history, repository pins, and settings. Corrupt installations fail validation;
uninstall does not silently repair or force-remove them. Cleanup/durability failure
after removal reports the actual completed removal and remaining limitation.

Receipt authentication is a statement about the completed install transaction.
It is not proof that local files remain unchanged, that a runtime backend override
is trusted, or that a program is correct. The manager never modifies published
toolchain contents, but cannot enforce immutability against the filesystem owner.

## Bootstrap and self-update

`qleisliup-init` is a native bootstrap executable using the same persistent TUF
client and trust repository as the manager. It detects the supported host and
resolves the independent authenticated manager stable channel. The production
source remains unconfigured; there is no embedded fixture root or endpoint
override. Bootstrap installs the manager and six relative symlinks to `qleisliup`.
It installs no Qleisli toolchain and modifies no shell startup file. PATH setup
is an explicit printed instruction to the user.

The manager manifest is the immutable TUF target
`qleisliup/<version>/manifest.json`, with closed schema 1:

```json
{
  "schema": 1,
  "qleisliup": "0.1.0",
  "artifacts": {
    "aarch64-apple-darwin": {
      "target": "qleisliup/0.1.0/aarch64-apple-darwin/qleisliup",
      "sha256": "<64 lowercase hexadecimal digits>",
      "size": 12345678
    }
  }
}
```

Only initial supported hosts are accepted. The channel's ordinary exact version,
manifest version, host, immutable target path, SHA-256, and signed target size
must agree. Manager artifacts are raw native executables bounded to 128 MiB;
channel and manifest targets are bounded to 1 MiB. They are independent of
Qleisli/std/qargo versions and toolchain archives.

After complete TUF verification, give the staged executable ordinary 0755
permissions, reject text/scripts, and run only its `--version` probe with closed
stdin, a 10-second timeout, and a 1 KiB limit for each captured stream. Require
successful exit, exactly `qleisliup <version>\n` on stdout, and empty stderr.
Run the probe in a separate Unix process group and terminate that group on every
completion path, including success. Keep the leader waitable until signalling
the group, then reap it before validating captured output. On macOS, an EPERM
result for an exited group is accepted only when reaping the leader and a
non-destructive existence check confirm that the group has disappeared. Failure
to terminate a live group rejects the candidate. This cleans up ordinary inherited
helpers; it is not a sandbox against authenticated code that deliberately changes
its process group or session.
The kernel checks actual executability; full CPU/linkage compatibility remains
a production host gate. Rehash after the probe to reject a changed staged file.
This executes authenticated manager code, never unverified download bytes or
archive-provided installation hooks.

For bootstrap, require an absent `<home>/bin`, including refusing an existing
empty directory, symlink, or unrelated command layout. Stage the complete bin
directory on the home filesystem with the manager, six symlinks, and
`.qleisliup-managed.json`: `{"schema":1,"host":"<host>","authenticated":true}`.
Sync the contents and persist the manager identity before an exclusive atomic
rename of the directory. Sync the home directory afterward. A rejected transaction
never publishes a partial bin. Existing home state, toolchains, pins, and defaults
are preserved. Both lifecycle operations serialize under the home mutation lock.

Initial bootstrap authenticity requires an out-of-band distribution/verification
method. TUF begins with that trusted executable/root; it does not solve its own
bootstrap authenticity. Test keys and test repositories remain isolated from
production. Production URLs, signing keys, thresholds, expiration schedules,
root ceremony, and publication operations are not configured by local Stages 1–4.

`self update` consults the independent authenticated manager channel and target,
checks host/identity and rejects a downgrade, verifies a staged executable, and
atomically replaces only a bootstrap-owned `<home>/bin/qleisliup`. Require the
running executable's canonical path to be that managed path, the real bin
directory, a valid ownership marker for the current host, and all six expected
relative proxy links. Canonicalize both compared paths so invocation through a
symlink or filesystem case alias of the same executable remains valid. The
managed destination itself must still be a regular file, not a symlink.
Require an executable regular file with one hardlink, no
privileged permission bits, and digest/size matching the remembered identity for
the running Cargo package version. Revalidate under the lock and before publication;
a process still running the old version after another update must rerun the
installed executable. The current process may finish on its old executable
image. Symlinks continue to reference that path. Stage inside bin so replacement
remains on that filesystem,
even if bin and home have different mount points. Persist the verified new
identity before replacing the executable; retain prior identities and the
immutable ownership marker. No mutable version pointer or separate current
receipt must be updated after rename. A same-version update still authenticates
the manifest and checks the remembered identity, then reuses the executable.
Refuse channel rollback below the observed manager high-water mark and candidate
versions below the running version.

Before replacement, failures leave the old manager usable. Errors after rename
report completed publication and any unconfirmed durability. A killed process
may leave private staging; it is not an installed manager and does not change
the active path. These ownership records describe past authentication and rely
on the local filesystem owner; they are not a boundary against owner tampering.
An externally installed manager, including a package-manager/Cargo installation, refuses
self-replacement and refers the user to that installation method. No toolchain
install command updates the manager implicitly.

## References

- [The Update Framework specification](https://theupdateframework.github.io/specification/latest/)
  defines metadata roles, root rotation, expiry, and verified target retrieval.
- [tough RepositoryLoader](https://docs.rs/tough/0.24.0/tough/struct.RepositoryLoader.html)
  documents the persistent datastore and expiration enforcement settings.
- [tough Repository](https://docs.rs/tough/0.24.0/tough/struct.Repository.html)
  documents that target streams must finish verification before their bytes are used.
- [Qargo toolchain design](https://github.com/MGYamada/qargo/blob/v0.1.5/docs/toolchains.md)
  distinguishes its linked checker from a future external toolchain protocol.
