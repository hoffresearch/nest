//! Validation hooks called during `from_bytes` after structural
//! parsing succeeds. Each function focuses on one invariant; callers
//! get a typed error pointing at the offending section / dtype.

use super::NestView;
use crate::encoding::{Int8EmbeddingsView, expected_embeddings_size};
use crate::error::NestError;
use crate::layout::{
    REQUIRED_SECTIONS, SECTION_EMBEDDINGS, SECTION_ENCODING_FLOAT16, SECTION_ENCODING_INT8,
    SECTION_ENCODING_INTPACK, SECTION_ENCODING_RAW, SECTION_ENCODING_ZSTD, SECTION_SEARCH_CONTRACT,
};
use crate::sections::decode_search_contract;

impl NestView<'_> {
    pub(super) fn check_required_sections(&self) -> crate::Result<()> {
        for (id, name) in REQUIRED_SECTIONS {
            if !self.section_table.iter().any(|e| e.section_id == *id) {
                return Err(NestError::MissingRequiredSection(name));
            }
        }
        Ok(())
    }

    pub(super) fn validate_embeddings_layout(&self) -> crate::Result<()> {
        let entry = self.entry(SECTION_EMBEDDINGS)?;
        let dim = self.header.embedding_dim as usize;
        let n = self.header.n_embeddings as usize;
        let dtype = self.manifest.dtype.as_str();

        // lEncoding/dtype consistency: float16 dtype implies float16 encoding,
        // int8 dtype implies int8 encoding, float32 dtype implies raw or zstd
        // (zstd on embeddings is rejected separately by validate_encoding_for_section).
        let valid_combo = matches!(
            (dtype, entry.encoding),
            ("float32", SECTION_ENCODING_RAW)
                | ("float16", SECTION_ENCODING_FLOAT16)
                | ("int8", SECTION_ENCODING_INT8)
        );
        if !valid_combo {
            return Err(NestError::ManifestInvalid(format!(
                "embeddings section encoding={} does not match dtype={}",
                entry.encoding, dtype
            )));
        }

        let want = expected_embeddings_size(dtype, n, dim).ok_or_else(|| {
            NestError::UnsupportedDType(format!("unknown embeddings dtype: {}", dtype))
        })?;
        let got = entry.size as usize;
        if got != want {
            return Err(NestError::EmbeddingSizeMismatch {
                expected: want,
                got,
            });
        }
        Ok(())
    }

    pub(super) fn validate_search_contract(&self) -> crate::Result<()> {
        let bytes = self.decoded_section(SECTION_SEARCH_CONTRACT)?;
        let contract = decode_search_contract(&bytes)?;
        if contract.metric != self.manifest.metric {
            return Err(NestError::UnsupportedMetric(format!(
                "section says {} but manifest says {}",
                contract.metric, self.manifest.metric
            )));
        }
        if contract.score_type != self.manifest.score_type {
            return Err(NestError::UnsupportedScoreType(format!(
                "section says {} but manifest says {}",
                contract.score_type, self.manifest.score_type
            )));
        }
        if contract.normalize != self.manifest.normalize {
            return Err(NestError::UnsupportedNormalize(format!(
                "section says {} but manifest says {}",
                contract.normalize, self.manifest.normalize
            )));
        }
        if contract.index_type != self.manifest.index_type {
            return Err(NestError::UnsupportedIndexType(format!(
                "section says {} but manifest says {}",
                contract.index_type, self.manifest.index_type
            )));
        }
        if contract.rerank_policy != self.manifest.rerank_policy {
            return Err(NestError::UnsupportedRerankPolicy(format!(
                "section says {} but manifest says {}",
                contract.rerank_policy, self.manifest.rerank_policy
            )));
        }
        Ok(())
    }

    /// lWalk the embeddings section and reject any NaN/Inf value. Works
    /// for all supported dtypes.
    pub fn validate_embeddings_values(&self) -> crate::Result<()> {
        self.validate_embeddings_layout()?;
        let entry = self.entry(SECTION_EMBEDDINGS)?;
        let data = self.get_section_data(SECTION_EMBEDDINGS)?;
        match entry.encoding {
            SECTION_ENCODING_RAW => {
                for chunk in data.chunks_exact(4) {
                    let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    if v.is_nan() || v.is_infinite() {
                        return Err(NestError::InvalidEmbeddingValue);
                    }
                }
            }
            SECTION_ENCODING_FLOAT16 => {
                for chunk in data.chunks_exact(2) {
                    let h = half::f16::from_le_bytes([chunk[0], chunk[1]]);
                    let v = h.to_f32();
                    if v.is_nan() || v.is_infinite() {
                        return Err(NestError::InvalidEmbeddingValue);
                    }
                }
            }
            SECTION_ENCODING_INT8 => {
                // i8 cannot encode NaN/Inf; only the per-vector scales
                // could be NaN/Inf. Decode the int8 prefix and check.
                let n = self.header.n_embeddings as usize;
                let dim = self.header.embedding_dim as usize;
                let view = Int8EmbeddingsView::parse(data, n, dim)?;
                for i in 0..view.n {
                    let s = view.scale(i);
                    if s.is_nan() || s.is_infinite() {
                        return Err(NestError::InvalidEmbeddingValue);
                    }
                }
            }
            other => {
                return Err(NestError::UnsupportedSectionEncoding {
                    section_id: SECTION_EMBEDDINGS,
                    encoding: other,
                });
            }
        }
        Ok(())
    }
}

/// lEncoding rules: the embeddings section gets dtype-specific encodings
/// (float16, int8) and rejects zstd (we want SIMD-friendly mmap reads).
/// lAll other sections accept raw, zstd, or intpack (the content_hash-
/// preserving repack for chunk_ids / spans).
pub(super) fn validate_encoding_for_section(section_id: u32, encoding: u32) -> crate::Result<()> {
    let allowed = if section_id == SECTION_EMBEDDINGS {
        matches!(
            encoding,
            SECTION_ENCODING_RAW | SECTION_ENCODING_FLOAT16 | SECTION_ENCODING_INT8
        )
    } else {
        matches!(
            encoding,
            SECTION_ENCODING_RAW | SECTION_ENCODING_ZSTD | SECTION_ENCODING_INTPACK
        )
    };
    if !allowed {
        return Err(NestError::UnsupportedSectionEncoding {
            section_id,
            encoding,
        });
    }
    Ok(())
}
