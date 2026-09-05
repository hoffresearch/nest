//! Multimodal space open logic (0x15 space_table + the 0x20-0x2F vector
//! bands), carved out of `mmap_file.rs` so that file stays under the
//! 300-line crate guard. Opens only behind the additive
//! `supports_multimodal` capability, mirroring how the blob pair opens
//! behind `blobs_present`. The bands are fixed-stride slabs read straight
//! off the mmap by the per-space exact search.

use nest_format::layout::{SECTION_SPACE_EMBEDDINGS_BASE, SECTION_SPACE_TABLE};
use nest_format::reader::NestView;
use nest_format::sections::{SpaceEntry, decode_space_table};

use crate::error::RuntimeError;

/// one opened space: its directory entry plus the byte range of its band
/// slab inside the mmap (resolved once at open time).
#[derive(Clone, Debug)]
pub(crate) struct OpenSpace {
    pub entry: SpaceEntry,
    pub offset: usize,
    pub size: usize,
}

/// Open the space_table and resolve each listed space's band range.
/// `None` when the capability or the section is absent. the band's
/// presence and exact size were already validated by the reader
/// (`validate_space_bands`), so a missing band here is a typed error,
/// never a silent skip.
pub(crate) fn open_space_sections(view: &NestView) -> Result<Option<Vec<OpenSpace>>, RuntimeError> {
    let multimodal = view
        .manifest
        .capabilities_ext
        .as_ref()
        .and_then(|e| e.supports_multimodal)
        .unwrap_or(false);
    if !multimodal || view.entry(SECTION_SPACE_TABLE).is_err() {
        return Ok(None);
    }
    let entries = decode_space_table(&view.decoded_section(SECTION_SPACE_TABLE)?)?;
    let mut spaces = Vec::with_capacity(entries.len());
    for entry in entries {
        let band_id = SECTION_SPACE_EMBEDDINGS_BASE + entry.space_index as u32;
        let band = view.entry(band_id)?;
        spaces.push(OpenSpace {
            entry,
            offset: band.offset as usize,
            size: band.size as usize,
        });
    }
    Ok(Some(spaces))
}
