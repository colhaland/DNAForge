//! Pipeline: end-to-end orchestration (encode -> validate -> fragment ->
//! simulate -> recover -> reconstruct -> verify -> report).
//!
//! Each stage can be run independently through the CLI so failures can be
//! localized.

use crate::channel::{simulate, ChannelConfig};
use crate::constraints::{validate, ConstraintConfig};
use crate::ecc::{ReedSolomon, ReedSolomonConfig};
use crate::fragment::{
    deserialize_block, serialize_block, Fragment, FragmentKind, HEADER_SIZE,
};
use crate::metrics::{measure, peak_rss_kb, ResilienceMetrics, StageMetric, StorageMetrics};
use crate::report::BenchmarkRow;
use crate::storage::{
    read_archive_blocks_dense, write_archive, ArchiveManifest, StorageError,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// End-to-end pipeline configuration.
#[derive(Clone, Debug)]
pub struct PipelineConfig {
    /// DNA bases per data fragment payload.
    pub fragment_size_bases: usize,
    /// Reed-Solomon data fragments.
    pub ecc_k: usize,
    /// Reed-Solomon parity fragments (tolerable erasures).
    pub ecc_r: usize,
    /// Channel simulation config (zero rates = no simulation).
    pub channel: ChannelConfig,
    /// Validate biological constraints after encoding (reported, not fatal unless strict).
    pub validate_constraints: bool,
    /// If true, constraint violations make `encode` fail.
    pub strict_constraints: bool,
    pub constraint_config: ConstraintConfig,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            fragment_size_bases: 1024,
            ecc_k: 4,
            ecc_r: 2,
            channel: ChannelConfig::default(),
            validate_constraints: false,
            strict_constraints: false,
            constraint_config: ConstraintConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PipelineError {
    Io(String),
    Storage(StorageError),
    Format(crate::fragment::FormatError),
    Codec(crate::codec::CodecError),
    Channel(crate::channel::ChannelError),
    Recovery(crate::recovery::RecoveryError),
    Ecc(crate::ecc::ReedSolomonError),
    Config(String),
    ConstraintViolations(Vec<crate::constraints::ConstraintViolation>),
    HashMismatch { expected: String, actual: String },
    TooManyErasures { erasures: usize, tolerance: usize },
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Io(msg) => write!(f, "pipeline: I/O error: {msg}"),
            PipelineError::Storage(e) => write!(f, "pipeline: storage error: {e}"),
            PipelineError::Format(e) => write!(f, "pipeline: format error: {e}"),
            PipelineError::Codec(e) => write!(f, "pipeline: codec error: {e}"),
            PipelineError::Channel(e) => write!(f, "pipeline: channel error: {e}"),
            PipelineError::Recovery(e) => write!(f, "pipeline: recovery error: {e}"),
            PipelineError::Ecc(e) => write!(f, "pipeline: Reed-Solomon error: {e}"),
            PipelineError::Config(msg) => write!(f, "pipeline: config error: {msg}"),
            PipelineError::ConstraintViolations(v) => {
                write!(f, "pipeline: {} constraint violation(s)", v.len())
            }
            PipelineError::HashMismatch { expected, actual } => {
                write!(f, "pipeline: hash mismatch: expected {expected}, actual {actual}")
            }
            PipelineError::TooManyErasures { erasures, tolerance } => {
                write!(f, "pipeline: {erasures} erasures exceed tolerance {tolerance}")
            }
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<StorageError> for PipelineError {
    fn from(e: StorageError) -> Self {
        PipelineError::Storage(e)
    }
}

impl From<crate::fragment::FormatError> for PipelineError {
    fn from(e: crate::fragment::FormatError) -> Self {
        PipelineError::Format(e)
    }
}

impl From<crate::codec::CodecError> for PipelineError {
    fn from(e: crate::codec::CodecError) -> Self {
        PipelineError::Codec(e)
    }
}

impl From<crate::channel::ChannelError> for PipelineError {
    fn from(e: crate::channel::ChannelError) -> Self {
        PipelineError::Channel(e)
    }
}

impl From<crate::recovery::RecoveryError> for PipelineError {
    fn from(e: crate::recovery::RecoveryError) -> Self {
        PipelineError::Recovery(e)
    }
}

impl From<std::io::Error> for PipelineError {
    fn from(e: std::io::Error) -> Self {
        PipelineError::Io(e.to_string())
    }
}

impl From<crate::ecc::ReedSolomonError> for PipelineError {
    fn from(e: crate::ecc::ReedSolomonError) -> Self {
        PipelineError::Ecc(e)
    }
}

/// Encoded archive in memory: manifest plus fragments ready to write.
pub struct EncodedArchive {
    pub manifest: ArchiveManifest,
    pub fragments: Vec<Fragment>,
    pub block_payload_size: usize,
}

/// Validation report for an archive.
#[derive(Clone, Debug)]
pub struct ValidationReport {
    pub manifest: ArchiveManifest,
    pub blocks_total: u64,
    pub blocks_parsed: u64,
    pub blocks_corrupted: u64,
    pub blocks_missing: u64,
    pub constraint_violations: Vec<crate::constraints::ConstraintViolation>,
    pub data_fragment_count: u64,
}

/// Simulation report written by the simulate command.
#[derive(Clone, Debug)]
pub struct SimulationReport {
    pub fragments_total: u64,
    pub fragments_dropped: u64,
    pub fragments_corrupted: u64,
    pub events: Vec<crate::channel::ErrorEvent>,
    pub stats: crate::channel::ErrorStats,
}

/// Encode `input` bytes into an archive according to `config`.
pub fn encode_pipeline(
    input: &[u8],
    config: &PipelineConfig,
) -> Result<EncodedArchive, PipelineError> {
    let k = config.ecc_k;
    let r = config.ecc_r;

    let mut hasher = Sha256::new();
    hasher.update(input);
    let original_hash = hasher.finalize();
    let original_hash_hex: String = original_hash.iter().map(|b| format!("{b:02x}")).collect();

    // Codec: bytes -> DNA bases.
    let bases = crate::codec::encode_bytes(input);
    let dna_bases = bases.len();

    // Constraint check (reported; optionally fatal).
    let (_status, violations) = validate(&bases, config.constraint_config);
    if config.strict_constraints && !violations.is_empty() {
        return Err(PipelineError::ConstraintViolations(violations.clone()));
    }

    // Fragment the DNA stream into fixed-size payloads.
    let frag_size = config.fragment_size_bases;
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut offset: u64 = 0;
    for chunk in bases.chunks(frag_size) {
        let payload: Vec<crate::codec::Base> = chunk.to_vec();
        fragments.push(Fragment::new_data(
            fragments.len() as u64,
            fragments.len() as u32,
            k as u32,
            offset,
            payload,
        ));
        offset += chunk.len() as u64;
    }

    if fragments.is_empty() {
        fragments.push(Fragment::new_data(0, 0, k as u32, 0, Vec::new()));
    }

    let total_data = fragments.len() as u32;
    let max_payload = fragments.iter().map(|f| f.meta.payload_len).max().unwrap_or(0);
    let block_payload_size = max_payload.next_multiple_of(16).max(16);

    // Serialize data blocks.
    let data_blocks: Vec<Vec<u8>> = fragments
        .iter()
        .map(|f| serialize_block(f, block_payload_size))
        .collect();

    // Reed-Solomon parity over the serialized blocks.
    let ecc_config = ReedSolomonConfig::new(k, r)?;
    let rs = ReedSolomon::new(ecc_config)?;
    let refs: Vec<&[u8]> = data_blocks.iter().map(|b| b.as_slice()).collect();
    let parity_blocks = rs.encode(&refs)?;

    // Parity fragments with authentic headers (payloads hold RS symbols).
    let mut all_fragments = fragments;
    for (j, parity) in parity_blocks.into_iter().enumerate() {
        let id = (k + j) as u64;
        let mut hasher = Sha256::new();
        hasher.update(&parity);
        let payload_hash = hasher.finalize().into();
        all_fragments.push(Fragment {
            meta: crate::fragment::FragmentMeta {
                id,
                kind: FragmentKind::Parity,
                data_index: None,
                total_data,
                offset_bases: 0,
                payload_len: parity.len(),
                payload_hash,
                version: crate::fragment::FRAGMENT_FORMAT_VERSION,
            },
            payload: parity.iter().map(|&b| crate::codec::Base(b)).collect(),
        });
    }

    let manifest = ArchiveManifest {
        format: "dnaforge-archive".into(),
        version: crate::fragment::FRAGMENT_FORMAT_VERSION,
        original_size_bytes: input.len() as u64,
        original_sha256: original_hash_hex,
        dna_bases: dna_bases as u64,
        fragment_size_bases: frag_size as u64,
        block_payload_size: block_payload_size as u64,
        header_size: HEADER_SIZE as u64,
        block_size: (HEADER_SIZE + block_payload_size) as u64,
        ecc_k: k as u64,
        ecc_r: r as u64,
        total_data: total_data as u64,
        total_parity: r as u64,
        fragment_count: all_fragments.len() as u64,
        gc_min: Some(config.constraint_config.gc_min),
        gc_max: Some(config.constraint_config.gc_max),
        max_homopolymer: Some(config.constraint_config.max_homopolymer),
        created_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string(),
        ),
    };

    Ok(EncodedArchive {
        manifest,
        fragments: all_fragments,
        block_payload_size,
    })
}

/// Write an encoded archive to a directory.
pub fn write_encoded_archive(
    archive: &EncodedArchive,
    dir: &Path,
) -> Result<(), PipelineError> {
    write_archive(dir, &archive.manifest, &archive.fragments, archive.block_payload_size)?;
    Ok(())
}

/// Decode an archive back into bytes (assumes no channel errors).
pub fn decode_pipeline(
    dir: &Path,
) -> Result<(Vec<u8>, ArchiveManifest), PipelineError> {
    let blocks = read_archive_blocks_dense(dir)?;
    let manifest_path = dir.join("manifest.json");
    let manifest: ArchiveManifest =
        serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)?,
        )
        .map_err(|e| PipelineError::Storage(StorageError::Manifest(e.to_string())))?;
    let block_payload_size = manifest.block_payload_size as usize;

    let config = crate::recovery::RecoveryConfig {
        ecc_k: manifest.ecc_k as usize,
        ecc_r: manifest.ecc_r as usize,
        block_payload_size,
        validate_constraints: false,
        constraint_config: ConstraintConfig::default(),
    };
    let original_hash = manifest.original_hash();
    let report = crate::recovery::recover_blocks(&blocks, &config, original_hash)?;
    let bytes = report.recovered_bytes.ok_or_else(|| {
        PipelineError::Config("recovery produced no bytes".into())
    })?;
    Ok((bytes, manifest))
}

