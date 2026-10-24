//! Command-line interface for DNAForge.
//!
//! Commands: encode, decode, validate, simulate, recover, benchmark, inspect.

use clap::{Parser, Subcommand};
use dnaforge::channel::ChannelConfig;
use dnaforge::constraints::ConstraintConfig;
use dnaforge::pipeline::{
    benchmark_once, decode_pipeline, encode_pipeline, recover_pipeline, simulate_pipeline,
    validate_pipeline, PipelineConfig,
};
use dnaforge::report::{write_csv, write_json_summary, BenchmarkSummary};
use dnaforge::storage::{read_file, write_file, sha256_hex, ArchiveManifest};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser)]
#[command(name = "dnaforge", version = "0.1.0", about = "DNAForge — computational DNA data storage baseline")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Encode a file into a DNAForge archive (bytes -> DNA -> fragments).
    Encode {
        #[command(flatten)]
        io: IoArgs,
        /// DNA bases per data fragment payload.
        #[arg(long, default_value = "1024")]
        fragment_size: usize,
        /// Reed-Solomon data fragments.
        #[arg(long, default_value = "4")]
        ecc_k: usize,
        /// Reed-Solomon parity fragments (max tolerable erasures).
        #[arg(long, default_value = "2")]
        ecc_r: usize,
        /// Reject input whose encoded DNA violates the constraint config.
        #[arg(long)]
        strict: bool,
        /// Minimum GC fraction (0..=1).
        #[arg(long, default_value = "0.3")]
        gc_min: f64,
        /// Maximum GC fraction (0..=1).
        #[arg(long, default_value = "0.7")]
        gc_max: f64,
        /// Maximum homopolymer run length.
        #[arg(long, default_value = "5")]
        max_homopolymer: usize,
    },
    /// Decode an archive back into bytes.
    Decode {
        #[command(flatten)]
        io: IoArgs,
    },
    /// Validate an archive's structure, checksums, and constraints.
    Validate {
        #[command(flatten)]
        io: IoArgs,
    },
    /// Simulate DNA channel errors on an archive.
    Simulate {
        #[command(flatten)]
        io: IoArgs,
        /// Output directory for the corrupted archive.
        #[arg(long)]
        output: PathBuf,
        /// Substitution rate per base (0..=1).
        #[arg(long, default_value = "0.0")]
        sub: f64,
        /// Insertion rate per base (0..=1).
        #[arg(long, default_value = "0.0")]
        ins: f64,
        /// Deletion rate per base (0..=1).
        #[arg(long, default_value = "0.0")]
        del: f64,
        /// Fragment dropout rate (0..=1).
        #[arg(long, default_value = "0.0")]
        dropout: f64,
        /// Fragment duplication rate (0..=1).
        #[arg(long, default_value = "0.0")]
        dup: f64,
        /// Random seed (deterministic).
        #[arg(long, default_value = "0")]
        seed: u64,
    },
    /// Recover a (possibly corrupted) archive into bytes.
    Recover {
        #[command(flatten)]
        io: IoArgs,
        /// Original file to compare hashes for exact-recovery verification.
        #[arg(long)]
        original: Option<PathBuf>,
    },
    /// Benchmark encode/decode/recover and export CSV or JSON.
    Benchmark {
        #[command(flatten)]
        io: IoArgs,
        /// Output report path (.csv or .json).
        #[arg(long)]
        report: PathBuf,
        /// Input sizes to benchmark (e.g. 1KiB,64KiB,1MiB).
        #[arg(long, default_value = "1KiB,64KiB,1MiB")]
        sizes: String,
        /// Random seeds for repeated runs.
        #[arg(long, default_value = "1,2,3")]
        seeds: String,
        #[arg(long, default_value = "4")]
        ecc_k: usize,
        #[arg(long, default_value = "2")]
        ecc_r: usize,
        #[arg(long, default_value = "1024")]
        fragment_size: usize,
        /// Channel substitution rate (0..=1).
        #[arg(long, default_value = "0.0")]
        sub: f64,
        /// Channel insertion rate (0..=1).
        #[arg(long, default_value = "0.0")]
        ins: f64,
        /// Channel deletion rate (0..=1).
        #[arg(long, default_value = "0.0")]
        del: f64,
        /// Channel dropout rate (0..=1).
        #[arg(long, default_value = "0.0")]
        dropout: f64,
        /// Channel duplication rate (0..=1).
        #[arg(long, default_value = "0.0")]
        dup: f64,
        #[arg(long, default_value = "0")]
        seed: u64,
    },
    /// Inspect an archive: manifest and per-fragment summary.
    Inspect {
        #[command(flatten)]
        io: IoArgs,
    },
}

