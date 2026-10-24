//! Recovery engine: validation, Reed-Solomon erasure correction, and
//! reconstruction.
//!
//! Pipeline: parse fragment blocks -> authenticate each with SHA-256 -> classify
//! missing/corrupt fragments as erasures -> reconstruct erasures with the
//! Reed-Solomon code (crate::ecc) -> reassemble the DNA stream in offset order
//! -> decode to bytes -> verify the original SHA-256.
//!
//! Detection (checksum/hash/length) is strictly separated from correction
//! (Reed-Solomon). A checksum alone only detects; the erasure code is what
//! corrects, and only up to the configured redundancy.

use crate::codec::{decode_bases, CodecError};
use crate::constraints::{validate, ConstraintConfig};
use crate::ecc::{ReedSolomon, ReedSolomonConfig, ReedSolomonError};
use crate::fragment::{deserialize_block, serialize_block, Fragment, HEADER_SIZE};
use sha2::{Digest, Sha256};

/// Timing for recovery stages (milliseconds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageTiming {
    pub parse_ms: u128,
    pub ecc_ms: u128,
    pub reassemble_ms: u128,
    pub decode_ms: u128,
    pub verify_ms: u128,
    pub total_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecoveryConfig {
    pub ecc_k: usize,
    pub ecc_r: usize,
    pub block_payload_size: usize,
    pub validate_constraints: bool,
    pub constraint_config: ConstraintConfig,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size: 1024,
            validate_constraints: false,
            constraint_config: ConstraintConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryReport {
    pub blocks_total: usize,
    pub blocks_parsed: usize,
    pub blocks_corrupted: usize,
    pub blocks_missing: usize,
    pub duplicates_removed: usize,
    pub data_erasures: usize,
    pub data_recovered: usize,
    pub unrecovered: bool,
    pub recovered_bytes: Option<Vec<u8>>,
    pub recovered_hash: Option<[u8; 32]>,
    pub original_hash: Option<[u8; 32]>,
    pub exact_match: Option<bool>,
    pub constraint_violations: Vec<crate::constraints::ConstraintViolation>,
    pub dna_bases: usize,
    pub stage_timing: StageTiming,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    NotEnoughDataBlocks { have: usize, need: usize },
    TooManyErasures { erasures: usize, tolerance: usize },
    BlockCountMismatch { expected: usize, got: usize },
    BlockSizeMismatch { expected: usize, got: usize },
    Ecc(ReedSolomonError),
    Format(crate::fragment::FormatError),
    Codec(CodecError),
    HashMismatch { expected: [u8; 32], actual: [u8; 32] },
    Io(String),
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryError::NotEnoughDataBlocks { have, need } => {
                write!(f, "recovery: only {have} data blocks available, {need} needed")
            }
            RecoveryError::TooManyErasures { erasures, tolerance } => {
                write!(
                    f,
                    "recovery: {erasures} data erasures exceed redundancy tolerance {tolerance}"
                )
            }
            RecoveryError::BlockCountMismatch { expected, got } => {
                write!(f, "recovery: expected {expected} blocks, got {got}")
            }
            RecoveryError::BlockSizeMismatch { expected, got } => {
                write!(f, "recovery: expected block size {expected}, got {got}")
            }
            RecoveryError::Ecc(e) => write!(f, "recovery: Reed-Solomon error: {e}"),
            RecoveryError::Format(e) => write!(f, "recovery: format error: {e}"),
            RecoveryError::Codec(e) => write!(f, "recovery: codec error: {e}"),
            RecoveryError::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "recovery: final hash mismatch: expected {expected:02x?}, actual {actual:02x?}"
                )
            }
            RecoveryError::Io(msg) => write!(f, "recovery: I/O error: {msg}"),
        }
    }
}

impl std::error::Error for RecoveryError {}

impl From<ReedSolomonError> for RecoveryError {
    fn from(e: ReedSolomonError) -> Self {
        RecoveryError::Ecc(e)
    }
}

impl From<crate::fragment::FormatError> for RecoveryError {
    fn from(e: crate::fragment::FormatError) -> Self {
        RecoveryError::Format(e)
    }
}

