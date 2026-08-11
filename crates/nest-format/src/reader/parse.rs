//! `NestView::from_bytes` — header, section table, manifest, footer
//! parsing. Validation hooks are in `super::validate` and run after the
//! basic structure has been recognized.

use super::NestView;
use crate::error::NestError;
use crate::layout::{
    NEST_FOOTER_SIZE, NEST_HEADER_SIZE, NEST_MAGIC, NEST_SECTION_ENTRY_SIZE, NEST_VERSION_MAJOR,
    NEST_VERSION_MINOR, NestFooter, NestHeader, SECTION_ALIGNMENT, SectionEntry,
};
use crate::manifest::Manifest;

impl<'a> NestView<'a> {
    pub fn from_bytes(data: &'a [u8]) -> crate::Result<Self> {
        if data.len() < NEST_HEADER_SIZE + NEST_FOOTER_SIZE {
            return Err(NestError::FileTruncated);
        }

        // ---- header ----
        let mut header = NestHeader::default();
        header
            .as_bytes_mut()
            .copy_from_slice(&data[..NEST_HEADER_SIZE]);

        if &header.magic != NEST_MAGIC {
            return Err(NestError::MagicMismatch {
                expected: *NEST_MAGIC,
                got: header.magic,
            });
        }
        if header.version_major != NEST_VERSION_MAJOR || header.version_minor > NEST_VERSION_MINOR {
            return Err(NestError::UnsupportedVersion(
                header.version_major,
                header.version_minor,
            ));
        }
        header.validate_checksum()?;

        if header.file_size as usize != data.len() {
            return Err(NestError::FileSizeMismatch {
                expected: header.file_size,
                got: data.len() as u64,
            });
        }

        // ---- section table ----
        let section_table_offset = header.section_table_offset as usize;
        let section_table_count = header.section_table_count as usize;
        let section_table_end = section_table_offset
            .checked_add(
                section_table_count
                    .checked_mul(NEST_SECTION_ENTRY_SIZE)
                    .ok_or(NestError::SectionOffsetOutOfBounds {
                        section_id: 0,
                        offset: 0,
                    })?,
            )
            .ok_or(NestError::SectionOffsetOutOfBounds {
                section_id: 0,
                offset: 0,
            })?;
        if section_table_end > data.len().saturating_sub(NEST_FOOTER_SIZE) {
            return Err(NestError::FileTruncated);
        }

        let mut section_table = Vec::with_capacity(section_table_count);
        for i in 0..section_table_count {
            let off = section_table_offset + i * NEST_SECTION_ENTRY_SIZE;
            let mut entry = SectionEntry::new(0, 0, 0);
            entry
                .as_bytes_mut()
                .copy_from_slice(&data[off..off + NEST_SECTION_ENTRY_SIZE]);
            section_table.push(entry);
        }

        // ---- section bounds + alignment + encoding + checksums ----
        let body_end = data.len() - NEST_FOOTER_SIZE;
        for entry in &section_table {
            super::validate::validate_encoding_for_section(entry.section_id, entry.encoding)?;
            if entry.offset % SECTION_ALIGNMENT != 0 {
                return Err(NestError::SectionMisaligned {
                    section_id: entry.section_id,
                    offset: entry.offset,
                    alignment: SECTION_ALIGNMENT,
                });
            }
            let start = entry.offset as usize;
            let end = start.checked_add(entry.size as usize).ok_or(
                NestError::SectionOffsetOutOfBounds {
                    section_id: entry.section_id,
                    offset: entry.offset,
                },
            )?;
            if end > body_end {
                return Err(NestError::SectionOffsetOutOfBounds {
                    section_id: entry.section_id,
                    offset: entry.offset,
                });
            }
            entry.validate_checksum(&data[start..end])?;
        }

        // ---- manifest ----
        let manifest_offset = header.manifest_offset as usize;
        let manifest_size = header.manifest_size as usize;
        if manifest_offset
            .checked_add(manifest_size)
            .map(|e| e > body_end)
            .unwrap_or(true)
        {
            return Err(NestError::FileTruncated);
        }
        let manifest_data = &data[manifest_offset..manifest_offset + manifest_size];
        let manifest: Manifest = serde_json::from_slice(manifest_data)?;
        manifest.validate()?;

        // ---- footer hash ----
        let footer = NestFooter::from_bytes(&data[body_end..])?;
        let computed = NestFooter::compute_file_hash(&data[..body_end]);
        if computed != footer.file_hash {
            return Err(NestError::FooterHashMismatch);
        }

        // ---- coherence ----
        if manifest.embedding_dim != header.embedding_dim {
            return Err(NestError::ManifestInvalid(
                "manifest embedding_dim disagrees with header".into(),
            ));
        }
        if manifest.n_chunks != header.n_chunks {
            return Err(NestError::ManifestInvalid(
                "manifest n_chunks disagrees with header".into(),
            ));
        }

        let view = Self {
            data,
            header,
            section_table,
            manifest,
            footer,
        };

        // ---- semantic validation (delegates to super::validate) ----
        view.check_required_sections()?;
        view.validate_embeddings_layout()?;
        view.validate_space_bands()?;
        view.validate_search_contract()?;

        Ok(view)
    }
}
