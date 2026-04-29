//! Decoded section access + identity hashes (file_hash, content_hash).

use std::borrow::Cow;

use super::NestView;
use crate::encoding::decode_payload;
use crate::error::NestError;
use crate::layout::{CANONICAL_SECTIONS, SECTION_SEARCH_CONTRACT};
use crate::sections::{SearchContract, decode_search_contract};

impl<'a> NestView<'a> {
    /// Logical (decoded) bytes of a section's payload. Borrows for raw
    /// encoding; copies for zstd. Float16/int8 embedding payloads are
    /// returned as-is — the runtime dispatches on `manifest.dtype`.
    pub fn decoded_section(&self, section_id: u32) -> crate::Result<Cow<'a, [u8]>> {
        let entry = self.entry(section_id)?;
        let phys = self.get_section_data(section_id)?;
        decode_payload(entry.encoding, phys).map_err(|e| match e {
            NestError::UnsupportedSectionEncoding { encoding, .. } => {
                NestError::UnsupportedSectionEncoding {
                    section_id,
                    encoding,
                }
            }
            NestError::MalformedSectionPayload { reason, .. } => {
                NestError::MalformedSectionPayload { section_id, reason }
            }
            other => other,
        })
    }

    /// Decode the `search_contract` section. Already validated to agree
    /// with the manifest at construction time.
    pub fn search_contract(&self) -> crate::Result<SearchContract> {
        let bytes = self.decoded_section(SECTION_SEARCH_CONTRACT)?;
        decode_search_contract(&bytes)
    }

    /// `sha256:<hex>` of the file as written, including the footer.
    pub fn file_hash_hex(&self) -> String {
        use sha2::{Digest, Sha256};
        let h = Sha256::digest(self.data);
        format!("sha256:{}", hex::encode(h))
    }

    /// `sha256:<hex>` of the canonical sections in the order fixed by spec
    /// (see `CANONICAL_SECTIONS`). Hashes the **decoded** bytes so two
    /// files that wire-compress the same logical content (zstd vs raw)
    /// produce the same content_hash and therefore stable citations.
    /// Quantized embeddings (float16 / int8) hash their on-disk bytes —
    /// they're already the canonical representation for that precision.
    /// Optional sections (HNSW, BM25) are NOT included.
    pub fn content_hash_hex(&self) -> crate::Result<String> {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (id, name) in CANONICAL_SECTIONS {
            let bytes = self.decoded_section(*id)?;
            // Domain-separate by name length + name bytes so hashes for
            // different sections cannot collide via concatenation.
            h.update((name.len() as u32).to_le_bytes());
            h.update(name.as_bytes());
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(bytes.as_ref());
        }
        Ok(format!("sha256:{}", hex::encode(h.finalize())))
    }
}
