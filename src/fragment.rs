//! Fragments: versioned, identified, integrity-protected pieces of a DNA stream.
//!
//! A DNA stream (sequence of bases produced by the codec) is split into
//! fixed-size fragments. Each fragment carries:
//!   * a logical fragment ID and position within the stream,
//!   * a payload length and the DNA payload,
//!   * a SHA-256 checksum of the payload for integrity detection,
//!   * a fragment kind (data or parity) and format version.
//!
//! Fragment integrity is detected here (checksum, length, magic, version).
//! Correction is performed by the Reed-Solomon layer over fixed-size blocks
//! ([`crate::ecc`]).

use crate::codec::Base;
use sha2::{Digest, Sha256};

/// The on-disk format version. Increment when the binary layout changes.
pub const FRAGMENT_FORMAT_VERSION: u16 = 1;
/// Magic bytes identifying a DNAForge fragment block.
pub const FRAGMENT_MAGIC: &[u8; 4] = b"DNAF";

/// Size of the fixed fragment header, in bytes.
pub const HEADER_SIZE: usize = 62;
/// Flags bit: block is a parity fragment (not a data fragment).
pub const FLAG_PARITY: u32 = 1;

/// Kind of fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FragmentKind {
    Data,
    Parity,
}

impl FragmentKind {
    pub fn flag_bits(self) -> u32 {
        match self {
            FragmentKind::Data => 0,
            FragmentKind::Parity => FLAG_PARITY,
        }
    }
}

/// Metadata describing a fragment's position and integrity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FragmentMeta {
    /// Logical fragment ID (unique within an archive).
    pub id: u64,
    /// Kind: data or parity.
    pub kind: FragmentKind,
    /// Index of this fragment among data fragments only (data fragments).
    /// Meaningless for parity.
    pub data_index: Option<u32>,
    /// Total number of data fragments.
    pub total_data: u32,
    /// Offset of this fragment's payload within the full DNA stream, in bases.
    pub offset_bases: u64,
    /// Number of DNA bases in the payload (unpadded).
    pub payload_len: usize,
    /// SHA-256 of the payload bytes.
    pub payload_hash: [u8; 32],
    /// Format version.
    pub version: u16,
}

/// A fragment: metadata plus the raw payload bytes (DNA bases as `Base`
/// values, packed one per byte for simplicity in this baseline).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    pub meta: FragmentMeta,
    pub payload: Vec<Base>,
}

impl Fragment {
    pub fn new_data(
        id: u64,
        data_index: u32,
        total_data: u32,
        offset_bases: u64,
        payload: Vec<Base>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(payload_bytes(&payload));
        let payload_hash = hasher.finalize().into();
        Self {
            meta: FragmentMeta {
                id,
                kind: FragmentKind::Data,
                data_index: Some(data_index),
                total_data,
                offset_bases,
                payload_len: payload.len(),
                payload_hash,
                version: FRAGMENT_FORMAT_VERSION,
            },
            payload,
        }
    }

    pub fn new_parity(
        id: u64,
        total_data: u32,
        offset_bases: u64,
        payload: Vec<Base>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(payload_bytes(&payload));
        let payload_hash = hasher.finalize().into();
        Self {
            meta: FragmentMeta {
                id,
                kind: FragmentKind::Parity,
                data_index: None,
                total_data,
                offset_bases,
                payload_len: payload.len(),
                payload_hash,
                version: FRAGMENT_FORMAT_VERSION,
            },
            payload,
        }
    }

    pub fn is_data(&self) -> bool {
        self.meta.kind == FragmentKind::Data
    }

    pub fn is_parity(&self) -> bool {
        self.meta.kind == FragmentKind::Parity
    }
}

fn payload_bytes(bases: &[Base]) -> Vec<u8> {
    bases.iter().map(|b| b.0).collect()
}

/// Computes the padded block size for a fragment: header + payload padded to
/// a multiple of `pad_to` bytes (default 16) so blocks have uniform size for
/// the erasure coder, except the final data fragment may be shorter and is
/// padded with zeros.
pub fn padded_block_size(payload_len: usize) -> usize {
    HEADER_SIZE + payload_len.next_multiple_of(16)
}

/// Serialize a fragment into a fixed-size byte block: header followed by
/// payload (payload zero-padded to `block_payload_size`).
///
/// Block layout (all integers little-endian):
///   [0..4]   magic "DNAF"
///   [4..6]   version u16
///   [6..14]  fragment id u64
///   [14..18] total data fragments u32
///   [18..22] payload length u32 (bases, unpadded)
///   [22..26] payload offset in bases u32
///   [26..58] SHA-256 of payload (32 bytes)
///   [58..62] flags u32
///   [62..]   payload bytes (Base values 0..3), zero-padded
pub fn serialize_block(fragment: &Fragment, block_payload_size: usize) -> Vec<u8> {
    let mut block = vec![0u8; HEADER_SIZE + block_payload_size];
    block[0..4].copy_from_slice(FRAGMENT_MAGIC);
    block[4..6].copy_from_slice(&fragment.meta.version.to_le_bytes());
    block[6..14].copy_from_slice(&fragment.meta.id.to_le_bytes());
    block[14..18].copy_from_slice(&fragment.meta.total_data.to_le_bytes());
    block[18..22].copy_from_slice(&(fragment.meta.payload_len as u32).to_le_bytes());
    block[22..26].copy_from_slice(&(fragment.meta.offset_bases as u32).to_le_bytes());
    block[26..58].copy_from_slice(&fragment.meta.payload_hash);
    block[58..62].copy_from_slice(&fragment.meta.kind.flag_bits().to_le_bytes());
    let payload: Vec<u8> = fragment.payload.iter().map(|b| b.0).collect();
    let payload_end = 62 + fragment.payload.len();
    block[62..payload_end].copy_from_slice(&payload);
    // Remainder stays zero-padded.
    block
}

