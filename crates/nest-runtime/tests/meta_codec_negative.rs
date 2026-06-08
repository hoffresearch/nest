//! Negative coverage for the meta_index (0x17) codec via the PUBLIC API
//! (MetaIndex::from_bytes), so it lives outside the 300-line src guard. Guards
//! the decode trust boundary: a count/gap-blob mismatch and a hostile length
//! prefix must each be a typed error — never an allocator abort, a panic, or a
//! silently-wrong index (e.g. gaps[k] out of bounds).

use nest_format::encoding::pack_u64s;
use nest_runtime::meta::MetaIndex;

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&u32le(s.len() as u32));
    out.extend_from_slice(s.as_bytes());
}

#[test]
fn rejects_count_gap_blob_mismatch() {
    // version, 1 field "f", 1 value "v", count=3, but the gap blob has only 2
    // gaps -> total(3) != gaps.len()(2). The `gaps.len() != total` check (line
    // before the posting loop) must fire BEFORE Vec::with_capacity(count) and
    // any gaps[k] read, so this is a typed error, not an OOB/abort.
    let mut b = Vec::new();
    b.extend_from_slice(&u32le(1)); // version
    b.extend_from_slice(&u32le(1)); // n_fields
    push_str(&mut b, "f");
    b.extend_from_slice(&u32le(1)); // n_values
    push_str(&mut b, "v");
    b.extend_from_slice(&u32le(3)); // count = 3 (claims 3 postings)
    let blob = pack_u64s(&[5, 1]); // only 2 gaps
    b.extend_from_slice(&u32le(blob.len() as u32));
    b.extend_from_slice(&blob);
    assert!(
        MetaIndex::from_bytes(&b).is_err(),
        "count/gap-blob mismatch must be a typed error"
    );
}

#[test]
fn rejects_hostile_field_count_via_public_api() {
    // n_fields = u32::MAX with no following data: a typed error, never an abort
    // from reserving ~200 GB of Vec capacity from an unvalidated length prefix.
    let mut b = Vec::new();
    b.extend_from_slice(&u32le(1)); // version
    b.extend_from_slice(&u32le(u32::MAX)); // n_fields
    assert!(MetaIndex::from_bytes(&b).is_err());
}
