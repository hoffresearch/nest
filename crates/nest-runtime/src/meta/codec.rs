//! On-disk encoding/decoding for the meta_index section (`0x17`), payload
//! version 1. Fields are sorted by name and values by string for
//! byte-identical builds; every posting list (ascending chunk ordinals) is
//! delta-gapped and all gaps go into ONE shared `intpack` column, the same
//! shape the bm25 codec uses for its postings. The section is optional and
//! excluded from content_hash.
//!
//! ```text
//!   u32 LE  payload_version = 1
//!   u32 LE  n_fields
//!   for field in fields (sorted by name):
//!       u32 LE  name_len; bytes name (UTF-8)
//!       u32 LE  n_values
//!       for value in values (sorted by string):
//!           u32 LE  value_len; bytes value (UTF-8)
//!           u32 LE  count            (postings for this field/value)
//!   u32 LE  blob_len; bytes intpack(all posting gaps, concatenated in
//!                                   field-sorted, value-sorted order, each
//!                                   posting list delta-gapped from 0)
//! ```

use std::collections::HashMap;

use nest_format::encoding::{pack_u64s, unpack_u64s};
use nest_format::error::NestError;
use nest_format::layout::SECTION_META_INDEX;

use super::{META_INDEX_PAYLOAD_VERSION, MetaIndex};
use crate::error::RuntimeError;

impl MetaIndex {
    /// lEncode for storage in section `0x17` (v1). Deterministic: fields sorted
    /// by name, values sorted by string, postings ascending + delta-gapped.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&META_INDEX_PAYLOAD_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.fields.len() as u32).to_le_bytes());

        let mut field_names: Vec<&String> = self.fields.keys().collect();
        field_names.sort();
        let mut gaps: Vec<u64> = Vec::new();
        for fname in field_names {
            let vmap = &self.fields[fname];
            write_str(&mut out, fname);
            out.extend_from_slice(&(vmap.len() as u32).to_le_bytes());
            let mut values: Vec<&String> = vmap.keys().collect();
            values.sort();
            for val in values {
                let postings = &vmap[val];
                write_str(&mut out, val);
                out.extend_from_slice(&(postings.len() as u32).to_le_bytes());
                let mut prev = 0u32;
                for &p in postings {
                    gaps.push(p.wrapping_sub(prev) as u64);
                    prev = p;
                }
            }
        }
        write_blob(&mut out, &pack_u64s(&gaps));
        out
    }

    /// lDecode a section `0x17` payload. Rejects an unknown version and any
    /// posting-column length mismatch with a typed error (never a panic).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RuntimeError> {
        let mut cur = ByteCursor::new(bytes);
        let version = cur.u32()?;
        if version != META_INDEX_PAYLOAD_VERSION {
            return Err(RuntimeError::Format(NestError::UnsupportedSectionVersion {
                section_id: SECTION_META_INDEX,
                version,
            }));
        }
        let n_fields = cur.u32()? as usize;
        // lreject a count that cannot fit: a huge claim makes with_capacity ABORT
        // (uncatchable). each field is >= 8 wire bytes (intpack-cap discipline).
        if n_fields > cur.remaining() / 8 {
            return Err(malformed("meta_index: n_fields exceeds payload"));
        }
        // lRead the dictionary first (field -> [(value, count)]), accumulating
        // the total posting count so the single gap column can be sliced back.
        let mut dict: Vec<(String, Vec<(String, u32)>)> = Vec::with_capacity(n_fields);
        let mut total: usize = 0;
        for _ in 0..n_fields {
            let fname = cur.string()?;
            let n_values = cur.u32()? as usize; // same anti-abort guard, per value
            if n_values > cur.remaining() / 8 {
                return Err(malformed("meta_index: n_values exceeds payload"));
            }
            let mut vals: Vec<(String, u32)> = Vec::with_capacity(n_values);
            for _ in 0..n_values {
                let val = cur.string()?;
                let count = cur.u32()?;
                total += count as usize;
                vals.push((val, count));
            }
            dict.push((fname, vals));
        }
        let gaps = cur.intpack_column()?;
        if gaps.len() != total {
            return Err(malformed("meta_index: posting column mismatch"));
        }
        let mut fields: HashMap<String, HashMap<String, Vec<u32>>> =
            HashMap::with_capacity(n_fields);
        let mut k = 0usize;
        for (fname, vals) in dict {
            let mut vmap: HashMap<String, Vec<u32>> = HashMap::with_capacity(vals.len());
            for (val, count) in vals {
                let mut postings = Vec::with_capacity(count as usize);
                let mut prev = 0u32;
                for j in 0..count {
                    let p = prev.wrapping_add(gaps[k] as u32);
                    // lreject a zero gap (dup) or wrap (non-monotonic): a corrupt
                    // payload would otherwise yield a duplicate citation.
                    if j > 0 && p <= prev {
                        return Err(malformed("meta_index: non-ascending or duplicate posting"));
                    }
                    prev = p;
                    postings.push(p);
                    k += 1;
                }
                vmap.insert(val, postings);
            }
            fields.insert(fname, vmap);
        }
        Ok(Self { fields })
    }
}

