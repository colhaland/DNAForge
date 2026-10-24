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

use crate::codec::{CodecError, decode_bases};
use crate::constraints::{ConstraintConfig, validate};
use crate::ecc::{ReedSolomon, ReedSolomonConfig, ReedSolomonError};
use crate::fragment::{Fragment, FragmentKind, HEADER_SIZE, deserialize_block};
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
    pub fragment_size_bases: usize,
    pub validate_constraints: bool,
    pub constraint_config: ConstraintConfig,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size: 1024,
            fragment_size_bases: 0,
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
    NotEnoughDataBlocks {
        have: usize,
        need: usize,
    },
    TooManyErasures {
        erasures: usize,
        tolerance: usize,
    },
    BlockCountMismatch {
        expected: usize,
        got: usize,
    },
    BlockSizeMismatch {
        expected: usize,
        got: usize,
    },
    Ecc(ReedSolomonError),
    Format(crate::fragment::FormatError),
    Codec(CodecError),
    HashMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    Io(String),
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryError::NotEnoughDataBlocks { have, need } => {
                write!(
                    f,
                    "recovery: only {have} data blocks available, {need} needed"
                )
            }
            RecoveryError::TooManyErasures {
                erasures,
                tolerance,
            } => {
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
    let mut dna_bases = 0usize;
    let mut inferred_fragment_size_bases = 0usize;
    for block in blocks.iter().flatten() {
        if let Ok(fragment) = deserialize_block(block, config.block_payload_size)
            && fragment.is_data()
        {
            if fragment.meta.payload_len > 0 {
                dna_bases =
                    dna_bases.max(fragment.meta.offset_bases as usize + fragment.meta.payload_len);
            }
            inferred_fragment_size_bases =
                inferred_fragment_size_bases.max(fragment.meta.payload_len);
        }
    }
    let fragment_size_bases = if config.fragment_size_bases == 0 {
        inferred_fragment_size_bases.max(1)
    } else {
        config.fragment_size_bases
    };
    recover_striped(
        blocks,
        config,
        config.ecc_k,
        dna_bases,
        fragment_size_bases,
        original_hash,
    )
}

