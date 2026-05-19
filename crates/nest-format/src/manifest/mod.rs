//! Manifest types and constants. The `validate` impl lives in
//! `manifest::validate`; the canonical-JSON serializer in
//! `manifest::canonical`. This module owns the data shape only.

mod canonical;
mod validate;

pub use validate::{
    ALLOWED_DTYPES, ALLOWED_INDEX_TYPES, ALLOWED_RERANK_POLICIES, ALLOWED_SCORE_TYPES,
    SUPPORTED_DTYPE, SUPPORTED_INDEX_TYPE, SUPPORTED_METRIC, SUPPORTED_NORMALIZE,
    SUPPORTED_RERANK_POLICY, SUPPORTED_SCORE_TYPE,
};

use crate::layout::{NEST_FORMAT_VERSION, NEST_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// lCapabilities advertised by a .nest file. v1 only requires `supports_exact`
/// and `supports_reproducible_build` to be true; the rest are forward-looking
/// flags so a runtime can decide what to do without reading every section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub supports_exact: bool,
    pub supports_ann: bool,
    pub supports_bm25: bool,
    pub supports_citations: bool,
    pub supports_reproducible_build: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            supports_exact: true,
            supports_ann: false,
            supports_bm25: false,
            supports_citations: true,
            supports_reproducible_build: true,
        }
    }
}

/// lJCS-canonical manifest for a .nest file.
///
/// lField order on disk follows declaration order. Extra keys land in `extra`
/// and are serialized in BTreeMap order; this keeps the JSON deterministic.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format_version: u32,
    pub schema_version: u32,
    pub embedding_model: String,
    pub embedding_dim: u32,
    pub n_chunks: u64,
    pub dtype: String,
    pub metric: String,
    pub score_type: String,
    pub normalize: String,
    pub index_type: String,
    pub rerank_policy: String,
    pub model_hash: String,
    pub chunker_version: String,
    pub capabilities: Capabilities,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            format_version: NEST_FORMAT_VERSION,
            schema_version: NEST_SCHEMA_VERSION,
            embedding_model: String::new(),
            embedding_dim: 0,
            n_chunks: 0,
            dtype: SUPPORTED_DTYPE.to_string(),
            metric: SUPPORTED_METRIC.to_string(),
            score_type: SUPPORTED_SCORE_TYPE.to_string(),
            normalize: SUPPORTED_NORMALIZE.to_string(),
            index_type: SUPPORTED_INDEX_TYPE.to_string(),
            rerank_policy: SUPPORTED_RERANK_POLICY.to_string(),
            model_hash: String::new(),
            chunker_version: String::new(),
            capabilities: Capabilities::default(),
            title: None,
            version: None,
            created: None,
            description: None,
            authors: None,
            license: None,
            extra: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NestError;

    pub(super) fn good_manifest() -> Manifest {
        Manifest {
            embedding_model: "demo".into(),
            embedding_dim: 4,
            n_chunks: 1,
            chunker_version: "demo-chunker/1".into(),
            model_hash: format!("sha256:{}", "0".repeat(64)),
            ..Default::default()
        }
    }

    #[test]
    fn default_is_invalid_until_filled() {
        assert!(Manifest::default().validate().is_err());
    }

    #[test]
    fn valid_manifest_passes() {
        good_manifest().validate().unwrap();
    }

    #[test]
    fn rejects_future_format_version() {
        let mut m = good_manifest();
        m.format_version = NEST_FORMAT_VERSION + 1;
        assert!(matches!(
            m.validate(),
            Err(NestError::UnsupportedFormatVersion(_))
        ));
    }

    #[test]
    fn accepts_older_format_version() {
        let mut m = good_manifest();
        if NEST_FORMAT_VERSION > 0 {
            m.format_version = NEST_FORMAT_VERSION - 1;
            assert!(m.validate().is_ok());
        }
    }

    #[test]
    fn rejects_future_schema_version() {
        let mut m = good_manifest();
        m.schema_version = NEST_SCHEMA_VERSION + 1;
        assert!(matches!(
            m.validate(),
            Err(NestError::UnsupportedSchemaVersion(_))
        ));
    }

    #[test]
    fn rejects_unsupported_dtype() {
        let mut m = good_manifest();
        m.dtype = "bfloat16".into();
        assert!(matches!(m.validate(), Err(NestError::UnsupportedDType(_))));
    }

    #[test]
    fn accepts_float16_and_int8_dtypes() {
        for dt in ["float32", "float16", "int8"] {
            let mut m = good_manifest();
            m.dtype = dt.into();
            m.validate()
                .unwrap_or_else(|e| panic!("{} rejected: {}", dt, e));
        }
    }

    #[test]
    fn hnsw_requires_exact_rerank() {
        let mut m = good_manifest();
        m.index_type = "hnsw".into();
        m.rerank_policy = "none".into();
        m.capabilities.supports_ann = true;
        assert!(matches!(m.validate(), Err(NestError::ManifestInvalid(_))));
        m.rerank_policy = "exact".into();
        m.validate().unwrap();
    }

    #[test]
    fn hybrid_requires_bm25_capability() {
        let mut m = good_manifest();
        m.index_type = "hybrid".into();
        m.rerank_policy = "exact".into();
        m.score_type = "hybrid_rrf".into();
        m.capabilities.supports_bm25 = false;
        assert!(matches!(m.validate(), Err(NestError::ManifestInvalid(_))));
        m.capabilities.supports_bm25 = true;
        m.validate().unwrap();
    }

    #[test]
    fn rejects_unsupported_metric() {
        let mut m = good_manifest();
        m.metric = "l2".into();
        assert!(matches!(m.validate(), Err(NestError::UnsupportedMetric(_))));
    }

    #[test]
    fn rejects_invalid_model_hash() {
        let mut m = good_manifest();
        m.model_hash = "deadbeef".into();
        assert!(matches!(m.validate(), Err(NestError::InvalidModelHash(_))));
    }

    #[test]
    fn rejects_missing_chunker_version() {
        let mut m = good_manifest();
        m.chunker_version = "".into();
        assert!(matches!(m.validate(), Err(NestError::ManifestInvalid(_))));
    }

    #[test]
    fn capabilities_required_for_v1() {
        let mut m = good_manifest();
        m.capabilities.supports_exact = false;
        assert!(matches!(m.validate(), Err(NestError::ManifestInvalid(_))));
    }

    #[test]
    fn canonical_json_is_deterministic() {
        let m1 = good_manifest();
        let m2 = good_manifest();
        assert_eq!(
            m1.to_canonical_json().unwrap(),
            m2.to_canonical_json().unwrap()
        );
    }
}
