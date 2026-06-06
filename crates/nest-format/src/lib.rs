pub mod chunk;
pub mod encoding;
pub mod error;
pub mod layout;
pub mod manifest;
pub mod reader;
pub mod sections;
pub mod writer;

pub use chunk::{ChunkInput, chunk_id};
pub use encoding::{
    DEFAULT_ZSTD_LEVEL, INT4_BLOCK, Int4EmbeddingsView, Int8EmbeddingsView, TXT_STREAMS_V1,
    TxtStreams, decode_payload, encode_int4_embeddings, encode_int8_embeddings, encode_txt_streams,
    expected_embeddings_size, f16_bytes_to_f32, f32_to_f16_bytes, int4_blocks_per_row,
    nibble_to_i4, pack_nibbles, quantize_f32_to_i4, quantize_f32_to_i8, zstd_encode,
};
pub use error::{NestError, Result};
pub use layout::*;
pub use manifest::{Capabilities, Manifest};
pub use reader::NestView;
pub use sections::{
    OriginalSpan, SearchContract, decode_chunk_ids, decode_chunks_canonical,
    decode_chunks_original_spans, decode_provenance, decode_search_contract, decode_txt_streams,
    encode_chunks_canonical,
};
pub use writer::{EmbeddingDType, NestFileBuilder, SectionEncoding};
