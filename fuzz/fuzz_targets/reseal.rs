//! Recompute the header checksum, every in-bounds section checksum and the
//! footer hash so a fuzzer-mutated container passes the integrity layer and
//! reaches the decoders. Mirrors `tests/common/mutation.rs::reseal`.

use nest_format::layout::{
    NEST_FOOTER_SIZE, NEST_HEADER_SIZE, NEST_MAGIC, NEST_SECTION_ENTRY_SIZE, NestFooter,
    NestHeader, SectionEntry,
};

pub fn reseal(b: &mut [u8]) {
    if b.len() < NEST_HEADER_SIZE + NEST_FOOTER_SIZE || &b[..4] != NEST_MAGIC {
        return;
    }
    let mut header = NestHeader::default();
    header
        .as_bytes_mut()
        .copy_from_slice(&b[..NEST_HEADER_SIZE]);
    header.file_size = b.len() as u64;
    header.compute_checksum();
    b[..NEST_HEADER_SIZE].copy_from_slice(header.as_bytes());

    let body_end = b.len() - NEST_FOOTER_SIZE;
    let table_off = header.section_table_offset as usize;
    for i in 0..header.section_table_count as usize {
        let Some(at) = table_off.checked_add(i * NEST_SECTION_ENTRY_SIZE) else {
            break;
        };
        if at.checked_add(NEST_SECTION_ENTRY_SIZE).is_none_or(|end| end > body_end) {
            break;
        }
        let mut e = SectionEntry::new(0, 0, 0);
        e.as_bytes_mut()
            .copy_from_slice(&b[at..at + NEST_SECTION_ENTRY_SIZE]);
        let start = e.offset as usize;
        let Some(end) = start.checked_add(e.size as usize) else {
            continue;
        };
        if end > body_end {
            continue;
        }
        let payload = b[start..end].to_vec();
        e.compute_checksum(&payload);
        b[at..at + NEST_SECTION_ENTRY_SIZE].copy_from_slice(e.as_bytes());
    }
    let hash = NestFooter::compute_file_hash(&b[..body_end]);
    b[body_end..].copy_from_slice(NestFooter::new(hash).as_bytes());
}

/// First input byte decides whether to reseal; the rest is the file.
pub fn split(data: &[u8]) -> Option<Vec<u8>> {
    let (&flag, rest) = data.split_first()?;
    let mut bytes = rest.to_vec();
    if flag & 1 == 1 {
        reseal(&mut bytes);
    }
    Some(bytes)
}