/// Recover an archive whose data and parity shards are grouped into stripes.
pub fn recover_striped(
    blocks: &[Option<Vec<u8>>],
    config: &RecoveryConfig,
    total_data_fragments: usize,
    dna_bases: usize,
    fragment_size_bases: usize,
    original_hash: Option<[u8; 32]>,
) -> Result<RecoveryReport, RecoveryError> {
    let start = now_ms();
    let k = config.ecc_k;
    let r = config.ecc_r;
    let ecc_config = ReedSolomonConfig::new(k, r)?;
    let rs = ReedSolomon::new(ecc_config)?;
    let n = ecc_config.n();
    let block_size = HEADER_SIZE + config.block_payload_size;
    if total_data_fragments == 0 || fragment_size_bases == 0 {
        return Err(RecoveryError::BlockCountMismatch {
            expected: 1,
            got: total_data_fragments,
        });
    }
    let stripe_count = total_data_fragments.div_ceil(k);
    let expected_blocks = stripe_count
        .checked_mul(n)
        .ok_or(RecoveryError::BlockCountMismatch {
            expected: usize::MAX,
            got: blocks.len(),
        })?;

    if blocks.len() != expected_blocks {
        return Err(RecoveryError::BlockCountMismatch {
            expected: expected_blocks,
            got: blocks.len(),
        });
    }

    // --- Parse and authenticate -------------------------------------------
    let parse_start = now_ms();
    let mut parsed: Vec<Option<Fragment>> = vec![None; expected_blocks];
    let mut corrupted = 0usize;
    let mut missing = 0usize;
    let mut data_erasures = 0usize;

    for (id, block) in blocks.iter().enumerate() {
        let stripe = id / n;
        let shard = id % n;
        let data_index = stripe * k + shard;
        let is_data = shard < k;
        let expected_offset = if is_data {
            (data_index * fragment_size_bases) as u64
        } else {
            0
        };
        let expected_len = if is_data && data_index < total_data_fragments {
            dna_bases
                .saturating_sub(expected_offset as usize)
                .min(fragment_size_bases)
        } else if is_data {
            0
        } else {
            config.block_payload_size
        };

        let valid_fragment = match block {
            None => {
                missing += 1;
                None
            }
            Some(block) => {
                if block.len() != block_size {
                    return Err(RecoveryError::BlockSizeMismatch {
                        expected: block_size,
                        got: block.len(),
                    });
                }
                match deserialize_block(block, config.block_payload_size) {
                    Ok(fragment)
                        if fragment.meta.id == id as u64
                            && fragment.meta.total_data as usize == total_data_fragments
                            && fragment.meta.kind
                                == if is_data {
                                    FragmentKind::Data
                                } else {
                                    FragmentKind::Parity
                                }
                            && fragment.meta.offset_bases == expected_offset
                            && fragment.meta.payload_len == expected_len
                            && (!is_data
                                || fragment.payload.iter().all(|base| base.is_valid())) =>
                    {
                        Some(fragment)
                    }
                    _ => {
                        corrupted += 1;
                        None
                    }
                }
            }
        };
        if is_data && data_index < total_data_fragments && valid_fragment.is_none() {
            data_erasures += 1;
        }
        parsed[id] = valid_fragment;
    }
    let parsed_count = parsed.iter().filter(|fragment| fragment.is_some()).count();
    let parse_ms = now_ms() - parse_start;

    // --- Reed-Solomon erasure correction ----------------------------------
    let ecc_start = now_ms();
    let mut recovered_data = Vec::with_capacity(total_data_fragments);
    for stripe in 0..stripe_count {
        let start_id = stripe * n;
        let rs_blocks: Vec<Option<Vec<u8>>> = (0..n)
            .map(|shard| {
                let id = start_id + shard;
                parsed[id].as_ref().map(|fragment| {
                    let mut payload: Vec<u8> = fragment.payload.iter().map(|base| base.0).collect();
                    payload.resize(config.block_payload_size, 0);
                    payload
                })
            })
            .collect();
        let stripe_erasures = rs_blocks.iter().filter(|block| block.is_none()).count();
        if stripe_erasures > r {
            return Err(RecoveryError::TooManyErasures {
                erasures: stripe_erasures,
                tolerance: r,
            });
        }
        let recovered = rs.recover(
            &rs_blocks
                .iter()
                .map(|block| block.as_deref())
                .collect::<Vec<_>>(),
        )?;

        for (shard, recovered_shard) in recovered.iter().take(k).enumerate() {
            let data_index = stripe * k + shard;
            if data_index >= total_data_fragments {
                continue;
            }
            let offset = data_index * fragment_size_bases;
            let length = dna_bases.saturating_sub(offset).min(fragment_size_bases);
            let payload = &recovered_shard[..length];
            if payload.iter().any(|base| *base > 3) {
                return Err(RecoveryError::Format(
                    crate::fragment::FormatError::PayloadTooLarge {
                        payload_len: length,
                        block_payload_size: config.block_payload_size,
                    },
                ));
            }
            recovered_data.extend(payload.iter().copied().map(crate::codec::Base));
        }
    }
    let ecc_ms = now_ms() - ecc_start;

    // --- Reassemble DNA stream in offset order ----------------------------
    let reassemble_start = now_ms();
    let dna = recovered_data;
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
    let exact_match = original_hash.map(|expected| expected == recovered_hash);
    let verify_ms = now_ms() - verify_start;

    let report = RecoveryReport {
        blocks_total: expected_blocks,
        blocks_parsed: parsed_count,
        blocks_corrupted: corrupted,
        blocks_missing: missing,
        duplicates_removed: 0,
        data_erasures,
        data_recovered: data_erasures,
        unrecovered: false,
        recovered_bytes: Some(bytes),
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

    if let Some(expected) = original_hash
        && recovered_hash != expected
    {
        return Err(RecoveryError::HashMismatch {
            expected,
            actual: recovered_hash,
        });
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

/// Recover a flat archive that may contain multiple codeword stripes.
/// Each stripe is recovered independently and the recovered payloads are
/// concatenated in the original fragment order.
pub fn recover_archive_blocks(
    blocks: &[Option<Vec<u8>>],
    config: &RecoveryConfig,
    total_data: usize,
    original_hash: Option<[u8; 32]>,
) -> Result<RecoveryReport, RecoveryError> {
    if total_data == 0 {
        let mut sha = Sha256::new();
        sha.update([]);
        let empty_hash = sha.finalize().into();
        let exact_match = original_hash.map(|expected| expected == empty_hash);
        return Ok(RecoveryReport {
            blocks_total: blocks.len(),
            blocks_parsed: 0,
            blocks_corrupted: 0,
            blocks_missing: 0,
            duplicates_removed: 0,
            data_erasures: 0,
            data_recovered: 0,
            unrecovered: false,
            recovered_bytes: Some(Vec::new()),
            recovered_hash: Some(empty_hash),
            original_hash,
            exact_match,
            constraint_violations: Vec::new(),
            dna_bases: 0,
            stage_timing: StageTiming::default(),
        });
    }

    let mut dna_bases = 0usize;
    let mut inferred_fragment_size_bases = 0usize;
    for block in blocks.iter().flatten() {
        if let Ok(fragment) = deserialize_block(block, config.block_payload_size)
            && fragment.is_data()
        {
            if fragment.meta.payload_len > 0 {
                dna_bases =
                    dna_bases.max(fragment.meta.offset_bases as usize + fragment.meta.payload_len);
            }
            inferred_fragment_size_bases =
                inferred_fragment_size_bases.max(fragment.meta.payload_len);
        }
    }
    let fragment_size_bases = if config.fragment_size_bases == 0 {
        inferred_fragment_size_bases.max(1)
    } else {
        config.fragment_size_bases
    };

    let report = recover_striped(
        blocks,
        config,
        total_data,
        dna_bases,
        fragment_size_bases,
        original_hash,
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Base;
    use crate::fragment::{Fragment, serialize_block};

    fn archive_blocks(k: usize, r: usize, payloads: &[Vec<u8>]) -> Vec<Option<Vec<u8>>> {
        let block_payload_size = if payloads.is_empty() {
            16
        } else {
            payloads
                .iter()
                .map(|p| p.len() * 4)
                .max()
                .unwrap()
                .next_multiple_of(16)
        };

        let mut fragments: Vec<Fragment> = Vec::new();
        let mut offset: u64 = 0;
        for (i, payload) in payloads.iter().enumerate() {
            let bases: Vec<Base> = payload
                .iter()
                .flat_map(|b| {
                    let mut v = Vec::with_capacity(4);
                    for shift in (0..4).rev() {
                        v.push(Base((b >> (shift * 2)) & 0b11));
                    }
                    v
                })
                .collect();
            fragments.push(Fragment::new_data(
                i as u64, i as u32, k as u32, offset, bases,
            ));
            offset += payload.len() as u64 * 4;
        }

        let data_payloads: Vec<Vec<u8>> = fragments
            .iter()
            .map(|fragment| fragment.payload.iter().map(|base| base.0).collect())
            .collect();
        let rs = ReedSolomon::new(ReedSolomonConfig::new(k, r).unwrap()).unwrap();
        let refs: Vec<&[u8]> = data_payloads.iter().map(Vec::as_slice).collect();
        let parity_blocks = rs.encode(&refs).unwrap();

        let mut blocks: Vec<Option<Vec<u8>>> = fragments
            .iter()
            .map(|fragment| Some(serialize_block(fragment, block_payload_size)))
            .collect();
        for (j, parity) in parity_blocks.into_iter().enumerate() {
            let frag = Fragment::new_parity(
                (k + j) as u64,
                k as u32,
                0,
                parity.iter().copied().map(Base).collect(),
            );
            blocks.push(Some(serialize_block(&frag, block_payload_size)));
        }
        blocks
    }

    #[test]
    fn recover_clean_archive() {
        let payloads: Vec<Vec<u8>> = vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
            vec![10, 11, 12],
        ];
        let blocks = archive_blocks(4, 2, &payloads);
        let mut hasher = Sha256::new();
        for p in &payloads {
            hasher.update(p);
        }
        let hash = hasher.finalize().into();
        let block_payload_size = payloads
            .iter()
            .map(|p| p.len() * 4)
            .max()
            .unwrap()
            .next_multiple_of(16);
        let config = RecoveryConfig {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size,
            ..Default::default()
        };
        let report = recover_blocks(&blocks, &config, Some(hash)).unwrap();
        assert!(!report.unrecovered);
        assert!(report.exact_match == Some(true));
        let mut expected = Vec::new();
        for p in &payloads {
            expected.extend_from_slice(p);
        }
        assert_eq!(report.recovered_bytes.unwrap(), expected);
    }

    #[test]
    fn recover_with_data_erasure() {
        let payloads: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 8]).collect();
        let mut blocks = archive_blocks(4, 2, &payloads);
        blocks[1] = None; // drop data fragment 1
        let mut hasher = Sha256::new();
        for p in &payloads {
            hasher.update(p);
        }
        let hash = hasher.finalize().into();
        let block_payload_size = payloads
            .iter()
            .map(|p| p.len() * 4)
            .max()
            .unwrap()
            .next_multiple_of(16);
        let config = RecoveryConfig {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size,
            ..Default::default()
        };
        let report = recover_blocks(&blocks, &config, Some(hash)).unwrap();
        assert!(!report.unrecovered);
        assert!(report.exact_match == Some(true));
        assert_eq!(report.data_erasures, 1);
    }

    #[test]
    fn recover_with_corrupted_block() {
        let payloads: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 8]).collect();
        let mut blocks = archive_blocks(4, 2, &payloads);
        let block_payload_size = payloads
            .iter()
            .map(|p| p.len() * 4)
            .max()
            .unwrap()
            .next_multiple_of(16);
        blocks[2].as_mut().unwrap()[62 + block_payload_size / 2] ^= 0xFF; // corrupt payload inside block
        let mut hasher = Sha256::new();
        for p in &payloads {
            hasher.update(p);
        }
        let hash = hasher.finalize().into();
        let config = RecoveryConfig {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size,
            ..Default::default()
        };
        let report = recover_blocks(&blocks, &config, Some(hash)).unwrap();
        assert!(!report.unrecovered);
        assert!(report.exact_match == Some(true));
        assert_eq!(report.blocks_corrupted, 1);
    }

    #[test]
    fn too_many_erasures_is_error() {
        let payloads: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8; 8]).collect();
        let mut blocks = archive_blocks(4, 2, &payloads);
        let block_payload_size = payloads
            .iter()
            .map(|p| p.len() * 4)
            .max()
            .unwrap()
            .next_multiple_of(16);
        blocks[0] = None;
        blocks[1] = None;
        blocks[2] = None; // 3 erasures > r=2
        let config = RecoveryConfig {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size,
            ..Default::default()
        };
        let err = recover_blocks(&blocks, &config, None).unwrap_err();
        assert!(matches!(err, RecoveryError::TooManyErasures { .. }));
    }

    #[test]
    fn hash_mismatch_reported() {
        let payloads: Vec<Vec<u8>> = vec![vec![1; 8], vec![2; 8], vec![3; 8], vec![4; 8]];
        let blocks = archive_blocks(4, 2, &payloads);
        let block_payload_size = payloads
            .iter()
            .map(|p| p.len() * 4)
            .max()
            .unwrap()
            .next_multiple_of(16);
        let config = RecoveryConfig {
            ecc_k: 4,
            ecc_r: 2,
            block_payload_size,
            ..Default::default()
        };
        let err = recover_blocks(&blocks, &config, Some([0u8; 32])).unwrap_err();
        assert!(matches!(err, RecoveryError::HashMismatch { .. }));
    }
}