/// Deserialize a fixed-size byte block into a fragment. Validates magic,
/// version, and that the declared payload length fits the block.
pub fn deserialize_block(block: &[u8], block_payload_size: usize) -> Result<Fragment, FormatError> {
    if block.len() != HEADER_SIZE + block_payload_size {
        return Err(FormatError::BlockSizeMismatch {
            expected: HEADER_SIZE + block_payload_size,
            got: block.len(),
        });
    }
    if &block[0..4] != FRAGMENT_MAGIC {
        return Err(FormatError::BadMagic);
    }
    let version = u16::from_le_bytes(block[4..6].try_into().unwrap());
    if version != FRAGMENT_FORMAT_VERSION {
        return Err(FormatError::UnsupportedVersion {
            supported: FRAGMENT_FORMAT_VERSION,
            found: version,
        });
    }
    let id = u64::from_le_bytes(block[6..14].try_into().unwrap());
    let total_data = u32::from_le_bytes(block[14..18].try_into().unwrap());
    let payload_len = u32::from_le_bytes(block[18..22].try_into().unwrap()) as usize;
    let offset_bases = u32::from_le_bytes(block[22..26].try_into().unwrap()) as u64;
    let mut payload_hash = [0u8; 32];
    payload_hash.copy_from_slice(&block[26..58]);
    let flags = u32::from_le_bytes(block[58..62].try_into().unwrap());
    let kind = if flags & FLAG_PARITY != 0 {
        FragmentKind::Parity
    } else {
        FragmentKind::Data
    };
    if payload_len > block_payload_size {
        return Err(FormatError::PayloadTooLarge {
            payload_len,
            block_payload_size,
        });
    }
    let payload: Vec<Base> = block[62..62 + payload_len]
        .iter()
        .map(|&b| Base(b))
        .collect();
    let data_index = if kind == FragmentKind::Data {
        // data_index is not stored in the block layout; recovered from archive
        // ordering by the archive layer. Store as None here.
        None
    } else {
        None
    };
    let mut hasher = Sha256::new();
    hasher.update(payload_bytes(&payload));
    let actual_hash = hasher.finalize();
    if actual_hash.as_slice() != payload_hash {
        return Err(FormatError::PayloadHashMismatch);
    }
    Ok(Fragment {
        meta: FragmentMeta {
            id,
            kind,
            data_index,
            total_data,
            offset_bases,
            payload_len,
            payload_hash,
            version,
        },
        payload,
    })
}

/// Errors in fragment serialization / deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    BadMagic,
    UnsupportedVersion { supported: u16, found: u16 },
    BlockSizeMismatch { expected: usize, got: usize },
    PayloadTooLarge { payload_len: usize, block_payload_size: usize },
    PayloadHashMismatch,
    /// Underlying checksum/hash failure.
    Hash(String),
    Io(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::BadMagic => write!(f, "format: bad magic bytes (not a DNAForge fragment)"),
            FormatError::UnsupportedVersion { supported, found } => {
                write!(f, "format: unsupported version {found} (supported {supported})")
            }
            FormatError::BlockSizeMismatch { expected, got } => {
                write!(f, "format: block size mismatch: expected {expected}, got {got}")
            }
            FormatError::PayloadTooLarge { payload_len, block_payload_size } => {
                write!(
                    f,
                    "format: payload length {payload_len} exceeds block payload size {block_payload_size}"
                )
            }
            FormatError::PayloadHashMismatch => {
                write!(f, "format: payload SHA-256 mismatch (fragment corrupted)")
            }
            FormatError::Hash(msg) => write!(f, "format: hash error: {msg}"),
            FormatError::Io(msg) => write!(f, "format: I/O error: {msg}"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(e: std::io::Error) -> Self {
        FormatError::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> Vec<Base> {
        vec![Base(0), Base(1), Base(2), Base(3), Base(0), Base(1)]
    }

    #[test]
    fn serialize_deserialize_round_trip() {
        let frag = Fragment::new_data(7, 2, 10, 100, sample_payload());
        let block = serialize_block(&frag, 16);
        assert_eq!(block.len(), HEADER_SIZE + 16);
        let back = deserialize_block(&block, 16).unwrap();
        assert_eq!(back.meta.id, 7);
        assert_eq!(back.meta.kind, FragmentKind::Data);
        assert_eq!(back.payload, frag.payload);
    }

    #[test]
    fn parity_fragment_round_trip() {
        let frag = Fragment::new_parity(99, 10, 0, sample_payload());
        let block = serialize_block(&frag, 16);
        let back = deserialize_block(&block, 16).unwrap();
        assert!(back.is_parity());
        assert_eq!(back.meta.id, 99);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut block = serialize_block(&Fragment::new_data(1, 0, 1, 0, sample_payload()), 16);
        block[0] ^= 0xFF;
        let err = deserialize_block(&block, 16).unwrap_err();
        assert!(matches!(err, FormatError::BadMagic));
    }

    #[test]
    fn corrupted_payload_hash_rejected() {
        let frag = Fragment::new_data(1, 0, 1, 0, sample_payload());
        let mut block = serialize_block(&frag, 16);
        block[62] ^= 0x01; // flip a payload byte
        let err = deserialize_block(&block, 16).unwrap_err();
        assert!(matches!(err, FormatError::PayloadHashMismatch));
    }

    #[test]
    fn padding_zeros() {
        let frag = Fragment::new_data(1, 0, 1, 0, vec![Base(0); 3]);
        let block = serialize_block(&frag, 16);
        assert_eq!(&block[65..78], &vec![0u8; 13]);
    }
}
