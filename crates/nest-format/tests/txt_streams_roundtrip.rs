//! positive coverage for the `txt_streams` wire codec (encoding id 10):
//! the per-chunk-streams re-layout of the `chunks_canonical` (0x02)
//! section. the load-bearing invariant is that `decode_payload(10, ...)`
//! rebuilds bytes BYTE-IDENTICAL to `encode_chunks_canonical`, so
//! `content_hash` (hashed over decoded bytes) and `nest://` citations are
//! unchanged. also covers O(1) single-chunk seek and determinism.

use nest_format::encoding::TxtStreams;
use nest_format::layout::SECTION_ENCODING_TXT_STREAMS;
use nest_format::{
    decode_payload, decode_txt_streams, encode_chunks_canonical, encode_txt_streams,
};

fn texts(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// the content_hash invariant: encode -> decode_payload(id 10) rebuilds the
/// EXACT canonical bytes for every corpus shape.
fn assert_decodes_byte_identical(items: &[&str]) {
    let t = texts(items);
    let raw = encode_chunks_canonical(&t).unwrap();
    let packed = encode_txt_streams(&t).unwrap();
    assert_eq!(packed[0], nest_format::TXT_STREAMS_V1, "kind/version byte");

    // through the wire-codec registry (the path the reader actually uses).
    let via_payload = decode_payload(SECTION_ENCODING_TXT_STREAMS, &packed).unwrap();
    assert_eq!(
        via_payload.as_ref(),
        raw.as_slice(),
        "decode_payload != raw"
    );

    // through the sections-level dispatch (parallel to decode_intpack_repack).
    let via_sections = decode_txt_streams(&packed).unwrap();
    assert_eq!(via_sections, raw, "decode_txt_streams != raw");
}

#[test]
fn empty_corpus_decodes_byte_identical() {
    assert_decodes_byte_identical(&[]);
}

#[test]
fn single_chunk_decodes_byte_identical() {
    assert_decodes_byte_identical(&["only one chunk of canonical text"]);
}

#[test]
fn many_chunks_decode_byte_identical() {
    let owned: Vec<String> = (0..200)
        .map(|i| format!("chunk number {i} with some body"))
        .collect();
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    assert_decodes_byte_identical(&refs);
}

#[test]
fn multibyte_utf8_ptbr_accents_decode_byte_identical() {
    // pt-br accents + empty string in the middle must round-trip byte-exact.
    assert_decodes_byte_identical(&[
        "coração",
        "informação pública",
        "açaí é ótimo à tarde",
        "",
        "São Paulo, ñ, ü, ç",
    ]);
}

#[test]
fn offset_table_o1_seek_returns_the_right_stream() {
    let t = texts(&["alpha", "beta", "coração", "delta", "épsilon"]);
    let packed = encode_txt_streams(&t).unwrap();
    let parsed = TxtStreams::parse(&packed).unwrap();
    assert_eq!(parsed.len(), t.len());
    assert!(!parsed.is_empty());
    // each chunk i seeks to the right stream WITHOUT decoding its neighbors.
    for (i, s) in t.iter().enumerate() {
        assert_eq!(&parsed.text(i).unwrap(), s, "O(1) seek text({i}) mismatch");
    }
    // out-of-range index is a typed error, never a panic.
    assert!(parsed.text(t.len()).is_err());
}

#[test]
fn two_encodes_are_byte_identical_deterministic() {
    let t = texts(&["a", "bb", "ccc", "coração", "dddd"]);
    assert_eq!(
        encode_txt_streams(&t).unwrap(),
        encode_txt_streams(&t).unwrap()
    );
}
