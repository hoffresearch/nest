//! section payload encoding (raw / zstd / float16 / int8).
//!
//! Two orthogonal axes:
//!
//! - **Wire encoding** of a section payload (`SECTION_ENCODING_*` in
//!   `layout`): how the bytes are stored on disk. `raw` and `zstd`
//!   apply to any non-embedding section. `float16` and `int8` only
//!   apply to the embeddings section.
//!
//! - **Logical dtype** of an embeddings section (`Manifest::dtype`):
//!   how to interpret the bytes after wire decoding. `float32` is the
//!   recall-max baseline; `float16` and `int8` are smaller-but-lossy
//!   variants that always accumulate in `f32` at search time.
//!
//! Section checksums (`SectionEntry::checksum`) hash the **physical**
//! bytes as stored. `content_hash` hashes the **decoded** bytes so two
//! files with the same logical content but different wire encoding
//! still produce the same content_hash for non-quantized sections.

mod dedup;
mod float16;
mod fsst;
mod int4;
mod int8;
mod intpack;
mod txt_streams;
mod zstd_codec;
mod zstd_dict;

pub use dedup::{
    DEDUP_MAP_V1, Deduped, decode_map as decode_dedup_map, dedup, encode_map as encode_dedup_map,
    expand as expand_dedup,
};
pub use float16::{f16_bytes_to_f32, f32_to_f16_bytes};
pub use fsst::{TXT_STREAMS_V3, decode as decode_fsst_payload, encode as encode_fsst};
pub use int4::{
    INT4_BLOCK, INT4_PAYLOAD_VERSION, INT4_PREFIX_SIZE, INT4_SCALE_KIND_PER_GROUP,
    Int4EmbeddingsView, encode_int4_embeddings, int4_blocks_per_row, nibble_to_i4, pack_nibbles,
    quantize_f32_to_i4,
};
pub use int8::{
    INT8_PAYLOAD_VERSION, INT8_PREFIX_SIZE, INT8_SCALE_KIND_PER_VECTOR, Int8EmbeddingsView,
    encode_int8_embeddings, quantize_f32_to_i8,
};
pub use intpack::{INTPACK_BLOCK, IntpackReader, pack_u64s, unpack_u64s};
pub use txt_streams::{
    TXT_STREAMS_V1, TxtStreams, decode as decode_txt_streams_payload, encode_txt_streams,
};
pub use zstd_codec::{DEFAULT_ZSTD_LEVEL, zstd_encode};
pub use zstd_dict::{
    MAX_DICT_BYTES, TXT_STREAMS_V2, decode as decode_zstd_dict_payload, encode as encode_zstd_dict,
    train_dict,
};

use crate::error::NestError;
use crate::layout::{
    SECTION_ENCODING_FLOAT16, SECTION_ENCODING_FSST, SECTION_ENCODING_INT4, SECTION_ENCODING_INT8,
    SECTION_ENCODING_INTPACK, SECTION_ENCODING_RAW, SECTION_ENCODING_TXT_STREAMS,
    SECTION_ENCODING_ZSTD, SECTION_ENCODING_ZSTD_DICT,
};
use std::borrow::Cow;

/// lThe context-free wire codecs, as a small registry. Decoding dispatches
/// through `WireCodec::from_id`, so adding a reserved codec is a localized
/// additive diff: a variant, a `from_id` arm, a `decode` arm, and its own
/// `<=300`-line module. Reserved-but-unimplemented ids (and the dict codec,
/// which needs section 0x0A) are deliberately ABSENT here so old and new
/// readers agree on the frozen wire format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WireCodec {
    Raw,
    Zstd,
    Float16Embeddings,
    Int8Embeddings,
    Int4Embeddings,
    Intpack,
    TxtStreams,
    Fsst,
}

impl WireCodec {
    fn from_id(encoding: u32) -> Option<Self> {
        match encoding {
            SECTION_ENCODING_RAW => Some(Self::Raw),
            SECTION_ENCODING_ZSTD => Some(Self::Zstd),
            SECTION_ENCODING_FLOAT16 => Some(Self::Float16Embeddings),
            SECTION_ENCODING_INT8 => Some(Self::Int8Embeddings),
            SECTION_ENCODING_INT4 => Some(Self::Int4Embeddings),
            SECTION_ENCODING_INTPACK => Some(Self::Intpack),
            SECTION_ENCODING_TXT_STREAMS => Some(Self::TxtStreams),
            SECTION_ENCODING_FSST => Some(Self::Fsst),
            _ => None,
        }
    }

