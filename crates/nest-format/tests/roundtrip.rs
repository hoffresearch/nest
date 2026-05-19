use std::path::PathBuf;

use nest_format::layout::*;
use nest_format::manifest::Manifest;
use nest_format::sections::{
    decode_chunk_ids, decode_chunks_canonical, decode_chunks_original_spans,
};
use nest_format::writer::NestFileBuilder;
use nest_format::{ChunkInput, NestView};

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(name);
    p
}

fn manifest(dim: u32, n: u64) -> Manifest {
    Manifest {
        embedding_model: "demo".into(),
        embedding_dim: dim,
        n_chunks: n,
        chunker_version: "demo-chunker/1".into(),
        model_hash: format!("sha256:{}", "0".repeat(64)),
        ..Default::default()
    }
}

fn chunk(text: &str, uri: &str, start: u64, end: u64, emb: Vec<f32>) -> ChunkInput {
    ChunkInput {
        canonical_text: text.into(),
        source_uri: uri.into(),
        byte_start: start,
        byte_end: end,
        embedding: emb,
    }
}

#[test]
fn writer_reader_roundtrip() {
    let path = tmp_path("test_minimal.nest");
    let _ = std::fs::remove_file(&path);

    NestFileBuilder::new(manifest(4, 2))
        .add_chunk(chunk("alpha", "doc.txt", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .add_chunk(chunk("beta", "doc.txt", 5, 9, vec![0.0, 1.0, 0.0, 0.0]))
        .write_to_path(&path)
        .unwrap();

    let data = std::fs::read(&path).unwrap();
    let view = NestView::from_bytes(&data).unwrap();
    assert_eq!(view.header.embedding_dim, 4);
    assert_eq!(view.header.n_chunks, 2);
    assert_eq!(view.header.n_embeddings, 2);
    assert_eq!(&view.header.magic, b"NEST");
    assert_eq!(view.manifest.embedding_model, "demo");
    assert_eq!(view.manifest.dtype, "float32");
    assert_eq!(view.manifest.metric, "ip");

    // lAll required sections must be present and parsable.
    let ids = decode_chunk_ids(view.get_section_data(SECTION_CHUNK_IDS).unwrap(), 2).unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids[0].starts_with("sha256:"));
    assert_ne!(ids[0], ids[1]);

    let texts =
        decode_chunks_canonical(view.get_section_data(SECTION_CHUNKS_CANONICAL).unwrap(), 2)
            .unwrap();
    assert_eq!(texts, vec!["alpha".to_string(), "beta".to_string()]);

    let spans = decode_chunks_original_spans(
        view.get_section_data(SECTION_CHUNKS_ORIGINAL_SPANS)
            .unwrap(),
        2,
    )
    .unwrap();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].source_uri, "doc.txt");
    assert_eq!(spans[0].byte_start, 0);
    assert_eq!(spans[0].byte_end, 5);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn same_input_same_hash() {
    let b1 = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("alpha", "doc.txt", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();
    let b2 = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("alpha", "doc.txt", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();
    assert_eq!(b1, b2);

    let v1 = NestView::from_bytes(&b1).unwrap();
    let v2 = NestView::from_bytes(&b2).unwrap();
    assert_eq!(v1.file_hash_hex(), v2.file_hash_hex());
    assert_eq!(
        v1.content_hash_hex().unwrap(),
        v2.content_hash_hex().unwrap()
    );
    assert!(v1.file_hash_hex().starts_with("sha256:"));
    assert!(v1.content_hash_hex().unwrap().starts_with("sha256:"));
}

#[test]
fn truncated_file_rejected() {
    // lAnything shorter than header + footer is unparseable.
    let res = NestView::from_bytes(&[0u8; 32]);
    assert!(matches!(res, Err(nest_format::NestError::FileTruncated)));

    // lExactly header + footer worth of bytes is still not a valid file —
    // it has neither a section table nor a manifest, and the magic is
    // wrong, so the reader rejects with MagicMismatch first.
    let res = NestView::from_bytes(&[0u8; NEST_HEADER_SIZE + NEST_FOOTER_SIZE]);
    assert!(matches!(
        res,
        Err(nest_format::NestError::MagicMismatch { .. })
    ));
}

#[test]
fn embeddings_size_mismatch_rejected() {
    // lBuild, then truncate the embeddings section by one float so its
    // size no longer matches n_embeddings * dim * 4. The section is
    // still present and its checksum is valid, so the reader has to
    // catch the inconsistency via validate_embeddings_layout.
    use nest_format::layout::SECTION_EMBEDDINGS;

    let mut bytes = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("hi", "doc", 0, 2, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();

    // lFind the embeddings entry's table slot; size is at offset 16..24
    // within the 32-byte SectionEntry.
    let mut hdr = NestHeader::default();
    hdr.as_bytes_mut()
        .copy_from_slice(&bytes[..NEST_HEADER_SIZE]);
    let n = hdr.section_table_count as usize;
    let mut emb_idx = None;
    for i in 0..n {
        let off = NEST_HEADER_SIZE + i * NEST_SECTION_ENTRY_SIZE;
        let id = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        if id == SECTION_EMBEDDINGS {
            emb_idx = Some(off);
            break;
        }
    }
    let entry_off = emb_idx.expect("embeddings section present");

    // lRead current size, drop 4 bytes, and recompute checksum over the
    // truncated slice so the reader's section checksum check passes
    // and the layout check is what fires.
    let mut size_buf = [0u8; 8];
    size_buf.copy_from_slice(&bytes[entry_off + 16..entry_off + 24]);
    let size = u64::from_le_bytes(size_buf);
    let new_size = size - 4;
    bytes[entry_off + 16..entry_off + 24].copy_from_slice(&new_size.to_le_bytes());

    let mut off_buf = [0u8; 8];
    off_buf.copy_from_slice(&bytes[entry_off + 8..entry_off + 16]);
    let data_off = u64::from_le_bytes(off_buf) as usize;
    let new_end = data_off + new_size as usize;
    use sha2::{Digest, Sha256};
    let new_hash = Sha256::digest(&bytes[data_off..new_end]);
    bytes[entry_off + 24..entry_off + 32].copy_from_slice(&new_hash[..8]);

    let body_end = bytes.len() - NEST_FOOTER_SIZE;
    let len = bytes.len();
    let footer_hash = NestFooter::compute_file_hash(&bytes[..body_end]);
    bytes[body_end + 8..len].copy_from_slice(&footer_hash);

    let res = NestView::from_bytes(&bytes);
    assert!(
        matches!(
            res,
            Err(nest_format::NestError::EmbeddingSizeMismatch { .. })
        ),
        "expected EmbeddingSizeMismatch, got {:?}",
        res.err()
    );
}

#[test]
fn invalid_magic_fails() {
    let mut data = vec![0u8; 200];
    data[0..4].copy_from_slice(b"BAD!");
    let res = NestView::from_bytes(&data);
    assert!(matches!(
        res,
        Err(nest_format::NestError::MagicMismatch { .. })
    ));
}

#[test]
fn invalid_version_fails() {
    let path = tmp_path("test_version.nest");
    let _ = std::fs::remove_file(&path);

    NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("a", "b", 0, 1, vec![1.0, 0.0, 0.0, 0.0]))
        .write_to_path(&path)
        .unwrap();

    let mut data = std::fs::read(&path).unwrap();
    data[6] = 255; // version_minor
    let res = NestView::from_bytes(&data);
    assert!(matches!(
        res,
        Err(nest_format::NestError::UnsupportedVersion(1, 255))
    ));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn nan_in_embedding_fails() {
    // lBuilder rejects NaN up front.
    let bad = chunk("a", "b", 0, 1, vec![f32::NAN, 0.0, 0.0, 0.0]);
    let res = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(bad)
        .build_bytes();
    assert!(matches!(
        res,
        Err(nest_format::NestError::InvalidEmbeddingValue)
    ));
}

#[test]
fn inf_in_embedding_fails() {
    let bad = chunk("a", "b", 0, 1, vec![f32::INFINITY, 0.0, 0.0, 0.0]);
    let res = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(bad)
        .build_bytes();
    assert!(matches!(
        res,
        Err(nest_format::NestError::InvalidEmbeddingValue)
    ));
}

#[test]
fn corrupt_section_hash_fails() {
    let path = tmp_path("test_corrupt_section.nest");
    let _ = std::fs::remove_file(&path);

    NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("a", "b", 0, 1, vec![1.0, 0.0, 0.0, 0.0]))
        .write_to_path(&path)
        .unwrap();

    let mut data = std::fs::read(&path).unwrap();
    let mut hdr = NestHeader::default();
    hdr.as_bytes_mut()
        .copy_from_slice(&data[..NEST_HEADER_SIZE]);
    let off = (hdr.manifest_offset + hdr.manifest_size) as usize;
    data[off] ^= 0xFF;
    let res = NestView::from_bytes(&data);
    assert!(
        res.is_err(),
        "corrupted section data must fail integrity check"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn corrupt_footer_fails() {
    let path = tmp_path("test_corrupt_footer.nest");
    let _ = std::fs::remove_file(&path);

    NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("a", "b", 0, 1, vec![1.0, 0.0, 0.0, 0.0]))
        .write_to_path(&path)
        .unwrap();

    let mut data = std::fs::read(&path).unwrap();
    let off = data.len() - NEST_FOOTER_SIZE + 8;
    data[off] ^= 0xFF;
    let res = NestView::from_bytes(&data);
    assert!(matches!(
        res,
        Err(nest_format::NestError::FooterHashMismatch)
    ));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_required_section_fails() {
    // lBuild valid bytes, then surgically rewrite the section table to drop
    // lSECTION_CHUNK_IDS. The file will still pass header/checksum/footer
    // validation; the reader must catch the missing section by name.
    use nest_format::sections::{
        OriginalSpan, SearchContract, encode_chunk_ids, encode_chunks_canonical,
        encode_chunks_original_spans, encode_provenance, encode_search_contract,
    };

    let m = manifest(4, 1);
    let raw_emb: Vec<u8> = [1.0f32, 0.0, 0.0, 0.0]
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();

    // lBuild manually missing chunk_ids (SECTION_CHUNK_IDS).
    let manifest_json = m.to_canonical_json().unwrap();
    let mut sections: Vec<(u32, Vec<u8>)> = vec![
        (
            SECTION_CHUNKS_CANONICAL,
            encode_chunks_canonical(&["alpha".into()]).unwrap(),
        ),
        (
            SECTION_CHUNKS_ORIGINAL_SPANS,
            encode_chunks_original_spans(&[OriginalSpan {
                source_uri: "x".into(),
                byte_start: 0,
                byte_end: 5,
            }])
            .unwrap(),
        ),
        (SECTION_EMBEDDINGS, raw_emb),
        (
            SECTION_PROVENANCE,
            encode_provenance(&serde_json::json!({})).unwrap(),
        ),
        (
            SECTION_SEARCH_CONTRACT,
            encode_search_contract(&SearchContract {
                metric: "ip".into(),
                score_type: "cosine".into(),
                normalize: "l2".into(),
                index_type: "exact".into(),
                rerank_policy: "none".into(),
            })
            .unwrap(),
        ),
    ];
    sections.sort_by_key(|s| s.0);

    let count = sections.len() as u64;
    let header_size = NEST_HEADER_SIZE as u64;
    let manifest_offset = header_size + count * NEST_SECTION_ENTRY_SIZE as u64;
    let manifest_size = manifest_json.len() as u64;
    let mut current = align_up(manifest_offset + manifest_size, SECTION_ALIGNMENT);
    let mut entries = Vec::new();
    for (id, data) in &sections {
        entries.push(SectionEntry::new(*id, current, data.len() as u64));
        current = align_up(current + data.len() as u64, SECTION_ALIGNMENT);
    }
    let last_end = entries
        .last()
        .map(|e| e.offset + e.size)
        .unwrap_or(manifest_offset + manifest_size);
    let data_end = last_end as usize;
    let file_size = data_end + NEST_FOOTER_SIZE;
    let mut buf = vec![0u8; file_size];

    let mut header = NestHeader::new(
        m.embedding_dim,
        m.n_chunks,
        1,
        file_size as u64,
        header_size,
        count,
        manifest_offset,
        manifest_size,
    );
    buf[..NEST_HEADER_SIZE].copy_from_slice(header.as_bytes());
    for (i, e) in entries.iter().enumerate() {
        let off = NEST_HEADER_SIZE + i * NEST_SECTION_ENTRY_SIZE;
        buf[off..off + NEST_SECTION_ENTRY_SIZE].copy_from_slice(e.as_bytes());
    }
    buf[manifest_offset as usize..manifest_offset as usize + manifest_json.len()]
        .copy_from_slice(&manifest_json);
    use sha2::{Digest, Sha256};
    for (i, (_, data)) in sections.iter().enumerate() {
        let off = entries[i].offset as usize;
        buf[off..off + data.len()].copy_from_slice(data);
        let h = Sha256::digest(data);
        let entry_off = NEST_HEADER_SIZE + i * NEST_SECTION_ENTRY_SIZE;
        buf[entry_off + 24..entry_off + 32].copy_from_slice(&h[..8]);
    }
    let footer_hash = NestFooter::compute_file_hash(&buf[..data_end]);
    buf[data_end..data_end + 8].copy_from_slice(&NEST_FOOTER_SIZE.to_le_bytes());
    buf[data_end + 8..file_size].copy_from_slice(&footer_hash);
    header.compute_checksum();
    buf[..NEST_HEADER_SIZE].copy_from_slice(header.as_bytes());

    // chunk_ids missing — reader rejects.
    let res = NestView::from_bytes(&buf).err();
    let kind = res.as_ref().map(|e| format!("{}", e)).unwrap_or_default();
    assert!(
        matches!(
            res,
            Some(nest_format::NestError::MissingRequiredSection("chunk_ids"))
        ),
        "got {}",
        kind
    );

    // lSanity-check the encoder is reachable (otherwise the test would
    // silently degrade if encode_chunk_ids was removed).
    let _ = encode_chunk_ids(&["sha256:demo".into()]).unwrap();
}

#[test]
fn citation_id_uses_content_hash_with_nest_scheme() {
    // lBuild, open via runtime, search, and check citation_id format.
    // (Runtime test lives in nest-runtime/tests, but we sanity-check the
    // content_hash format here.)
    let bytes = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("alpha", "doc.txt", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();
    let view = NestView::from_bytes(&bytes).unwrap();
    let content_hash = view.content_hash_hex().unwrap();
    assert!(content_hash.starts_with("sha256:"));
    let hex_part = content_hash.strip_prefix("sha256:").unwrap();
    assert_eq!(hex_part.len(), 64);
    assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn rejects_unsupported_dtype_in_manifest() {
    let mut m = manifest(4, 1);
    m.dtype = "bfloat16".into();
    let res = NestFileBuilder::new(m)
        .add_chunk(chunk("a", "b", 0, 1, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes();
    assert!(matches!(
        res,
        Err(nest_format::NestError::UnsupportedDType(_))
    ));
}

#[test]
fn rejects_unsupported_metric_in_manifest() {
    let mut m = manifest(4, 1);
    m.metric = "l2".into();
    let res = NestFileBuilder::new(m)
        .add_chunk(chunk("a", "b", 0, 1, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes();
    assert!(matches!(
        res,
        Err(nest_format::NestError::UnsupportedMetric(_))
    ));
}

#[test]
fn writer_aligns_every_section_to_64_bytes() {
    // lUse an embedding_dim that does not divide 16 (16 floats = 64 bytes)
    // so the embeddings section ends at an unaligned offset and we can
    // see padding being inserted before the next section.
    let bytes = NestFileBuilder::new(manifest(5, 3))
        .add_chunk(chunk("a", "doc", 0, 1, vec![1.0, 0.0, 0.0, 0.0, 0.0]))
        .add_chunk(chunk("b", "doc", 1, 2, vec![0.0, 1.0, 0.0, 0.0, 0.0]))
        .add_chunk(chunk("c", "doc", 2, 3, vec![0.0, 0.0, 1.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();

    let view = NestView::from_bytes(&bytes).unwrap();
    for entry in &view.section_table {
        assert_eq!(
            entry.offset % SECTION_ALIGNMENT,
            0,
            "section {} not aligned: offset={}",
            entry.section_id,
            entry.offset
        );
        assert_eq!(
            entry.encoding, SECTION_ENCODING_RAW,
            "section {} encoding={}",
            entry.section_id, entry.encoding
        );
    }
}

#[test]
fn padding_zeros_are_not_part_of_section_hash() {
    // lSection sizes are payload-only. Flipping a byte inside the
    // padding (between two sections) does not invalidate any section
    // checksum but does invalidate the file hash via the footer.
    let mut bytes = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("hello", "doc", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();
    let view = NestView::from_bytes(&bytes).unwrap();

    // lFind a section that has some padding before the next one.
    let mut entries = view.section_table.clone();
    entries.sort_by_key(|e| e.offset);
    let mut padding_byte: Option<usize> = None;
    for w in entries.windows(2) {
        let pad_start = (w[0].offset + w[0].size) as usize;
        let pad_end = w[1].offset as usize;
        if pad_end > pad_start {
            padding_byte = Some(pad_start);
            break;
        }
    }
    let off = padding_byte.expect("expected padding between sections");
    bytes[off] ^= 0xFF;

    // lThe footer hash covers everything pre-footer including the
    // padding, so corruption is caught — but as a `FooterHashMismatch`,
    // not a section checksum mismatch.
    let res = NestView::from_bytes(&bytes);
    assert!(matches!(
        res,
        Err(nest_format::NestError::FooterHashMismatch)
    ));
}

#[test]
fn reader_rejects_unsupported_section_encoding() {
    let mut bytes = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("hello", "doc", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();
    // lSectionEntry layout: section_id(4) | encoding(4) | offset(8) | size(8) | checksum(8)
    // lFirst section table entry starts at NEST_HEADER_SIZE = 128.
    // lPatch encoding (bytes 4..8 of the first entry) to 99 (zstd-ish).
    let entry_off = NEST_HEADER_SIZE;
    bytes[entry_off + 4..entry_off + 8].copy_from_slice(&99u32.to_le_bytes());
    // lRecompute footer hash since we changed the table.
    let body_end = bytes.len() - NEST_FOOTER_SIZE;
    let len = bytes.len();
    let footer_hash = NestFooter::compute_file_hash(&bytes[..body_end]);
    bytes[body_end + 8..len].copy_from_slice(&footer_hash);

    let res = NestView::from_bytes(&bytes);
    assert!(
        matches!(
            res,
            Err(nest_format::NestError::UnsupportedSectionEncoding { encoding: 99, .. })
        ),
        "expected UnsupportedSectionEncoding, got {:?}",
        res.err()
    );
}

#[test]
fn reader_rejects_future_format_version() {
    // lBuild a valid v1 file, then patch the manifest's format_version
    // field to NEST_FORMAT_VERSION+1, recompute footer hash, and
    // confirm the reader rejects with UnsupportedFormatVersion.
    let mut bytes = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("hi", "doc", 0, 2, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();

    // lLocate the manifest in the file via the header.
    let mut hdr = NestHeader::default();
    hdr.as_bytes_mut()
        .copy_from_slice(&bytes[..NEST_HEADER_SIZE]);
    let m_off = hdr.manifest_offset as usize;
    let m_size = hdr.manifest_size as usize;

    // lDecode -> bump format_version -> re-encode canonically.
    let mut manifest_obj: nest_format::manifest::Manifest =
        serde_json::from_slice(&bytes[m_off..m_off + m_size]).unwrap();
    manifest_obj.format_version = NEST_FORMAT_VERSION + 1;
    let new_json = manifest_obj.to_canonical_json().unwrap();

    // lWe need the new manifest to fit in the existing slot. If it's
    // shorter, pad; if longer, this path is not exercisable without
    // re-laying out the file. The default manifest size is stable for
    // this fixture, so a single-digit bump fits in place.
    assert!(
        new_json.len() <= m_size,
        "patched manifest grew ({} > {}); test setup needs a fresh build",
        new_json.len(),
        m_size
    );
    bytes[m_off..m_off + new_json.len()].copy_from_slice(&new_json);
    // lPad the rest with spaces so JSON parser still succeeds (whitespace
    // is allowed before EOF; serde_json::from_slice tolerates it).
    for b in &mut bytes[m_off + new_json.len()..m_off + m_size] {
        *b = b' ';
    }

    // lRefresh section checksums for sections that may have shifted? They
    // didn't shift — only manifest bytes changed. But footer hash covers
    // the whole pre-footer payload, so recompute.
    let body_end = bytes.len() - NEST_FOOTER_SIZE;
    let len = bytes.len();
    let footer_hash = NestFooter::compute_file_hash(&bytes[..body_end]);
    bytes[body_end + 8..len].copy_from_slice(&footer_hash);

    let res = NestView::from_bytes(&bytes);
    assert!(
        matches!(
            res,
            Err(nest_format::NestError::UnsupportedFormatVersion(_))
        ),
        "expected UnsupportedFormatVersion, got {:?}",
        res.err()
    );
}

#[test]
fn reader_rejects_misaligned_section() {
    let mut bytes = NestFileBuilder::new(manifest(4, 1))
        .add_chunk(chunk("hello", "doc", 0, 5, vec![1.0, 0.0, 0.0, 0.0]))
        .build_bytes()
        .unwrap();
    // lSectionEntry: section_id(4) | encoding(4) | offset(8) | size(8) | checksum(8)
    // lPatch the first entry's offset by +1 so it is no longer 64-aligned.
    let entry_off = NEST_HEADER_SIZE;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[entry_off + 8..entry_off + 16]);
    let mut off = u64::from_le_bytes(buf);
    off += 1;
    bytes[entry_off + 8..entry_off + 16].copy_from_slice(&off.to_le_bytes());
    let body_end = bytes.len() - NEST_FOOTER_SIZE;
    let len = bytes.len();
    let footer_hash = NestFooter::compute_file_hash(&bytes[..body_end]);
    bytes[body_end + 8..len].copy_from_slice(&footer_hash);

    let res = NestView::from_bytes(&bytes);
    assert!(
        matches!(res, Err(nest_format::NestError::SectionMisaligned { .. })),
        "expected SectionMisaligned, got {:?}",
        res.err()
    );
}
