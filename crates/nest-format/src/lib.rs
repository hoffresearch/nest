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
    TXT_STREAMS_V2, TXT_STREAMS_V3, TxtStreams, decode_payload, decode_payload_with_dict,
    encode_int4_embeddings, encode_int8_embeddings, encode_txt_streams, expected_embeddings_size,
    f16_bytes_to_f32, f32_to_f16_bytes, int4_blocks_per_row, nibble_to_i4, pack_nibbles,
    quantize_f32_to_i4, quantize_f32_to_i8, zstd_encode,
};
pub use error::{NestError, Result};
pub use layout::*;
pub use manifest::{Capabilities, Manifest};
pub use reader::NestView;
pub use sections::blob::{
    BLOB_REF_NONE, BLOB_REFS_PAYLOAD_VERSION, BLOB_SPAN_OVERLAY_PAYLOAD_VERSION, BlobRefRecord,
    BlobSpanEntry, decode_blob_refs, decode_blob_span_overlay, encode_blob_refs,
    encode_blob_span_overlay,
};
pub use sections::graph::{
    CsrParts, EDGE_TYPE_CITATION, EDGE_TYPE_NEXT_CHUNK, EDGE_TYPE_SEMANTIC, Edge,
    GRAPH_ADJACENCY_PAYLOAD_VERSION, GRAPH_MAX_DEGREE, decode_graph_adjacency,
    encode_graph_adjacency, parse_csr_parts,
};
pub use sections::{
    OriginalSpan, SearchContract, decode_chunk_ids, decode_chunks_canonical,
    decode_chunks_original_spans, decode_provenance, decode_search_contract, decode_txt_streams,
    encode_chunks_canonical,
};
pub use writer::{EmbeddingDType, NestFileBuilder, SectionEncoding};
