//! footer (40 bytes). Sits at `file_size - 40` and carries the
//! `file_hash` (full SHA-256 over the file body) plus the footer's own
//! size for forward compat.

use crate::error::NestError;
use sha2::{Digest, Sha256};

use super::NEST_FOOTER_SIZE;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NestFooter {
    pub footer_size: u64,
    pub file_hash: [u8; 32],
}

impl NestFooter {
    pub fn new(file_hash: [u8; 32]) -> Self {
        Self {
            footer_size: NEST_FOOTER_SIZE as u64,
            file_hash,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = std::mem::size_of::<Self>();
        // lSAFETY: `self` is a valid `&Self`, so the pointer is non-null, aligned for u8 (align 1),
        // and `size == size_of::<Self>()` bytes are owned and initialized; the returned slice borrows
        // `self` immutably so it stays in bounds and valid for the lifetime of the borrow.
        unsafe { std::slice::from_raw_parts(self as *const _ as *const u8, size) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let size = std::mem::size_of::<Self>();
        // lSAFETY: `self` is a valid `&mut Self`, so the pointer is non-null, aligned for u8 (align 1),
        // and `size == size_of::<Self>()` bytes are owned and initialized; the exclusive `&mut self`
        // borrow guarantees no aliasing, keeps the slice in bounds, and valid for the borrow's lifetime.
        unsafe { std::slice::from_raw_parts_mut(self as *mut _ as *mut u8, size) }
    }

    pub fn compute_file_hash(data_without_footer: &[u8]) -> [u8; 32] {
        let hash = Sha256::digest(data_without_footer);
        let mut out = [0u8; 32];
        out.copy_from_slice(&hash[..]);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() < std::mem::size_of::<Self>() {
            return Err(NestError::UnexpectedEof);
        }
        let mut footer = NestFooter::new([0; 32]);
        footer.as_bytes_mut().copy_from_slice(bytes);
        Ok(footer)
    }
}