/// Validate an archive: structure, checksums, constraints.
pub fn validate_pipeline(
    dir: &Path,
) -> Result<ValidationReport, PipelineError> {
    let manifest_path = dir.join("manifest.json");
    let manifest: ArchiveManifest =
        serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)?,
        )
        .map_err(|e| PipelineError::Storage(StorageError::Manifest(e.to_string())))?;
    let block_payload_size = manifest.block_payload_size as usize;

    let blocks = read_archive_blocks_dense(dir)?;
    let mut parsed = 0u64;
    let mut corrupted = 0u64;
    let mut missing = 0u64;
    let mut constraint_violations = Vec::new();

    for block in &blocks {
        match block {
            None => missing += 1,
            Some(block) => {
                match deserialize_block(block, block_payload_size) {
                    Ok(fragment) => {
                        parsed += 1;
                        if fragment.is_data() {
                            let (_status, violations) =
                                validate(&fragment.payload, ConstraintConfig::default());
                            if !violations.is_empty() {
                                constraint_violations.extend(violations);
                            }
                        }
                    }
                    Err(_) => corrupted += 1,
                }
            }
        }
    }

    Ok(ValidationReport {
        manifest,
        blocks_total: blocks.len() as u64,
        blocks_parsed: parsed,
        blocks_corrupted: corrupted,
        blocks_missing: missing,
        constraint_violations,
        data_fragment_count: 0,
    })
}

