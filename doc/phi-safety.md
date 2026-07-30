# PHI safety — threat model and controls

This repo is developed against a private clinical dataset (psychotherapy session
notes). **No PHI may ever be committed or pushed.** The remote is
`github.com/hoffresearch/nest`. This document records the threat model and the
defense-in-depth controls, so the controls are auditable and re-installable.

## Threat model

The development environment performs **environment-level auto-commit/push** of the
working tree (it is not a repo hook — the only `.git/hooks` present are benign
git-lfs delegators). The primary risk is: a PHI artifact lands inside the repo
working tree and an auto-committer stages + pushes it to GitHub before a human
notices.

A second vector is **scratch / measurement artifacts**: tools that read PHI
(cohort builders, the relevance ruler, dedup audits) can write cleartext
intermediates — sidecars, per-pair note files, temp `.nest` — to `/tmp` or the
work dir. `/tmp` is world-traversable (mode `1777`) and cleaned only by a periodic
job (files older than ~3 days), so PHI written there is readable by any local user
until then. Separately, these controls protect the *git repo*, not the *disk*:
`/Volumes/<redacted>` is an **unencrypted** external APFS volume, so physical loss/theft
or remounting it on another machine bypasses Unix permissions entirely.

## Controls (defense in depth)

**1. Physical separation — the root guarantee.**
Clinical data lives at `/Volumes/<redacted>/dat/psy/work`, which is **not under any git
repository** (no `.git` in any parent up to the `/Volumes` mount). The repo is at
`/Volumes/<redacted>/dev/nest`, a sibling tree. An auto-committer operating on the nest
repo therefore *cannot* see, stage, or push the clinical files at all. All build
tools write their `.nest` / sidecar outputs to `dat/psy/work`, outside the repo.

Verify:
```sh
git -C /Volumes/<redacted>/dat/psy/work rev-parse --show-toplevel
# expected: fatal: not a git repository (or any parent up to mount point /Volumes)
```

**2. `.gitignore` catch-all — survives `--no-verify`.**
`.gitignore` ignores **all** `*.nest`, `*.sidecar.jsonl`, and `**/pairs/` by
default, negating only the three sanctioned PUBLIC, non-PHI corpora that are
tracked (`dat/corpus_next.v1.nest`, `dat/measure/fakerecogna_exact.nest`,
`crates/nest-format/tests/fixtures/golden_v1_minimal.nest`). A PHI export with
*any* filename is ignored, so even `git add -A` from a `git commit --no-verify`
(which still honors `.gitignore`) cannot stage it. This is the layer that holds
when hooks are bypassed.

**3. Active `pre-commit` gate — catches force-adds.**
`scripts/pre-commit` (version-tracked) aborts any commit that stages a `.nest` /
`.sidecar.jsonl` / `pairs/` artifact not on its explicit allow-list — catching a
`git add -f` that bypasses `.gitignore`. It coexists with the git-lfs hooks (LFS
uses post-commit/pre-push, not pre-commit). It is **not** wired via
`core.hooksPath` (that would disable the LFS hooks).

Install on a fresh clone (`.git/hooks` is not version-tracked):
```sh
cp scripts/pre-commit .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
```

**4. Scratch hygiene — PHI never in `/tmp`, work dir locked down.**
All PHI intermediates go under `/Volumes/<redacted>/dat/psy/work` (mode `700`), **never
`/tmp`** (world-traversable). PHI files there are mode `600`. Measurement
artifacts (per-pair note files, sidecars, cohort `.nest`) are EPHEMERAL: deleted
(`trash`) immediately after the analysis that needs them; only non-PHI derived
results (counts, AUC, the numbers committed to the plan) persist. A tool that
reads note text writes its output to the work dir and cleans up on completion.

## Verifying the controls

```sh
# layer 2: an arbitrarily-named PHI .nest is ignored
touch dat/measure/zzz_fake_phi.nest
git check-ignore -v dat/measure/zzz_fake_phi.nest   # -> .gitignore:*.nest  (ignored)
git status --short | grep zzz                        # -> (no output: invisible to add -A)

# layer 3: pre-commit blocks even a forced stage
git add -f dat/measure/zzz_fake_phi.nest
.git/hooks/pre-commit                                # -> BLOCKED, exit 1
git reset -q dat/measure/zzz_fake_phi.nest && trash dat/measure/zzz_fake_phi.nest

# the three public corpora remain trackable (negations win)
git check-ignore -q dat/corpus_next.v1.nest && echo IGNORED-BAD || echo ok-trackable
```

## Residual risks (not closed by these controls)

These controls protect the **git repository**. Two PHI vectors remain — owned by
the operator, not the code:

- **Disk encryption (open).** `/Volumes/<redacted>` is an unencrypted external USB APFS
  volume (`diskutil info /Volumes/<redacted>` → `Encrypted: No`). `chmod 600` stops other
  users on the *running* system, not physical theft or remounting the disk
  elsewhere as admin. **Mitigation: enable APFS / FileVault encryption on the
  volume.** This is the one link `chmod` cannot close.
- **Backup / sync (verified clear, re-check before enabling any).** At audit time:
  **no Time Machine destination** (`tmutil destinationinfo` → none) and **no
  third-party cloud-sync daemon** (Dropbox/Drive/OneDrive) over the volume; the
  system iCloud `bird` daemon does not sync arbitrary external volumes. A backup
  tool, once enabled, copies PHI regardless of file mode — exclude `/Volumes/<redacted>`
  if one is ever turned on.

## Audit (as of this writing)

- PHI directory is outside any git repo: confirmed.
- Tracked `.nest` files: only the three public corpora above. No PHI tracked.
- Working tree: clean.
- All three repo controls verified end-to-end.
- Work dir `dat/psy/work` is mode `700`, PHI files `600`; `/tmp` cleared of PHI.
- Volume is unencrypted (residual risk above); no Time Machine, no cloud sync.
