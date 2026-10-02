# Stage 3 local review, 2026-10-02

The source and fixture review found four P2 defects. Each was reproduced before
the fix and covered by a regression test. All four have been corrected.

| Finding | Trigger and previous result | Correction and regression |
| --- | --- | --- |
| Superseded delegated metadata accumulates | Successive consistent-snapshot refreshes copied every numbered delegated cache file into the next committed generation. The bounded datastore would eventually reject otherwise valid refreshes. | Validate the complete private store, then retain the five core replay records. Repeated refreshes remain bounded; a newer signed snapshot referring to an older delegated role is still rejected. |
| Metadata URL prefix applies the wrong artifact bound | A target endpoint beneath the metadata endpoint made a valid artifact larger than 1 MiB fail the metadata transport cap. | Apply metadata caps during tough's eager load and target bounds during target reads. A signed artifact larger than 1 MiB installs through a nested target endpoint. |
| Archive directory aliases merge | Distinct case or Unicode spellings could map to the same directory on the extraction filesystem and bypass textual duplicate detection. | Create each newly recorded directory exclusively. Filesystem aliases are rejected; distinct names remain valid on filesystems that distinguish them. The regression probes both case folding and Unicode normalization. |
| A large link registry becomes unreadable | Registration could report success after writing a registry larger than the 1 MiB reader limit. Subsequent link inspection then failed. | Use the bounded state writer. An oversized registration fails before replacement, preserves the previous bytes, and leaves existing links inspectable. |

The metadata compaction and transport boundary rely on the pinned tough 0.24.0
implementation: metadata loading is eager, delegated bytes are fetched on every
load, and the persisted signed snapshot retains delegated version floors. Check
these assumptions when upgrading tough. Its public
[datastore contract](https://docs.rs/tough/0.24.0/tough/struct.RepositoryLoader.html)
requires persistent rollback state and enabled expiry checks; the
[TUF specification](https://theupdateframework.github.io/specification/latest/)
remains the distribution authentication contract.

## Validation and limits

Final validation is recorded in the
[implementation plan](implementation-plan.md#stage-3-review-validation-2026-10-02).
The review focused on persisted trust, archive extraction, transactional failure,
and bounded local state, using source inspection and isolated fixture repositories
on macOS ARM64. It does not establish live HTTPS behavior, real upstream bundle
compatibility, Linux or macOS x86_64 execution, storage power-loss behavior, or an
independent security audit. Production trust and endpoints remain unconfigured.
