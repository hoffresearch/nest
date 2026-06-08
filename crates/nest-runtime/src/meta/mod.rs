//! Generic metadata inverted index (section `0x17`): `(field, value) ->`
//! sorted chunk ordinals.
//!
//! Build time, a market hands one label column per field (`value` per chunk,
//! optional); query time, [`MmapNestFile::search_filtered`] scopes the exact
//! cosine to a single `(field, value)` posting list. The format carries NO
//! market-specific rule — `patient_file_id`, `tenant`, `lang`, `doc_class` are
//! all just field-name strings the caller chose. The section is optional and
//! EXCLUDED from content_hash, so attaching it never invalidates a `nest://`
//! citation. Encoding/decoding lives in [`super::meta::codec`].

mod codec;

use std::collections::HashMap;

use nest_format::error::NestError;
use nest_format::layout::SECTION_META_INDEX;
use nest_format::reader::NestView;

use crate::error::RuntimeError;
use crate::mmap_file::MmapNestFile;

/// lOpen the optional meta_index (0x17) from a parsed view, by section presence
/// (like bm25/hnsw — no manifest flag), so a file carrying the index has the
/// SAME content_hash as one without it. `None` when the section is absent.
pub(crate) fn open(view: &NestView) -> Result<Option<MetaIndex>, RuntimeError> {
    if view
        .section_table
        .iter()
        .any(|e| e.section_id == SECTION_META_INDEX)
    {
        let bytes = view.decoded_section(SECTION_META_INDEX)?;
        let mi = MetaIndex::from_bytes(&bytes)?;
        // lreject a stale/hostile index (ordinals built against a different
        // corpus) at open with a typed error, so the query path can never index
        // the embedding slab out of bounds (the rerank slices rows by ordinal
        // with no bounds check).
        mi.validate_ordinals(view.header.n_embeddings as usize)?;
        Ok(Some(mi))
    } else {
        Ok(None)
    }
}

/// on-disk payload version for the meta_index section (`0x17`). bumped only if
/// the wire layout changes; the section is optional + content_hash-excluded, so
/// a bump stays additive within frozen v1.
pub const META_INDEX_PAYLOAD_VERSION: u32 = 1;

/// in-memory metadata inverted index. `fields[field][value]` is the ascending,
/// deduplicated list of chunk ordinals carrying that value. `HashMap` for O(1)
/// query lookup; the on-disk form sorts fields and values so two builds are
/// byte-identical.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetaIndex {
    pub(super) fields: HashMap<String, HashMap<String, Vec<u32>>>,
}

impl MetaIndex {
    /// lBuild from per-field value columns. each column is one `(field_name,
    /// values)` pair where `values[i]` is the label of chunk `i` (`None` or an
    /// empty string means the chunk has no value for this field and is simply
    /// absent from every posting list of that field). pure and
    /// order-independent: the encoder sorts fields, values, and postings, so
    /// two builds from the same columns are byte-identical. Chunk ordinals are
    /// stored as `u32`, so the corpus must have fewer than 2^32 chunks.
    pub fn build(columns: &[(String, Vec<Option<String>>)]) -> Self {
        let mut fields: HashMap<String, HashMap<String, Vec<u32>>> = HashMap::new();
        for (name, values) in columns {
            let entry = fields.entry(name.clone()).or_default();
            for (i, v) in values.iter().enumerate() {
                if let Some(val) = v {
                    if !val.is_empty() {
                        entry.entry(val.clone()).or_default().push(i as u32);
                    }
                }
            }
        }
        // lNormalize every posting list to ascending + deduped, so lookup,
        // equality, and the gap-encoded wire form are all deterministic
        // regardless of the column iteration order the caller passed.
        for vmap in fields.values_mut() {
            for posting in vmap.values_mut() {
                posting.sort_unstable();
                posting.dedup();
            }
        }
        Self { fields }
    }

    /// lThe ascending chunk ordinals carrying `value` for `field`, or `None`
    /// when the `(field, value)` pair is absent.
    pub fn posting(&self, field: &str, value: &str) -> Option<&[u32]> {
        self.fields
            .get(field)
            .and_then(|m| m.get(value))
            .map(|v| v.as_slice())
    }

    /// lNumber of distinct fields the index carries.
    pub fn n_fields(&self) -> usize {
        self.fields.len()
    }

    /// lDistinct field names, sorted (stable for inspect / display).
    pub fn field_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.fields.keys().cloned().collect();
        v.sort();
        v
    }

    /// lNumber of distinct values held for `field` (0 when the field is absent).
    pub fn n_values(&self, field: &str) -> usize {
        self.fields.get(field).map(|m| m.len()).unwrap_or(0)
    }

    /// lReject any posting ordinal `>= n` (a stale or hostile index built
    /// against a different corpus). Called at open so a malformed 0x17 fails
    /// with a typed error instead of panicking later in the query path, where
    /// the rerank slices the embedding slab by ordinal without a bounds check.
    pub(crate) fn validate_ordinals(&self, n: usize) -> Result<(), RuntimeError> {
        for vmap in self.fields.values() {
            for postings in vmap.values() {
                if let Some(max) = postings.iter().copied().max() {
                    if max as usize >= n {
                        return Err(RuntimeError::Format(NestError::MalformedSectionPayload {
                            section_id: SECTION_META_INDEX,
                            reason: format!("meta_index ordinal {} >= n_chunks {}", max, n),
                        }));
                    }
                }
            }
        }
        Ok(())
    }
}

/// meta_index accessors on the open file. grouped with the feature (the field
/// itself lives on `MmapNestFile`) so `mmap_file.rs` stays under the 300-line
/// guard.
impl MmapNestFile {
    /// lWhether this file carries a meta_index (0x17) section.
    pub fn has_meta_index(&self) -> bool {
        self.meta_index.is_some()
    }
    /// lBorrow the metadata inverted index, if present. `search_filtered` uses
    /// it to resolve a `(field, value)` into the chunk subset to score.
    pub(crate) fn meta_index(&self) -> Option<&MetaIndex> {
        self.meta_index.as_ref()
    }
    /// lThe distinct field names the meta_index carries (sorted), or empty when
    /// the file has none — lets a caller discover what is filterable.
    pub fn meta_index_fields(&self) -> Vec<String> {
        self.meta_index
            .as_ref()
            .map(|m| m.field_names())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::MetaIndex;

    #[test]
    fn validate_ordinals_rejects_out_of_range() {
        // lpostings are ordinals 0 and 1 (two chunks labelled "a").
        let mi = MetaIndex::build(&[("p".into(), vec![Some("a".into()), Some("a".into())])]);
        assert!(mi.validate_ordinals(2).is_ok()); // 0,1 < 2
        assert!(mi.validate_ordinals(1).is_err()); // 1 >= 1 -> typed error, no panic
    }
}
