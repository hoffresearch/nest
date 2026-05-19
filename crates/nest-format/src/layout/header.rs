//! fixed-size binary header (128 bytes). The header contains
//! everything a reader needs to discover the section table and the
//! manifest without reading the file body. The `header_checksum` is
//! the first 8 bytes of `SHA-256` of the header bytes with the
//! checksum field zeroed.

use crate::error::NestError;
use sha2::{Digest, Sha256};

use super::{NEST_MAGIC, NEST_VERSION_MAJOR, NEST_VERSION_MINOR};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NestHeader {
    pub magic: [u8; 4],
    pub version_major: u16,
    pub version_minor: u16,
    pub flags: u32,
    pub embedding_dim: u32,
    pub n_chunks: u64,
    pub n_embeddings: u64,
    pub file_size: u64,
    pub section_table_offset: u64,
    pub section_table_count: u64,
    pub manifest_offset: u64,
    pub manifest_size: u64,
    pub header_checksum: [u8; 8],
    pub reserved: [u8; 48],
}

impl NestHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        embedding_dim: u32,
        n_chunks: u64,
        n_embeddings: u64,
        file_size: u64,
        section_table_offset: u64,
        section_table_count: u64,
        manifest_offset: u64,
        manifest_size: u64,
    ) -> Self {
        let mut h = Self {
            magic: *NEST_MAGIC,
            version_major: NEST_VERSION_MAJOR,
            version_minor: NEST_VERSION_MINOR,
            flags: 0,
            embedding_dim,
            n_chunks,
            n_embeddings,
            file_size,
            section_table_offset,
            section_table_count,
            manifest_offset,
            manifest_size,
            header_checksum: [0; 8],
            reserved: [0; 48],
        };
        h.compute_checksum();
        h
    }

    pub fn compute_checksum(&mut self) {
        let bytes = self.as_bytes_without_checksum();
        let hash = Sha256::digest(&bytes);
        self.header_checksum.copy_from_slice(&hash[..8]);
    }

    pub fn validate_checksum(&self) -> crate::Result<()> {
        let mut tmp = *self;
        tmp.header_checksum = [0; 8];
        let bytes = tmp.as_bytes_without_checksum();
        let hash = Sha256::digest(&bytes);
        if hash[..8] != self.header_checksum[..] {
            return Err(NestError::InvalidHeaderChecksum);
        }
        Ok(())
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = std::mem::size_of::<Self>();
        unsafe { std::slice::from_raw_parts(self as *const _ as *const u8, size) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let size = std::mem::size_of::<Self>();
        unsafe { std::slice::from_raw_parts_mut(self as *mut _ as *mut u8, size) }
    }

    fn as_bytes_without_checksum(&self) -> Vec<u8> {
        let bytes = self.as_bytes();
        let mut v = bytes[..72].to_vec();
        v.extend_from_slice(&bytes[80..]);
        v
    }
}

impl Default for NestHeader {
    fn default() -> Self {
        Self::new(0, 0, 0, 128, 128, 0, 128, 0)
    }
}
