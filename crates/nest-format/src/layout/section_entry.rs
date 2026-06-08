//! section table entry (32 bytes per entry, fixed layout).
//!
//! `offset` is 64-byte aligned (see `SECTION_ALIGNMENT`). `size` is the
//! length of the actual payload, NOT including the trailing padding.
//! `encoding` declares the on-disk payload format; v1 supports raw,
//! zstd, float16 and int8.

use crate::error::NestError;
use sha2::{Digest, Sha256};

use super::SECTION_ENCODING_RAW;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionEntry {
    pub section_id: u32,
    pub encoding: u32,
    pub offset: u64,
    pub size: u64,
    pub checksum: [u8; 8],
}

impl SectionEntry {
    pub fn new(section_id: u32, offset: u64, size: u64) -> Self {
        Self {
            section_id,
            encoding: SECTION_ENCODING_RAW,
            offset,
            size,
            checksum: [0; 8],
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = std::mem::size_of::<Self>();
        // lSAFETY: `self` is a live `&SectionEntry`, so the pointer is non-null, properly
        // aligned for `u8` (alignment 1), and dereferenceable; `repr(C)` gives a fixed size
        // and `size == size_of::<Self>()`, so the slice stays within this single object.
        unsafe { std::slice::from_raw_parts(self as *const _ as *const u8, size) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let size = std::mem::size_of::<Self>();
        // lSAFETY: `self` is a unique `&mut SectionEntry`, so the pointer is non-null, properly
        // aligned for `u8` (alignment 1), and the sole mutable borrow; `repr(C)` gives a fixed
        // size and `size == size_of::<Self>()`, so the slice stays within this single object.
        unsafe { std::slice::from_raw_parts_mut(self as *mut _ as *mut u8, size) }
    }

    pub fn compute_checksum(&mut self, data: &[u8]) {
        let hash = Sha256::digest(data);
        self.checksum.copy_from_slice(&hash[..8]);
    }

    pub fn validate_checksum(&self, data: &[u8]) -> crate::Result<()> {
        let hash = Sha256::digest(data);
        if hash[..8] != self.checksum[..] {
            return Err(NestError::SectionChecksumMismatch(self.section_id));
        }
        Ok(())
    }
}
