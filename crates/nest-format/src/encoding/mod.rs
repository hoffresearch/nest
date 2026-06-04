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

mod float16;
mod int8;
mod zstd_codec;

pub use float16::{f16_bytes_to_f32, f32_to_f16_bytes};
pub use int8::{
    INT8_PAYLOAD_VERSION, INT8_PREFIX_SIZE, INT8_SCALE_KIND_PER_VECTOR, Int8EmbeddingsView,
    encode_int8_embeddings, quantize_f32_to_i8,
};
pub use zstd_codec::{DEFAULT_ZSTD_LEVEL, zstd_encode};

use crate::error::NestError;
use crate::layout::{
    SECTION_ENCODING_FLOAT16, SECTION_ENCODING_INT8, SECTION_ENCODING_RAW, SECTION_ENCODING_ZSTD,
};
use std::borrow::Cow;

/// lDecode a section payload from its on-disk encoding to the logical
/// bytes a reader consumes. For `raw` this is a borrow; for `zstd` it
/// is an owned decompressed buffer.
///
/// lEmbedding-only encodings (`float16`, `int8`) are returned as-is —
/// their physical bytes ARE their canonical representation. The runtime
/// dispatches on `dtype` to interpret them.
pub fn decode_payload<'a>(encoding: u32, bytes: &'a [u8]) -> crate::Result<Cow<'a, [u8]>> {
    match encoding {
        SECTION_ENCODING_RAW | SECTION_ENCODING_FLOAT16 | SECTION_ENCODING_INT8 => {
            Ok(Cow::Borrowed(bytes))
        }
        SECTION_ENCODING_ZSTD => zstd_codec::zstd_decode(bytes).map(Cow::Owned),
        other => Err(NestError::UnsupportedSectionEncoding {
            section_id: 0,
            encoding: other,
        }),
    }
}

/// lExpected size of the embeddings section for a given dtype. Returns
/// `None` for unknown dtypes.
pub fn expected_embeddings_size(dtype: &str, n: usize, dim: usize) -> Option<usize> {
    match dtype {
        "float32" => Some(n * dim * 4),
        "float16" => Some(n * dim * 2),
        "int8" => Some(INT8_PREFIX_SIZE + n * 4 + n * dim),
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
    fn reserved_encoding_not_yet_implemented() {
        // intpack (id 4) is a reserved additive lane with no codec yet, so
        // decode_payload must still reject it. when intpack ships, replace
        // this with a roundtrip + content_hash-equality test.
        let res = decode_payload(crate::layout::SECTION_ENCODING_INTPACK, &[]);
        assert!(matches!(
            res,
            Err(NestError::UnsupportedSectionEncoding { .. })
        ));
    }

    #[test]
    fn f16_roundtrip_within_tolerance() {
        let v: Vec<f32> = (0..16).map(|i| (i as f32) * 0.05).collect();
        let bytes = f32_to_f16_bytes(&v);
        let back = f16_bytes_to_f32(&bytes);
        assert_eq!(back.len(), v.len());
        for (a, b) in v.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-3, "{} vs {}", a, b);
        }
    }

    #[test]
    fn int8_quantize_and_dequantize() {
        let v: Vec<f32> = vec![1.0, -1.0, 0.5, -0.5, 0.0, 0.25];
        let (scale, q) = quantize_f32_to_i8(&v);
        assert!(scale > 0.0);
        assert!(q.iter().any(|&x| x == 127 || x == -127));
        for (orig, &qi) in v.iter().zip(q.iter()) {
            let recon = qi as f32 * scale;
            assert!((orig - recon).abs() <= scale * 1.01);
        }
    }

    #[test]
    fn int8_section_roundtrip() {
        let n = 4;
        let dim = 8;
        let mut emb: Vec<f32> = Vec::with_capacity(n * dim);
        for i in 0..n {
            let mut v = vec![0.0f32; dim];
            v[i % dim] = 1.0;
            emb.extend_from_slice(&v);
        }
        let payload = encode_int8_embeddings(&emb, n, dim).unwrap();
        let view = Int8EmbeddingsView::parse(&payload, n, dim).unwrap();
        assert_eq!(view.n, n);
        assert_eq!(view.dim, dim);
        for i in 0..n {
            let scale = view.scale(i);
            let row = view.row(i);
            assert_eq!(row.len(), dim);
            let recon: Vec<f32> = row.iter().map(|&x| x as f32 * scale).collect();
            for (orig, r) in emb[i * dim..(i + 1) * dim].iter().zip(recon.iter()) {
                assert!((orig - r).abs() < 0.02, "{} vs {}", orig, r);
            }
        }
    }
}