#[derive(clap::Args)]
struct IoArgs {
    /// Input file or archive directory.
    #[arg(value_name = "INPUT")]
    input: PathBuf,
    /// Output file or directory.
    #[arg(value_name = "OUTPUT")]
    output: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err}");
        exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Encode { io, fragment_size, ecc_k, ecc_r, strict, gc_min, gc_max, max_homopolymer } => {
            let input = read_file(&io.input)?;
            let config = PipelineConfig {
                fragment_size_bases: fragment_size,
                ecc_k,
                ecc_r,
                strict_constraints: strict,
                constraint_config: ConstraintConfig { gc_min, gc_max, max_homopolymer, ..Default::default() },
                ..Default::default()
            };
            let archive = encode_pipeline(&input, &config)?;
            dnaforge::pipeline::write_encoded_archive(&archive, &io.output)?;
            println!(
                "encoded {} bytes -> {} DNA bases -> {} fragments ({} data + {} parity) in {}",
                input.len(),
                archive.manifest.dna_bases,
                archive.manifest.fragment_count,
                archive.manifest.total_data,
                archive.manifest.total_parity,
                io.output.display(),
            );
        }
        Commands::Decode { io } => {
            let (bytes, manifest) = decode_pipeline(&io.input)?;
            write_file(&io.output, &bytes)?;
            println!(
                "decoded {} -> {} bytes (sha256 {})",
                io.input.display(),
                bytes.len(),
                sha256_hex(&bytes)
            );
            let _ = manifest;
        }
        Commands::Validate { io } => {
            let report = validate_pipeline(&io.input)?;
            println!(
                "archive {}: {} blocks total, {} parsed, {} corrupted, {} missing, {} constraint violation(s)",
                io.input.display(),
                report.blocks_total,
                report.blocks_parsed,
                report.blocks_corrupted,
                report.blocks_missing,
                report.constraint_violations.len(),
            );
            for v in &report.constraint_violations {
                println!("  constraint violation: {:?} - {}", v.kind, v.detail);
            }
            if report.blocks_corrupted > 0 || report.blocks_missing > 0 || !report.constraint_violations.is_empty() {
                exit(2);
            }
        }
        Commands::Simulate { io, output, sub, ins, del, dropout, dup, seed } => {
            let config = ChannelConfig { substitution_rate: sub, insertion_rate: ins, deletion_rate: del, dropout_rate: dropout, duplication_rate: dup, seed };
            let report = simulate_pipeline(&io.input, &output, &config)?;
            println!(
                "simulated on {}: {} fragments total, {} dropped, {} corrupted events, seed {}",
                io.input.display(),
                report.fragments_total,
                report.fragments_dropped,
                report.fragments_corrupted,
                seed,
            );
            println!(
                "  substitutions {} insertions {} deletions {} dropouts {} duplications {}",
                report.stats.substitutions, report.stats.insertions, report.stats.deletions,
                report.stats.dropouts, report.stats.duplications,
            );
        }
        Commands::Recover { io, original } => {
            let original_hash = match &original {
                Some(path) => {
                    let data = read_file(path)?;
                    Some(hash_array(&data))
                }
                None => None,
            };
            let (bytes, report) = recover_pipeline(&io.input, original_hash)?;
            write_file(&io.output, &bytes)?;
            println!(
                "recovered {} bytes from {} (exact match: {:?})",
                bytes.len(),
                io.input.display(),
                report.exact_match,
            );
            println!(
                "  blocks: {} parsed, {} corrupted, {} missing; data erasures {}; stage ms: parse {} ecc {} reassemble {} decode {} verify {} total {}",
                report.blocks_parsed, report.blocks_corrupted, report.blocks_missing,
                report.data_erasures,
                report.stage_timing.parse_ms, report.stage_timing.ecc_ms,
                report.stage_timing.reassemble_ms, report.stage_timing.decode_ms,
                report.stage_timing.verify_ms, report.stage_timing.total_ms,
            );
            if report.exact_match != Some(true) {
                exit(3);
            }
        }
        Commands::Benchmark { io, report, sizes, seeds, ecc_k, ecc_r, fragment_size, sub, ins, del, dropout, dup, seed } => {
            let input = read_file(&io.input)?;
            let size_list = parse_sizes(&sizes)?;
            let seed_list: Vec<u64> = seeds.split(',').map(|s| s.trim().parse::<u64>()).collect::<Result<_, _>>()?;
            let mut config = PipelineConfig {
                ecc_k, ecc_r, fragment_size_bases: fragment_size,
                channel: ChannelConfig { substitution_rate: sub, insertion_rate: ins, deletion_rate: del, dropout_rate: dropout, duplication_rate: dup, seed },
                ..Default::default()
            };
            let mut rows = Vec::new();
            for size in &size_list {
                let data = deterministic_input(*size, seed);
                for &s in &seed_list {
                    let row = benchmark_once(&data, &format!("{}x{}", io.input.display(), size), s, &config)?;
                    rows.push(row);
                }
            }
            if report.extension().map_or(false, |e| e == "json") {
                let summary = BenchmarkSummary::new(rows.clone());
                write_json_summary(&report, &summary)?;
                let rows_len = rows.len();
                println!("benchmark: {} experiments -> {}", rows_len, report.display());
            } else {
                let rows_len = rows.len();
                write_csv(&report, &rows)?;
                println!("benchmark: {} experiments -> {}", rows_len, report.display());
            }
        }
        Commands::Inspect { io } => {
            let manifest_path = io.input.join("manifest.json");
            let manifest: ArchiveManifest =
                serde_json::from_str(&std::fs::read_to_string(&manifest_path)?).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            println!("archive: {}", io.input.display());
            println!("  format: {} v{}", manifest.format, manifest.version);
            println!("  original: {} bytes, sha256 {}", manifest.original_size_bytes, manifest.original_sha256);
            println!("  dna bases: {}", manifest.dna_bases);
            println!("  fragments: {} total ({} data + {} parity)", manifest.fragment_count, manifest.total_data, manifest.total_parity);
            println!("  fragment size: {} bases, block payload: {} bytes, block size: {} bytes", manifest.fragment_size_bases, manifest.block_payload_size, manifest.block_size);
            println!("  ecc: k={} r={}", manifest.ecc_k, manifest.ecc_r);
            if let Some(gc_min) = manifest.gc_min {
                println!("  constraints: gc [{}, {}], max homopolymer {:?}", gc_min, manifest.gc_max.unwrap_or(0.0), manifest.max_homopolymer);
            }
        }
    }
    Ok(())
}

