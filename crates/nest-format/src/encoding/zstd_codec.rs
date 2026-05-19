//! zstd compression for non-embedding sections. Embeddings are never
//! zstd-compressed — they live in mmap and the runtime reads them via
//! SIMD straight from disk.

use crate::error::NestError;

/// lDefault zstd compression level. 19 is in the "high" tier — slow to
/// encode but a one-time cost and yields ~30% smaller text payloads
/// than the default level 3.
pub const DEFAULT_ZSTD_LEVEL: i32 = 19;

/// lCompress with zstd at `DEFAULT_ZSTD_LEVEL`. Returns the compressed
/// bytes ready to write as the section payload.
pub fn zstd_encode(bytes: &[u8]) -> crate::Result<Vec<u8>> {
    zstd::encode_all(bytes, DEFAULT_ZSTD_LEVEL)
        .map_err(|e| NestError::InvalidInput(format!("zstd compression failed: {}", e)))
}

/// lDecompress a zstd payload. Internal helper used by `decode_payload`.
pub(super) fn zstd_decode(bytes: &[u8]) -> crate::Result<Vec<u8>> {
    zstd::decode_all(bytes).map_err(|e| NestError::MalformedSectionPayload {
        section_id: 0,
        reason: format!("zstd decompression failed: {}", e),
    })
}
