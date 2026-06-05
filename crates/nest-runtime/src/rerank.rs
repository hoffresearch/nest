//! Rerank-source handle: the single place the mandatory exact-cosine
//! rerank reads its vectors from.
//!
//! The honesty contract is that every non-exact search path (ann, hybrid,
//! and the future graph/space/cross paths) only *generates candidates*;
//! the returned `score` is always a real cosine recomputed here against a
//! fixed-stride embeddings slab. Routing that recompute through one
//! explicit handle (instead of hard-dispatching on a single `self.dtype`
//! and the single `embeddings` section) is what lets per-space and
//! sub-int8 paths plug into the SAME gate later without weakening it.
//!
//! Two precisions:
//!
//! - no fp source: the rerank reads the STORED dtype slab, so the score
//!   is "real cosine at stored precision" (full precision for float32,
//!   slightly lossy for float16/int8). this is today's behavior.
//! - with an `embeddings_fp` (0x09) source present: the rerank reads the
//!   full-precision fp slab instead, so a sub-int8 candidate slab still
//!   yields a real cosine. the fp source is mandatory for any sub-int8
//!   section (enforced where that section is written, phase 3); here we
//!   only consume it.
//!
//! The fp slab is a fixed-stride 64-byte-aligned raw slab (float32 or
//! float16), NEVER zstd, so the existing simd kernels score it byte-for-
//! byte unchanged; its dtype is read back from the section stride.

use nest_format::layout::{SECTION_EMBEDDINGS_FP, SectionEntry};
use nest_format::{Int8EmbeddingsView, NestError};

use crate::error::RuntimeError;
use crate::mmap_file::DType;
use crate::simd;

/// lThe optional full-precision rerank slab (`embeddings_fp`, section
/// `0x09`): a fixed-stride 64-byte-aligned raw slab, NEVER zstd, one row
/// per embedding. Present only when a sub-int8 candidate slab needs an
/// honest exact rerank. The dtype (float32 or float16 only) is read back
/// from the section stride. The writer side lands with the sub-int8
/// codecs (phase 3); the runtime read path is here so the rerank handle
/// can route to it byte-for-byte unchanged when it appears.
#[derive(Clone, Copy)]
pub(crate) struct FpSlab {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) dtype: DType,
}

impl FpSlab {
    /// lDetect an `embeddings_fp` (0x09) section in the table and infer its
    /// dtype from stride: 4 bytes/value -> float32, 2 -> float16 (an fp
    /// source is never int8). A bad stride is a malformed file, not a
    /// silent skip. Returns `None` when the section is absent.
    pub(crate) fn detect(
        entries: &[SectionEntry],
        n: usize,
        dim: usize,
    ) -> Result<Option<Self>, RuntimeError> {
        let Some(e) = entries
            .iter()
            .find(|e| e.section_id == SECTION_EMBEDDINGS_FP)
        else {
            return Ok(None);
        };
        let size = e.size as usize;
        let stride = dim.checked_mul(n).filter(|d| *d > 0).map(|d| size / d);
        let dtype = match stride {
            Some(4) => DType::Float32,
            Some(2) => DType::Float16,
            _ => {
                return Err(RuntimeError::Format(NestError::MalformedSectionPayload {
                    section_id: SECTION_EMBEDDINGS_FP,
                    reason: format!("fp slab {size} bytes is not f32/f16 for n={n} dim={dim}"),
                }));
            }
        };
        Ok(Some(Self {
            offset: e.offset as usize,
            size,
            dtype,
        }))
    }
}

/// lWhere one rerank reads its vectors from. Built once per search call,
/// then `score`d per candidate. Borrows the mmap slab; no copy.
pub(crate) struct RerankSource<'a> {
    rows: RerankRows<'a>,
    dim: usize,
}