    fn decode<'a>(self, bytes: &'a [u8]) -> crate::Result<Cow<'a, [u8]>> {
        // intpack / txt_streams / fsst all decode BYTE-IDENTICALLY to the raw
        // canonical payload, so content_hash and citations are unchanged; raw
        // and the embedding-only encodings ARE their canonical bytes (the
        // runtime dispatches on `dtype`).
        match self {
            Self::Raw | Self::Float16Embeddings | Self::Int8Embeddings | Self::Int4Embeddings => {
                Ok(Cow::Borrowed(bytes))
            }
            Self::Zstd => zstd_codec::zstd_decode(bytes).map(Cow::Owned),
            Self::Intpack => crate::sections::decode_intpack_repack(bytes).map(Cow::Owned),
            Self::TxtStreams => crate::sections::decode_txt_streams(bytes).map(Cow::Owned),
            Self::Fsst => fsst::decode(bytes).map(Cow::Owned),
        }
    }
}

/// lDecode a section payload from its on-disk encoding to the logical bytes
/// a reader consumes, via the wire-codec registry. For `raw` this is a
/// borrow; for `zstd` an owned decompressed buffer. The `zstd_dict` (id 5)
/// codec needs the shared dictionary (section 0x0A) and is decoded via
/// [`decode_payload_with_dict`], so it is rejected here. Unknown or
/// reserved-but-unimplemented encodings are rejected.
pub fn decode_payload(encoding: u32, bytes: &[u8]) -> crate::Result<Cow<'_, [u8]>> {
    match WireCodec::from_id(encoding) {
        Some(codec) => codec.decode(bytes),
        None => Err(NestError::UnsupportedSectionEncoding {
            section_id: 0,
            encoding,
        }),
    }
}

/// lDecode a chunks_canonical payload that MAY be dict-framed (`zstd_dict`,
/// id 5), supplying the shared dictionary from section 0x0A. all other
/// encodings ignore the dict and route through [`decode_payload`]. the dict
/// variant decodes BYTE-IDENTICALLY to the raw chunks_canonical payload, so
/// content_hash and citations are unchanged.
pub fn decode_payload_with_dict<'a>(
    encoding: u32,
    bytes: &'a [u8],
    dict: Option<&[u8]>,
) -> crate::Result<Cow<'a, [u8]>> {
    if encoding == SECTION_ENCODING_ZSTD_DICT {
        let dict = dict.ok_or_else(|| NestError::MalformedSectionPayload {
            section_id: 0,
            reason: "zstd_dict: dict-framed section but no dictionary (0x0A)".into(),
        })?;
        return zstd_dict::decode(bytes, dict).map(Cow::Owned);
    }
    decode_payload(encoding, bytes)
}

/// lEncode `payload` with one non-embedding wire encoding (raw or zstd).
/// The embedding dtypes (float16/int8) are not general-purpose encoders
/// and are rejected here; they are chosen by preset on the embeddings
/// section directly.
fn encode_wire(encoding: u32, payload: &[u8]) -> crate::Result<Vec<u8>> {
    match encoding {
        SECTION_ENCODING_RAW => Ok(payload.to_vec()),
        SECTION_ENCODING_ZSTD => zstd_codec::zstd_encode(payload),
        other => Err(NestError::UnsupportedSectionEncoding {
            section_id: 0,
            encoding: other,
        }),
    }
}

/// lCost-driven encoder: try every candidate wire encoding and return the
/// `(encoding_id, bytes)` of the SMALLEST result, so the writer can record
/// the chosen id in the section entry. Ties break toward the EARLIEST
/// candidate (cheaper-to-decode wins an equal-size race). This only auto-
/// picks among non-embedding encodings; existing presets that name an
/// encoding explicitly are untouched, so the frozen output stays
/// byte-identical.
pub fn encode_smallest(candidates: &[u32], payload: &[u8]) -> crate::Result<(u32, Vec<u8>)> {
    let mut best: Option<(u32, Vec<u8>)> = None;
    for &enc in candidates {
        let bytes = encode_wire(enc, payload)?;
        let smaller = best.as_ref().is_none_or(|(_, b)| bytes.len() < b.len());
        if smaller {
            best = Some((enc, bytes));
        }
    }
    best.ok_or_else(|| NestError::InvalidInput("encode_smallest: no candidate encodings".into()))
}