fn malformed(reason: impl Into<String>) -> RuntimeError {
    RuntimeError::Format(NestError::MalformedSectionPayload {
        section_id: SECTION_META_INDEX,
        reason: reason.into(),
    })
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    // lfail loud on a >4 GiB name rather than silently truncate to u32.
    let len = u32::try_from(s.len()).expect("meta_index string exceeds 2^32 bytes");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn write_blob(out: &mut Vec<u8>, blob: &[u8]) {
    let len = u32::try_from(blob.len()).expect("meta_index gap blob exceeds 2^32 bytes");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(blob);
}

struct ByteCursor<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> ByteCursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    /// lUnread bytes left; `pos <= buf.len()` always, so it never underflows.
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn u32(&mut self) -> Result<u32, RuntimeError> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], RuntimeError> {
        // lpos <= buf.len() so this never underflows (and can't wrap like pos+n).
        if n > self.buf.len() - self.pos {
            return Err(malformed(format!(
                "meta_index: unexpected EOF (need {})",
                n
            )));
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
    fn string(&mut self) -> Result<String, RuntimeError> {
        let len = self.u32()? as usize;
        let b = self.bytes(len)?;
        std::str::from_utf8(b)
            .map(|s| s.to_string())
            .map_err(|e| malformed(format!("meta_index utf-8: {}", e)))
    }
    fn intpack_column(&mut self) -> Result<Vec<u64>, RuntimeError> {
        let len = self.u32()? as usize;
        let blob = self.bytes(len)?;
        unpack_u64s(blob).map_err(RuntimeError::Format)
    }
}

#[cfg(test)]
mod tests {
    use super::super::MetaIndex;

    fn sample() -> MetaIndex {
        MetaIndex::build(&[
            (
                "patient".to_string(),
                vec![
                    Some("A".into()),
                    Some("B".into()),
                    Some("A".into()),
                    None,
                    Some("A".into()),
                ],
            ),
            (
                "lang".to_string(),
                vec![
                    Some("pt".into()),
                    Some("pt".into()),
                    Some("en".into()),
                    Some("pt".into()),
                    Some("".into()), // empty == absent
                ],
            ),
        ])
    }

    #[test]
    fn roundtrip_preserves_postings() {
        let mi = sample();
        let back = MetaIndex::from_bytes(&mi.to_bytes()).unwrap();
        assert_eq!(mi, back);
        assert_eq!(back.posting("patient", "A"), Some(&[0u32, 2, 4][..]));
        assert_eq!(back.posting("patient", "B"), Some(&[1u32][..]));
        assert_eq!(back.posting("lang", "pt"), Some(&[0u32, 1, 3][..]));
        assert_eq!(back.posting("lang", "en"), Some(&[2u32][..]));
        assert_eq!(back.posting("patient", "ZZ"), None);
        assert_eq!(back.posting("missing_field", "A"), None);
        assert_eq!(back.n_fields(), 2);
        assert_eq!(back.n_values("patient"), 2);
    }

    #[test]
    fn determinism_two_builds_byte_identical() {
        // lDifferent input order for the same logical content must encode
        // byte-identically (sorted fields/values/postings).
        let a = MetaIndex::build(&[(
            "f".to_string(),
            vec![Some("y".into()), Some("x".into()), Some("y".into())],
        )])
        .to_bytes();
        let b = MetaIndex::build(&[(
            "f".to_string(),
            vec![Some("y".into()), Some("x".into()), Some("y".into())],
        )])
        .to_bytes();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_index_roundtrips() {
        let mi = MetaIndex::build(&[]);
        let back = MetaIndex::from_bytes(&mi.to_bytes()).unwrap();
        assert_eq!(back.n_fields(), 0);
        assert_eq!(back.posting("x", "y"), None);
    }

    #[test]
    fn rejects_truncated_payload() {
        let mi = sample();
        let bytes = mi.to_bytes();
        // ldrop the trailing intpack blob: must be a typed error, not a panic.
        assert!(MetaIndex::from_bytes(&bytes[..bytes.len() - 4]).is_err());
    }

    #[test]
    fn rejects_hostile_field_count_without_aborting() {
        // ln_fields=u32::MAX would reserve ~200 GB and abort; must be typed Err.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes()); // version
        bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // n_fields
        assert!(MetaIndex::from_bytes(&bytes).is_err());
        let mut b2 = Vec::new(); // also a hostile n_values inside a real field
        b2.extend_from_slice(&1u32.to_le_bytes());
        b2.extend_from_slice(&1u32.to_le_bytes()); // n_fields = 1
        super::write_str(&mut b2, "f");
        b2.extend_from_slice(&u32::MAX.to_le_bytes()); // n_values
        assert!(MetaIndex::from_bytes(&b2).is_err());
    }

    #[test]
    fn rejects_non_ascending_postings() {
        // lgaps [5, 0] decode to postings [5, 5] (a duplicate) -> must reject.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u32.to_le_bytes()); // version
        bytes.extend_from_slice(&1u32.to_le_bytes()); // n_fields
        super::write_str(&mut bytes, "f");
        bytes.extend_from_slice(&1u32.to_le_bytes()); // n_values
        super::write_str(&mut bytes, "v");
        bytes.extend_from_slice(&2u32.to_le_bytes()); // count = 2
        super::write_blob(&mut bytes, &super::pack_u64s(&[5, 0])); // -> [5, 5]
        assert!(MetaIndex::from_bytes(&bytes).is_err());
    }
}
