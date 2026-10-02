# Stage 4 local review corrections, 2026-10-03

The local review reproduced two P2 manager defects and one P3 archive receipt
contract defect. Regression tests failed against the previous implementation
before applying the corrections below.

| Finding | Trigger and previous result | Correction and regression |
| --- | --- | --- |
| Managed invocation aliases fail ownership checks (P2) | On macOS, invoking the owned manager through a symlink or case alias could retain that spelling in current_exe. Comparing it with only the managed destination's canonical path rejected a legitimate update. | Canonicalize both paths. Signed update tests cover file/directory symlinks and filesystem case aliases; public CLI tests reach the unconfigured production-source boundary through the same aliases. Existing external, hardlink, modified-file, symlink-destination, and stale-version rejection tests remain. |
| Version probes leave helpers running (P2) | Killing only the direct child after a timeout left its helper running; a helper also survived a successful probe. | Start a separate process group, signal it before reaping the leader, and clean it up on every result. Native fixtures confirm that a helper started before timeout, excess output, abnormal exit, wrong version, or successful exit cannot write a marker after the probe returns. |
| Archive aliases bypass the receipt reservation (P3) | A signed bundle containing .QLEISLIUP-RECEIPT.JSON installed on a case-insensitive filesystem. The installer overwrote that supplied file; receipt forgery was not demonstrated, but the payload rejection contract was violated. | After extraction, exclusively create and remove the actual reserved destination. Signed fixtures supplying a file or directory alias are rejected before identity persistence or publication. Distinct names remain valid on case-sensitive filesystems. |

The process probe uses the existing pinned rustix 1.1.3 safe process APIs and
the standard library's process_group method; no unsafe code or new dependency
version is introduced. NOWAIT keeps the exited leader's PID reserved until the
group is signalled. Darwin's [group-signal implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_sig.c)
excludes zombies and can return EPERM for a group with only an exited leader.
Accept that case only after reaping and confirming group absence with a
non-destructive existence check; a live or inaccessible group still fails.
Process groups clean up ordinary helpers that inherit the group. They do not
contain authenticated code deliberately creating another group or session.

## Validation and limits

Final compiler/check results are recorded in the
[implementation plan](implementation-plan.md#stage-4-review-validation-2026-10-03).
The review and reproductions ran on macOS ARM64. Filesystem-sensitive regressions
probe the actual naming rules; Linux and macOS x86_64 execution remain CI cases,
not locally performed checks. Live production distribution, real upstream
bundles, power-loss behavior, and an independent security audit remain separate
release gates. Production endpoints and initial trust remain unconfigured.
