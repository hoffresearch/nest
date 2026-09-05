//! Blob section open logic (0x14 blob_refs + 0x16 blob_span_overlay),
//! carved out of `mmap_file.rs` so that file stays under the 300-line
//! crate guard. Both sections are OPTIONAL and content_hash-excluded, and
//! open only behind the additive `blobs_present` capability, mirroring how
//! the graph (0x0C) opens behind `graph_present`.

use nest_format::NestError;
use nest_format::layout::{SECTION_BLOB_REFS, SECTION_BLOB_SPAN_OVERLAY};
use nest_format::reader::NestView;
use nest_format::sections::{
    BLOB_REF_NONE, BlobRefRecord, OriginalSpan, decode_blob_refs, decode_blob_span_overlay,
};

use crate::error::RuntimeError;

/// Open the blob pair. Returns the decoded 0x14 table (`None` when the
/// capability or section is absent). When the 0x16 overlay is present,
/// per-chunk spans that point into a blob REPLACE the decoded 0x03 spans
/// in place (for a media corpus those carry ordinal placeholders), so
/// cite/retrieve report the real blob-relative byte range against the
/// blob's uri; BLOB_REF_NONE entries keep the 0x03 span (legacy/text
/// chunks). a dangling blob_ref_index is a typed format error, never a
/// silent fallback.
pub(crate) fn open_blob_sections(
    view: &NestView,
    spans: &mut [OriginalSpan],
) -> Result<Option<Vec<BlobRefRecord>>, RuntimeError> {
    let blobs_present = view
        .manifest
        .capabilities_ext
        .as_ref()
        .and_then(|e| e.blobs_present)
        .unwrap_or(false);
    if !blobs_present {
        return Ok(None);
    }
    let has = |id: u32| view.section_table.iter().any(|e| e.section_id == id);
    let blob_refs = if has(SECTION_BLOB_REFS) {
        let bytes = view.decoded_section(SECTION_BLOB_REFS)?;
        Some(decode_blob_refs(&bytes)?)
    } else {
        None
    };
    if has(SECTION_BLOB_SPAN_OVERLAY) {
        let bytes = view.decoded_section(SECTION_BLOB_SPAN_OVERLAY)?;
        let overlay = decode_blob_span_overlay(&bytes)?;
        for (i, entry) in overlay.iter().enumerate().take(spans.len()) {
            if entry.blob_ref_index == BLOB_REF_NONE {
                continue;
            }
            let uri = blob_refs
                .as_ref()
                .and_then(|refs| refs.get(entry.blob_ref_index as usize))
                .map(|r| r.original_uri.clone())
                .ok_or_else(|| {
                    RuntimeError::Format(NestError::MalformedSectionPayload {
                        section_id: SECTION_BLOB_SPAN_OVERLAY,
                        reason: format!(
                            "overlay entry {} references missing blob_ref {}",
                            i, entry.blob_ref_index
                        ),
                    })
                })?;
            spans[i] = OriginalSpan {
                source_uri: uri,
                byte_start: entry.byte_start,
                byte_end: entry.byte_end,
            };
        }
    }
    Ok(blob_refs)
}