/// lExpected size of the embeddings section for a given dtype. Returns
/// `None` for unknown dtypes.
pub fn expected_embeddings_size(dtype: &str, n: usize, dim: usize) -> Option<usize> {
    match dtype {
        "float32" => Some(n * dim * 4),
        "float16" => Some(n * dim * 2),
        "int8" => Some(INT8_PREFIX_SIZE + n * 4 + n * dim),
        // lprefix + f16 group scales (n * dim/64) + packed nibbles (n * dim/2).
        "int4" if dim % INT4_BLOCK == 0 => {
            Some(INT4_PREFIX_SIZE + n * (dim / INT4_BLOCK) * 2 + n * dim / 2)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_roundtrip_preserves_bytes() {
        let original = b"hello hello hello world world world".repeat(64);
        let compressed = zstd_encode(&original).unwrap();
        assert!(
            compressed.len() < original.len(),
            "zstd should shrink repetitive text"
        );
        let decoded = decode_payload(SECTION_ENCODING_ZSTD, &compressed).unwrap();
        assert_eq!(decoded.as_ref(), original.as_slice());
    }

    #[test]
    fn raw_decode_borrows() {
        let bytes = b"plain";
        let decoded = decode_payload(SECTION_ENCODING_RAW, bytes).unwrap();
        assert!(matches!(decoded, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn unknown_encoding_rejected() {
        let res = decode_payload(99, &[]);
        assert!(matches!(
            res,
            Err(NestError::UnsupportedSectionEncoding { encoding: 99, .. })
        ));
    }

    #[test]
    fn intpack_decode_payload_rebuilds_canonical_bytes() {
        // intpack (id 4) repacks chunk_ids/spans; decode_payload must
        // rebuild the BYTE-IDENTICAL raw payload so content_hash (hashed
        // over decoded bytes) is unchanged and citations stay stable.
        use crate::sections::{encode_chunk_ids, encode_chunk_ids_intpack};
        let ids: Vec<String> = (0u8..3)
            .map(|i| format!("sha256:{}", hex::encode([i; 32])))
            .collect();
        let packed = encode_chunk_ids_intpack(&ids).unwrap();
        let decoded = decode_payload(SECTION_ENCODING_INTPACK, &packed).unwrap();
        assert_eq!(decoded.as_ref(), encode_chunk_ids(&ids).unwrap().as_slice());
        // a malformed intpack payload is a typed error, never a panic.
        assert!(decode_payload(SECTION_ENCODING_INTPACK, &[]).is_err());
    }

    #[test]
    fn wire_codec_registry_maps_only_implemented_ids() {
        use crate::layout::{
            SECTION_ENCODING_FRONTCODE, SECTION_ENCODING_FSST, SECTION_ENCODING_INT4,
            SECTION_ENCODING_INTPACK, SECTION_ENCODING_RABITQ, SECTION_ENCODING_RAW,
            SECTION_ENCODING_TXT_STREAMS, SECTION_ENCODING_ZSTD, SECTION_ENCODING_ZSTD_DICT,
        };
        assert!(WireCodec::from_id(SECTION_ENCODING_RAW).is_some());
        assert!(WireCodec::from_id(SECTION_ENCODING_ZSTD).is_some());
        // intpack (id 4) is now implemented and in the registry.
        assert!(WireCodec::from_id(SECTION_ENCODING_INTPACK).is_some());
        // int4 (id 7) is now implemented and in the registry.
        assert!(WireCodec::from_id(SECTION_ENCODING_INT4).is_some());
        // txt_streams (id 10) is now implemented and in the registry.
        assert!(WireCodec::from_id(SECTION_ENCODING_TXT_STREAMS).is_some());
        // fsst (id 9) is now implemented and in the registry (self-contained).
        assert!(WireCodec::from_id(SECTION_ENCODING_FSST).is_some());
        // zstd_dict (id 5) is implemented but needs the shared dictionary
        // from section 0x0A, so it is NOT in the context-free registry: it is
        // decoded via `decode_payload_with_dict`, not `decode_payload`.
        assert!(WireCodec::from_id(SECTION_ENCODING_ZSTD_DICT).is_none());
        // still-reserved-but-unimplemented ids stay rejected: frontcode(6),
        // rabitq(8), and any unknown id.
        assert!(WireCodec::from_id(SECTION_ENCODING_FRONTCODE).is_none());
        assert!(WireCodec::from_id(SECTION_ENCODING_RABITQ).is_none());
        assert!(WireCodec::from_id(0xFF).is_none());
    }

    #[test]
    fn encode_smallest_picks_the_winner_and_records_its_id() {
        use crate::layout::{SECTION_ENCODING_RAW, SECTION_ENCODING_ZSTD};
        let candidates = [SECTION_ENCODING_RAW, SECTION_ENCODING_ZSTD];

        // lhighly repetitive payload: zstd wins, and the chosen id is recorded.
        let compressible = b"abcabcabcabc".repeat(64);
        let (id, bytes) = encode_smallest(&candidates, &compressible).unwrap();
        assert_eq!(id, SECTION_ENCODING_ZSTD);
        assert!(bytes.len() < compressible.len());

        // ltiny payload: zstd framing overhead loses, raw wins (first on ties).
        let (id2, _) = encode_smallest(&candidates, b"x").unwrap();
        assert_eq!(id2, SECTION_ENCODING_RAW);
    }

    #[test]
    fn encode_smallest_rejects_embedding_and_empty_candidates() {
        use crate::layout::SECTION_ENCODING_INT8;
        assert!(encode_smallest(&[SECTION_ENCODING_INT8], b"data").is_err());
        assert!(encode_smallest(&[], b"data").is_err());
    }
}