/// Simulate channel errors on an archive, writing a corrupted archive.
pub fn simulate_pipeline(
    src_dir: &Path,
    dst_dir: &Path,
    config: &ChannelConfig,
) -> Result<SimulationReport, PipelineError> {
    let blocks = read_archive_blocks_dense(src_dir)?;
    let manifest_path = src_dir.join("manifest.json");
    let manifest: ArchiveManifest =
        serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)?,
        )
        .map_err(|e| PipelineError::Storage(StorageError::Manifest(e.to_string())))?;
    let block_payload_size = manifest.block_payload_size as usize;

    // Data fragment payloads (DNA bases) in id order.
    let mut payloads: Vec<Vec<crate::codec::Base>> = Vec::new();
    for i in 0..manifest.ecc_k as usize {
        match &blocks[i] {
            None => payloads.push(Vec::new()),
            Some(block) => {
                let fragment = deserialize_block(block, block_payload_size)?;
                payloads.push(fragment.payload);
            }
        }
    }

    let sim = simulate(&payloads, config)?;

    // Rebuild a corrupted archive. Headers are kept intact, so insertions/
    // deletions change the payload length and cause checksum/length mismatches
    // that the recovery layer detects as erasures.
    let mut fragments: Vec<Fragment> = Vec::new();
    for (i, payload) in sim.fragments.iter().enumerate() {
        let mut block = match &blocks[i] {
            Some(b) => b.clone(),
            None => vec![0u8; HEADER_SIZE + block_payload_size],
        };
        let bases = payload.as_ref().map(|v| v.as_slice()).unwrap_or(&[]);
        let mut mutated: Vec<u8> = bases.iter().map(|b| b.0).collect();
        mutated.resize(block_payload_size, 0);
        mutated.truncate(block_payload_size);
        block[62..62 + mutated.len()].copy_from_slice(&mutated);
        let fragment = deserialize_block(&block, block_payload_size).unwrap_or_else(|_| {
            Fragment::new_data(i as u64, i as u32, manifest.ecc_k as u32, 0, Vec::new())
        });
        fragments.push(fragment);
    }

    // Parity blocks copied as-is.
    for i in manifest.ecc_k as usize..blocks.len() {
        if let Some(block) = &blocks[i] {
            fragments.push(deserialize_block(block, block_payload_size)?);
        }
    }

    let corrupted_manifest = ArchiveManifest {
        original_sha256: manifest.original_sha256.clone(),
        ..manifest.clone()
    };
    write_archive(dst_dir, &corrupted_manifest, &fragments, block_payload_size)?;

    Ok(SimulationReport {
        fragments_total: manifest.fragment_count,
        fragments_dropped: sim.stats.dropouts,
        fragments_corrupted: sim.stats.insertions + sim.stats.deletions + sim.stats.substitutions,
        events: sim.events,
        stats: sim.stats,
    })
}