impl From<CodecError> for RecoveryError {
    fn from(e: CodecError) -> Self {
        RecoveryError::Codec(e)
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Recover data from `blocks`, indexed by fragment id (position in the slice).
/// `None` denotes a missing/dropped block.
pub fn recover(
    blocks: &[Option<Vec<u8>>],
    config: &RecoveryConfig,
    original_hash: Option<[u8; 32]>,
) -> Result<RecoveryReport, RecoveryError> {
    let start = now_ms();
    let k = config.ecc_k;
    let r = config.ecc_r;
    let n = k + r;
    let block_size = HEADER_SIZE + config.block_payload_size;

    if blocks.len() != n {
        return Err(RecoveryError::BlockCountMismatch { expected: n, got: blocks.len() });
    }

    // --- Parse and authenticate -------------------------------------------
    let parse_start = now_ms();
    let mut parsed: Vec<Option<Fragment>> = vec![None; n];
    let mut seen_ids = std::collections::HashSet::new();
    let mut duplicates_removed = 0u64;
    let mut corrupted = 0u64;
    let mut missing = 0u64;

    for (id, block) in blocks.iter().enumerate() {
        match block {
            None => missing += 1,
            Some(block) => {
                if block.len() != block_size {
                    return Err(RecoveryError::BlockSizeMismatch { expected: block_size, got: block.len() });
                }
                match deserialize_block(block, config.block_payload_size) {
                    Ok(fragment) => {
                        if !seen_ids.insert(fragment.meta.id) {
                            duplicates_removed += 1;
                            continue;
                        }
                        parsed[id] = Some(fragment);
                    }
                    Err(_) => corrupted += 1,
                }
            }
        }
    }
    let parsed_count = parsed.iter().filter(|o| o.is_some()).count() as u64;
    let parse_ms = now_ms() - parse_start;

    // --- Classify erasures -------------------------------------------------
    let data_blocks: Vec<Option<&Fragment>> = parsed[..k].iter().map(|o| o.as_ref()).collect();
    let parity_blocks: Vec<Option<&Fragment>> = parsed[k..].iter().map(|o| o.as_ref()).collect();

    let data_erasures = data_blocks.iter().filter(|o| o.is_none()).count();
    if data_erasures > r {
        return Err(RecoveryError::TooManyErasures { erasures: data_erasures, tolerance: r });
    }

    // --- Reed-Solomon erasure correction ----------------------------------
    let ecc_start = now_ms();
    let ecc_config = ReedSolomonConfig::new(k, r)?;
    let rs = ReedSolomon::new(ecc_config)?;

    // Re-serialize known fragments into canonical blocks; erasures are None.
    let rs_blocks: Vec<Option<Vec<u8>>> = (0..n)
        .map(|i| match &parsed[i] {
            None => None,
            Some(fragment) => Some(serialize_block(fragment, config.block_payload_size)),
        })
        .collect();

    let recovered_blocks = rs.recover(&rs_blocks.iter().map(|b| b.as_deref()).collect::<Vec<_>>().as_slice())?;
    let ecc_ms = now_ms() - ecc_start;

    // Re-validate every recovered block's hash (RS output must authenticate).
    let mut recovered_fragments: Vec<Option<Fragment>> = vec![None; n];
    for (i, block) in recovered_blocks.iter().enumerate() {
        let fragment = deserialize_block(block, config.block_payload_size)?;
        recovered_fragments[i] = Some(fragment);
    }

    // --- Reassemble DNA stream in offset order ----------------------------
    let reassemble_start = now_ms();
    let mut data_fragments: Vec<&Fragment> = (0..k)
        .filter_map(|i| recovered_fragments[i].as_ref())
        .collect();
    data_fragments.sort_by_key(|f| f.meta.offset_bases);

    // Verify offsets are contiguous and cover the stream.
    let mut expected_offset: u64 = 0;
    let mut dna_bases = 0usize;
    for f in &data_fragments {
        if f.meta.offset_bases != expected_offset {
            return Err(RecoveryError::NotEnoughDataBlocks { have: data_fragments.len(), need: k });
        }
        expected_offset += f.meta.payload_len as u64;
        dna_bases += f.meta.payload_len;
    }

    let mut dna = Vec::with_capacity(dna_bases);
    for f in &data_fragments {
        dna.extend(f.payload.iter().cloned());
    }
    let reassemble_ms = now_ms() - reassemble_start;

    // --- Decode DNA -> bytes ----------------------------------------------
    let decode_start = now_ms();
    let bytes = decode_bases(&dna)?;
    let decode_ms = now_ms() - decode_start;

    // --- Constraints ------------------------------------------------------
    let mut constraint_violations = Vec::new();
    if config.validate_constraints {
        let (_status, violations) = validate(&dna, config.constraint_config);
        constraint_violations = violations;
    }

    // --- Verify -----------------------------------------------------------
    let verify_start = now_ms();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let recovered_hash = hasher.finalize().into();
    let exact_match = original_hash.map(|expected| {
        let match_ = expected == recovered_hash;
        if !match_ {
            // fall through to error below via caller? report it instead.
        }
        match_
    });
    let verify_ms = now_ms() - verify_start;

    let mut report = RecoveryReport {
        blocks_total: n,
        blocks_parsed: parsed_count as usize,
        blocks_corrupted: corrupted as usize,
        blocks_missing: missing as usize,
        duplicates_removed: duplicates_removed as usize,
        data_erasures: data_erasures,
        data_recovered: k - data_erasures,
        unrecovered: false,
        recovered_bytes: Some(bytes.clone()),
        recovered_hash: Some(recovered_hash),
        original_hash,
        exact_match,
        constraint_violations,
        dna_bases,
        stage_timing: StageTiming {
            parse_ms,
            ecc_ms,
            reassemble_ms,
            decode_ms,
            verify_ms,
            total_ms: now_ms() - start,
        },
    };

    if let Some(expected) = original_hash {
        if Some(recovered_hash) != Some(expected) {
            return Err(RecoveryError::HashMismatch { expected, actual: recovered_hash });
        }
    }

    Ok(report)
}

/// Recover from an archive where blocks are addressed by fragment id, with
/// `total_blocks = k + r`. Convenience wrapper over [`recover`].
pub fn recover_blocks(
    blocks: &[Option<Vec<u8>>],
    config: &RecoveryConfig,
    original_hash: Option<[u8; 32]>,
) -> Result<RecoveryReport, RecoveryError> {
    recover(blocks, config, original_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::encode_bytes;
    use crate::fragment::Fragment;

    fn make_fragment(id: u64, offset: u64, data_index: u32, payload: Vec<u8>) -> Fragment {
        let bases: Vec<crate::codec::Base> = payload.iter().flat_map(|b| {
            let mut v = Vec::with_capacity(4);
            for shift in (0..4).rev() {
                v.push(crate::codec::Base((b >> (shift * 2)) & 0b11));
            }
            v
        }).collect();
        Fragment::new_data(id, data_index, 4, offset, bases)
    }

    fn archive_blocks(k: usize, r: usize, payloads: Vec<Vec<u8>>) -> Vec<Option<Vec<u8>>> {
        let n = k + r;
        let block_payload_size = payloads.iter().map(|p| p.len()).max().unwrap().next_multiple_of(16);
        let mut fragments: Vec<Fragment> = Vec::new();
        let mut offset: u64 = 0;
        for (i, payload) in payloads.iter().enumerate() {
            let bases: Vec<crate::codec::Base> = payload.iter().flat_map(|b| {
                let mut v = Vec::with_capacity(4);
                for shift in (0..4).rev() {
                    v.push(crate::codec::Base((b >> (shift * 2)) & 0b11));
                }
                v
            }).collect();
            fragments.push(Fragment::new_data(i as u64, i as u32, k as u32, offset, bases));
            offset += payload.len() as u64;
        }
        let data_blocks: Vec<Vec<u8>> = fragments.iter().map(|f| serialize_block(f, block_payload_size)).collect();
        let rs = ReedSolomon::new(ReedSolomonConfig::new(k, r).unwrap()).unwrap();
        let refs: Vec<&[u8]> = data_blocks.iter().map(|b| b.as_slice()).collect();
        let parity = rs.encode(&refs).unwrap();
        let mut blocks: Vec<Option<Vec<u8>>> = data_blocks.into_iter().map(Some).collect();
        for (j, p) in parity.into_iter().enumerate() {
            let mut frag = Fragment::new_parity((k + j) as u64, k as u32, 0, vec![crate::codec::Base(0); p.len()]);
            // re-serialize with parity payload
            let mut block = vec![0u8; HEADER_SIZE + block_payload_size];
            block[0..4].copy_from_slice(crate::fragment::FRAGMENT_MAGIC);
            block[4..6].copy_from_slice(&1u16.to_le_bytes());
            block[6..14].copy_from_slice(&frag.meta.id.to_le_bytes());
            block[14..18].copy_from_slice(&(k as u32).to_le_bytes());
            block[18..22].copy_from_slice(&(p.len() as u32).to_le_bytes());
            block[22..26].copy_from_slice(&0u64.to_le_bytes());
            let mut h = Sha256::new(); h.update(&p); block[26..58].copy_from_slice(&h.finalize());
            block[58..62].copy_from_slice(&FragmentKind::Parity.flag_bits().to_le_bytes());
            block[62..62 + p.len()].copy_from_slice(&p);
            blocks.push(Some(block));
        }
        blocks
    }

    use crate::fragment::FragmentKind;

    #[test]
    fn recover_clean_archive() {
        let payloads: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9], vec![10, 11, 12]];
        let blocks = archive_blocks(4, 2, payloads);
        let mut hasher = Sha256::new();
        for p in &payloads { hasher.update(p); }
        let hash = hasher.finalize().into();
        let config = RecoveryConfig { ecc_k: 4, ecc_r: 2, block_payload_size: 16, ..Default::default() };
        let report = recover_blocks(&blocks, &config, Some(hash)).unwrap();
        assert!(!report.unrecovered);
        assert!(report.exact_match == Some(true));
        let mut expected = Vec::new();
        for p in &payloads { expected.extend_from_slice(p); }
        assert_eq!(report.recovered_bytes.unwrap(), expected);
    }

    #[test]
    fn recover_with_data_erasure() {
        let payloads: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 8]).collect();
        let blocks = archive_blocks(4, 2, payloads.clone());
        let mut blocks = blocks;
        blocks[1] = None; // drop data fragment 1
        let mut hasher = Sha256::new();
        for p in &payloads { hasher.update(p); }
        let hash = hasher.finalize().into();
        let config = RecoveryConfig { ecc_k: 4, ecc_r: 2, block_payload_size: 16, ..Default::default() };
        let report = recover_blocks(&blocks, &config, Some(hash)).unwrap();
        assert!(!report.unrecovered);
        assert!(report.exact_match == Some(true));
        assert_eq!(report.data_erasures, 1);
    }

    #[test]
    fn recover_with_corrupted_block() {
        let payloads: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 8]).collect();
        let mut blocks = archive_blocks(4, 2, payloads.clone());
        blocks[2].as_mut().unwrap()[70] ^= 0xFF; // corrupt payload inside block
        let mut hasher = Sha256::new();
        for p in &payloads { hasher.update(p); }
        let hash = hasher.finalize().into();
        let config = RecoveryConfig { ecc_k: 4, ecc_r: 2, block_payload_size: 16, ..Default::default() };
        let report = recover_blocks(&blocks, &config, Some(hash)).unwrap();
        assert!(!report.unrecovered);
        assert!(report.exact_match == Some(true));
        assert_eq!(report.blocks_corrupted, 1);
    }

    #[test]
    fn too_many_erasures_is_error() {
        let payloads: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 8]).collect();
        let mut blocks = archive_blocks(4, 2, payloads);
        blocks[0] = None;
        blocks[1] = None;
        blocks[2] = None; // 3 erasures > r=2
        let config = RecoveryConfig { ecc_k: 4, ecc_r: 2, block_payload_size: 16, ..Default::default() };
        let err = recover_blocks(&blocks, &config, None).unwrap_err();
        assert!(matches!(err, RecoveryError::TooManyErasures { .. }));
    }

    #[test]
    fn hash_mismatch_reported() {
        let payloads: Vec<Vec<u8>> = vec![vec![1; 8], vec![2; 8], vec![3; 8], vec![4; 8]];
        let blocks = archive_blocks(4, 2, payloads);
        let config = RecoveryConfig { ecc_k: 4, ecc_r: 2, block_payload_size: 16, ..Default::default() };
        let err = recover_blocks(&blocks, &config, Some([0u8; 32])).unwrap_err();
        assert!(matches!(err, RecoveryError::HashMismatch { .. }));
    }
}
