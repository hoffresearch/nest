# data governance, provenance & licensing

nest ships a `.nest` as an **immutable, self-contained, content-addressed**
file that is meant to be copied and distributed (phones, edge nodes, air-gapped
boxes). That design has direct governance consequences whenever the embedded
content is personal data. This document records the posture so it is auditable.

## 1. a shipped `.nest` is a datastore, not a cache

A `.nest` stores the canonical chunk text and `source_uri` in cleartext, plus
the embeddings (a derived representation of that text). There is no at-rest
encryption in the format; `zstd` is compression, not confidentiality.

**Consequence:** a `.nest` built over personal or sensitive data (e.g. a
clinical corpus) IS a copy of that data and must be handled with the same
controls as the source, including
**encryption at rest** (FileVault / APFS / LUKS on any volume that holds it).
For sensitive-category data this is a required control, not a "residual risk".

## 2. right-to-erasure and rectification (LGPD art. 18 / GDPR art. 17, 16)

The container is immutable and content-addressed: a citation is
`nest://<content_hash>/<chunk_id>`, and changing any chunk changes
`content_hash`, invalidating every issued citation and every distributed copy.
The runtime never opens a socket, so there is **no callback channel** to recall
copies already shipped.

**You cannot honor an erasure/rectification request by editing a distributed
`.nest` in place.** Before shipping any `.nest` that contains personal data:

1. **Establish a lawful basis and run a DPIA** (GDPR art. 35 / LGPD art. 38).
   Prefer to ship only **anonymized or CC0/permissively-licensed** corpora; the
   `python/forge/demo_corpus` CC0 path exists for exactly this.
2. **Define a revocation/rotation process** up front, since in-place deletion is
   impossible: version the corpus (each build has a distinct `content_hash` /
   `file_hash`), publish a revocation list, and place a contractual/operational
   obligation on operators to re-pull the current build and destroy superseded
   copies. Treat embeddings as in-scope derived personal data.
3. **Record consent/provenance** for embedded third-party content.

Do not distribute a `.nest` containing special-category data (health, etc.)
without counsel confirming the immutable-distribution model is compatible with
the applicable data-subject rights.

## 3. provenance & build integrity (as compliance assets)

These properties make a `.nest` a strong evidentiary artifact:

- **reproducible builds**; `reproducible=True` + same chunks + same model
  fingerprint ⇒ byte-identical `file_hash` on any machine.
- **four SHA-256 hashes**; header, per-section (physical bytes), whole-file, and
  `content_hash` (decoded bytes, stable across encodings).
- **model fingerprint**; `model_hash` over config + tokenizer + weights +
  pooling + dim + normalize; a query embedded by a different model fails the
  honesty gate (CLI, and the Python `retrieve` binding via `expected_model_hash`).

**Known gaps (tracked hardening items):**

- Release artifacts carry per-file sha256 checksums and **sigstore keyless
  attestations** (`gh attestation verify`), so an artifact can be attested
  against the repository. Release **tags** are not yet signed and **no SBOM**
  is published per release; both remain tracked items. `Cargo.lock` is
  committed so the Rust dependency set is pinned; Python deps are declared in
  `pyproject.toml`.
- Declarative builds add two audit artifacts: the versioned build manifest
  (`manifest_schema_version`, canonical serialization, provenance redaction
  modes minimal/standard/full) and `build.lock.json` (package versions, tool
  binary hashes, model hashes, the materialized spec). A byte-identical
  rebuild claim is only valid under a matching lock.
- The on-disk checksums are unkeyed SHA-256 (integrity, not authenticity); see
  [`SECURITY.md`](SECURITY.md).

## 4. corpus licensing

The `.nest` you distribute inherits the license of the content it embeds. The
demo corpus that ships with nest is a **union of several upstream datasets with
different licenses** (some CC-BY-SA, some unspecified academic distributions).
The per-source bill of materials and the effective redistribution obligations
are in [`dat/demo/Instructions.md`](../dat/demo/Instructions.md#corpus-license-bill-of-materials).
The repository code is MIT; that does **not** cover the corpus content. If you
redistribute a built `.nest`, honor the most-restrictive upstream license and
carry attribution. For anything you ship broadly, prefer a permissive/CC0 corpus.