enum RerankRows<'a> {
    F32(&'a [u8]),
    F16(&'a [u8]),
    Int8(Int8EmbeddingsView<'a>),
}

impl<'a> RerankSource<'a> {
    /// lBuild a rerank source over a fixed-stride embeddings slab. `bytes`
    /// is the raw section payload for `dtype` (for int8 that includes the
    /// per-vector scale prefix, parsed here once).
    pub(crate) fn new(
        dtype: DType,
        bytes: &'a [u8],
        n: usize,
        dim: usize,
    ) -> Result<Self, RuntimeError> {
        let rows = match dtype {
            DType::Float32 => RerankRows::F32(bytes),
            DType::Float16 => RerankRows::F16(bytes),
            DType::Int8 => RerankRows::Int8(
                Int8EmbeddingsView::parse(bytes, n, dim).map_err(RuntimeError::Format)?,
            ),
        };
        Ok(Self { rows, dim })
    }

    /// lReal cosine of `qnorm` against row `i` at this source's precision.
    /// `qnorm` is L2-normalized and rows are L2-normalized at write time,
    /// so the dot product IS the cosine.
    #[inline]
    pub(crate) fn score(&self, qnorm: &[f32], i: usize) -> f32 {
        match &self.rows {
            RerankRows::F32(b) => {
                let rs = self.dim * 4;
                let off = i * rs;
                simd::dot_f32_bytes(qnorm, &b[off..off + rs])
            }
            RerankRows::F16(b) => {
                let rs = self.dim * 2;
                let off = i * rs;
                simd::dot_f32_f16_bytes(qnorm, &b[off..off + rs])
            }
            RerankRows::Int8(view) => simd::dot_f32_i8(qnorm, view.row(i), view.scale(i)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nest_format::{encode_int8_embeddings, f32_to_f16_bytes};

    fn f32_slab(rows: &[[f32; 4]]) -> Vec<u8> {
        let mut b = Vec::new();
        for row in rows {
            for v in row {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn f32_source_dot_is_cosine() {
        let rows = [[1.0f32, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]];
        let bytes = f32_slab(&rows);
        let src = RerankSource::new(DType::Float32, &bytes, 2, 4).unwrap();
        let q = [1.0f32, 0.0, 0.0, 0.0];
        assert!((src.score(&q, 0) - 1.0).abs() < 1e-6);
        assert!(src.score(&q, 1).abs() < 1e-6);
    }

    /// lThe handle scores whatever slab it is handed. A full-precision fp
    /// slab returns the exact dot; the int8 slab over the SAME logical
    /// vector returns a quantized approximation. They differ, which is
    /// exactly why an fp rerank source makes a sub-int8 score honest.
    #[test]
    fn fp_source_is_more_precise_than_int8_for_same_vectors() {
        // lOne L2-normalized row with values int8 cannot represent exactly.
        let raw = [0.0312f32, 0.5007, -0.2013, 0.8401];
        let norm = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
        let row: [f32; 4] = std::array::from_fn(|j| raw[j] / norm);
        let q: [f32; 4] = row; // query == the row, exact cosine is 1.0

        let f32_bytes = f32_slab(&[row]);
        let fp = RerankSource::new(DType::Float32, &f32_bytes, 1, 4).unwrap();
        let int8_bytes = encode_int8_embeddings(&row, 1, 4).unwrap();
        let q8 = RerankSource::new(DType::Int8, &int8_bytes, 1, 4).unwrap();

        let s_fp = fp.score(&q, 0);
        let s_i8 = q8.score(&q, 0);
        assert!((s_fp - 1.0).abs() < 1e-6, "fp source must be exact: {s_fp}");
        assert!(
            (s_i8 - 1.0).abs() > 1e-4,
            "int8 must lose precision so the fp source is the honest one: {s_i8}"
        );

        // lf16 is between the two: lossy but far closer than int8.
        let f16_bytes = f32_to_f16_bytes(&row);
        let f16 = RerankSource::new(DType::Float16, &f16_bytes, 1, 4).unwrap();
        assert!((f16.score(&q, 0) - 1.0).abs() < 1e-2);
    }
}