fn hash_array(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Deterministic pseudorandom input of `size` bytes derived from `seed`.
fn deterministic_input(size: usize, seed: u64) -> Vec<u8> {
    // Simple LCG (deterministic, fast, no dependencies beyond std).
    let mut state: u64 = seed.wrapping_add(0x9E3779B97F4A7C15);
    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let hi = (state >> 32) as u32;
        let word = ((hi >> ((state >> 62) as u32)) ^ hi).rotate_left(hi as u32);
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.truncate(size);
    out
}

fn parse_sizes(s: &str) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
    s.split(',')
        .map(|part| parse_size(part.trim()))
        .collect()
}

fn parse_size(s: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let s = s.to_lowercase();
    let mult = if let Some(stripped) = s.strip_suffix("kib") {
        stripped.parse::<f64>()? * 1024.0
    } else if let Some(stripped) = s.strip_suffix("mib") {
        stripped.parse::<f64>()? * 1024.0 * 1024.0
    } else if let Some(stripped) = s.strip_suffix("gib") {
        stripped.parse::<f64>()? * 1024.0 * 1024.0 * 1024.0
    } else if let Some(stripped) = s.strip_suffix("kb") {
        stripped.parse::<f64>()? * 1000.0
    } else if let Some(stripped) = s.strip_suffix("mb") {
        stripped.parse::<f64>()? * 1000.0 * 1000.0
    } else {
        s.parse::<f64>()?
    };
    Ok(mult.round() as usize)
}