/// Recover a (possibly corrupted) archive into bytes.
pub fn recover_pipeline(
    dir: &Path,
    original_hash: Option<[u8; 32]>,
) -> Result<(Vec<u8>, crate::recovery::RecoveryReport), PipelineError> {
    let blocks = read_archive_blocks_dense(dir)?;
    let manifest_path = dir.join("manifest.json");
    let manifest: ArchiveManifest =
        serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)?,
        )
        .map_err(|e| PipelineError::Storage(StorageError::Manifest(e.to_string())))?;
    let block_payload_size = manifest.block_payload_size as usize;

    let config = crate::recovery::RecoveryConfig {
        ecc_k: manifest.ecc_k as usize,
        ecc_r: manifest.ecc_r as usize,
        block_payload_size,
        validate_constraints: false,
        constraint_config: ConstraintConfig::default(),
    };
    let hash = original_hash.or_else(|| manifest.original_hash());
    let report = crate::recovery::recover_blocks(&blocks, &config, hash)?;
    let bytes = report.recovered_bytes.clone().ok_or_else(|| {
        PipelineError::Config("recovery produced no bytes".into())
    })?;
    Ok((bytes, report))
}

/// Run one full benchmark experiment and return a CSV row.
pub fn benchmark_once(
    input: &[u8],
    label: &str,
    seed: u64,
    config: &PipelineConfig,
) -> Result<BenchmarkRow, PipelineError> {
    let (archive_result, encode_ms) = measure(|| encode_pipeline(input, config));
    let archive = archive_result?;

    let dir = std::env::temp_dir().join(format!("dnaforge-bench-{:x}-{}", seed, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_encoded_archive(&archive, &dir)?;

    let original_hash = archive.manifest.original_hash();

    let (_result, decode_ms) = measure(|| decode_pipeline(&dir));
    let _ = _result?;

    let (simulate_ms, recover_ms, resilience) = if config.channel.substitution_rate == 0.0
        && config.channel.insertion_rate == 0.0
        && config.channel.deletion_rate == 0.0
        && config.channel.dropout_rate == 0.0
    {
        (0, 0, ResilienceMetrics::default())
    } else {
        let corrupted_dir = dir.join("corrupted");
        let (sim_result, sim_ms) = measure(|| {
            simulate_pipeline(dir.as_path(), &corrupted_dir, &config.channel).map(|_| ())
        });
        sim_result?;
        let (rec_result, rec_ms) =
            measure(|| recover_pipeline(&corrupted_dir, original_hash));
        let (_bytes, report) = rec_result?;
        let resilience = ResilienceMetrics {
            fragments_total: report.blocks_total as u64,
            fragments_dropped: report.blocks_missing as u64,
            fragments_corrupted: report.blocks_corrupted as u64,
            fragments_recovered: report.data_recovered as u64,
            fragments_unrecovered: if report.unrecovered {
                report.blocks_total.saturating_sub(report.data_recovered) as u64
            } else {
                0
            },
            exact_file_recovery: report.exact_match == Some(true),
            recovery_rate_fragments: if report.blocks_total > 0 {
                report.data_recovered as f64 / report.blocks_total as f64
            } else {
                0.0
            },
        };
        (sim_ms, rec_ms, resilience)
    };

    let storage = StorageMetrics::new(
        input.len() as u64,
        archive.manifest.dna_bases,
        archive.fragments.len() as u64
            * (HEADER_SIZE + archive.block_payload_size) as u64,
    );

    Ok(BenchmarkRow {
        experiment: label.into(),
        seed,
        input_bytes: input.len() as u64,
        dna_bases: archive.manifest.dna_bases,
        fragment_size_bases: config.fragment_size_bases as u64,
        ecc_k: config.ecc_k as u64,
        ecc_r: config.ecc_r as u64,
        encode_ms,
        decode_ms,
        simulate_ms,
        recover_ms,
        total_ms: encode_ms + decode_ms + simulate_ms + recover_ms,
        encode_mib_s: StageMetric {
            name: "encode".into(),
            elapsed_ms: encode_ms,
            bytes_processed: input.len() as u64,
        }
        .throughput_mib_per_s(),
        decode_mib_s: StageMetric {
            name: "decode".into(),
            elapsed_ms: decode_ms,
            bytes_processed: input.len() as u64,
        }
        .throughput_mib_per_s(),
        fragments_total: resilience.fragments_total,
        fragments_dropped: resilience.fragments_dropped,
        fragments_corrupted: resilience.fragments_corrupted,
        fragments_recovered: resilience.fragments_recovered,
        fragments_unrecovered: resilience.fragments_unrecovered,
        exact_file_recovery: resilience.exact_file_recovery,
        recovery_rate: resilience.recovery_rate_fragments(),
        encoded_bytes: storage.encoded_bytes,
        overhead_percent: storage.overhead_percent,
        peak_rss_kb: peak_rss_kb(),
    })
}

use std::time::SystemTime;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let data: Vec<u8> = (0..200).map(|i| (i % 251) as u8).collect();
        let config = PipelineConfig {
            fragment_size_bases: 64,
            ecc_k: 4,
            ecc_r: 2,
            ..Default::default()
        };
        let archive = encode_pipeline(&data, &config).unwrap();
        let dir = std::env::temp_dir().join(format!("dnaforge-pipeline-test-{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&dir);
        write_encoded_archive(&archive, &dir).unwrap();
        let (decoded, manifest) = decode_pipeline(&dir).unwrap();
        assert_eq!(decoded, data);
        let expected_hash: [u8; 32] = {
            let hex = sha256_hex(&data);
            let mut arr = [0u8; 32];
            for i in 0..32 {
                arr[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
            }
            arr
        };
        assert_eq!(manifest.original_hash(), Some(expected_hash));
        let _ = manifest;
    }

    #[test]
    fn simulate_then_recover_within_tolerance() {
        let data: Vec<u8> = (0..500).map(|i| i as u8).collect();
        let config = PipelineConfig {
            fragment_size_bases: 64,
            ecc_k: 4,
            ecc_r: 2,
            channel: ChannelConfig {
                substitution_rate: 0.05,
                deletion_rate: 0.02,
                dropout_rate: 0.0,
                duplication_rate: 0.0,
                seed: 42,
                ..Default::default()
            },
            ..Default::default()
        };
        let archive = encode_pipeline(&data, &config).unwrap();
        let dir = std::env::temp_dir().join(format!("dnaforge-pipeline-test-{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&dir);
        write_encoded_archive(&archive, &dir).unwrap();

        let corrupted = dir.join("corrupted");
        simulate_pipeline(dir, &corrupted, &config.channel).unwrap();
        let (recovered, report) = recover_pipeline(&corrupted, archive.manifest.original_hash()).unwrap();
        assert_eq!(recovered, data);
        assert!(report.exact_match == Some(true));
    }
}
