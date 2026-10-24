//! Command-line interface for DNAForge.
//!
//! Commands: encode, decode, validate, simulate, recover, benchmark, inspect.

use clap::{Parser, Subcommand};
use dnaforge::channel::ChannelConfig;
use dnaforge::constraints::ConstraintConfig;
use dnaforge::pipeline::{
    PipelineConfig, benchmark_once, decode_pipeline, encode_pipeline, recover_pipeline,
    simulate_pipeline, validate_pipeline,
};
use dnaforge::report::{BenchmarkSummary, write_csv, write_json_summary};
use dnaforge::storage::{ArchiveManifest, read_file, sha256_hex, write_file};
use sha2::Digest;
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser)]
#[command(
    name = "dnaforge",
    version = "0.1.0",
    about = "DNAForge — computational DNA data storage baseline"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Encode a file into a DNAForge archive (bytes -> DNA -> fragments).
    Encode {
        #[command(flatten)]
        input: InputArgs,
        #[command(flatten)]
        output: OutputArgs,
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
        input: InputArgs,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Validate an archive's structure, checksums, and constraints.
    Validate {
        #[command(flatten)]
        input: InputArgs,
    },
    /// Simulate DNA channel errors on an archive.
    Simulate {
        #[command(flatten)]
        input: InputArgs,
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
        input: InputArgs,
        #[command(flatten)]
        output: OutputArgs,
        /// Original file to compare hashes for exact-recovery verification.
        #[arg(long)]
        original: Option<PathBuf>,
    },
    /// Benchmark encode/decode/recover and export CSV or JSON.
    Benchmark {
        /// File to benchmark; not read when --synthetic-sizes is supplied.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Output report path (.csv or .json).
        #[arg(long)]
        report: PathBuf,
        /// Generate deterministic inputs of these sizes instead of using INPUT.
        #[arg(long, value_name = "SIZE[,SIZE...]")]
        synthetic_sizes: Option<String>,
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
        input: InputArgs,
    },
}

#[derive(clap::Args)]
struct InputArgs {
    /// Input file or archive directory.
    #[arg(value_name = "INPUT")]
    input: PathBuf,
}

#[derive(clap::Args)]
struct OutputArgs {
    /// Output file or directory.
    #[arg(short, long, value_name = "OUTPUT")]
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
        Commands::Encode {
            input: source,
            output,
            fragment_size,
            ecc_k,
            ecc_r,
            strict,
            gc_min,
            gc_max,
            max_homopolymer,
        } => {
            let input = read_file(&source.input)?;
            let config = PipelineConfig {
                fragment_size_bases: fragment_size,
                ecc_k,
                ecc_r,
                strict_constraints: strict,
                constraint_config: ConstraintConfig {
                    gc_min,
                    gc_max,
                    max_homopolymer,
                    ..Default::default()
                },
                ..Default::default()
            };
            let archive = encode_pipeline(&input, &config)?;
            dnaforge::pipeline::write_encoded_archive(&archive, &output.output)?;
            println!(
                "encoded {} bytes -> {} DNA bases -> {} fragments ({} data + {} parity) in {}",
                input.len(),
                archive.manifest.dna_bases,
                archive.manifest.fragment_count,
                archive.manifest.total_data,
                archive.manifest.total_parity,
                output.output.display(),
            );
        }
        Commands::Decode {
            input: source,
            output,
        } => {
            let (bytes, _manifest) = decode_pipeline(&source.input)?;
            write_file(&output.output, &bytes)?;
            println!(
                "decoded {} -> {} bytes (sha256 {})",
                source.input.display(),
                bytes.len(),
                sha256_hex(&bytes)
            );
        }
        Commands::Validate { input: source } => {
            let report = validate_pipeline(&source.input)?;
            println!(
                "archive {}: {} blocks total, {} parsed, {} corrupted, {} missing, {} constraint violation(s)",
                source.input.display(),
                report.blocks_total,
                report.blocks_parsed,
                report.blocks_corrupted,
                report.blocks_missing,
                report.constraint_violations.len(),
            );
            for v in &report.constraint_violations {
                println!("  constraint violation: {:?} - {}", v.kind, v.detail);
            }
            if report.blocks_corrupted > 0
                || report.blocks_missing > 0
                || !report.constraint_violations.is_empty()
            {
                exit(2);
            }
        }
        Commands::Simulate {
            input: source,
            output,
            sub,
            ins,
            del,
            dropout,
            dup,
            seed,
        } => {
            let config = ChannelConfig {
                substitution_rate: sub,
                insertion_rate: ins,
                deletion_rate: del,
                dropout_rate: dropout,
                duplication_rate: dup,
                seed,
            };
            let report = simulate_pipeline(&source.input, &output, &config)?;
            println!(
                "simulated on {}: {} fragments total, {} dropped, {} corrupted events, seed {}",
                source.input.display(),
                report.fragments_total,
                report.fragments_dropped,
                report.fragments_corrupted,
                seed,
            );
            println!(
                "  substitutions {} insertions {} deletions {} dropouts {} duplications {}",
                report.stats.substitutions,
                report.stats.insertions,
                report.stats.deletions,
                report.stats.dropouts,
                report.stats.duplications,
            );
        }
        Commands::Recover {
            input: source,
            output,
            original,
        } => {
            let original_hash = match &original {
                Some(path) => {
                    let data = read_file(path)?;
                    Some(hash_array(&data))
                }
                None => None,
            };
            let (bytes, report) = recover_pipeline(&source.input, original_hash)?;
            write_file(&output.output, &bytes)?;
            println!(
                "recovered {} bytes from {} (exact match: {:?})",
                bytes.len(),
                source.input.display(),
                report.exact_match,
            );
            println!(
                "  blocks: {} parsed, {} corrupted, {} missing; data erasures {}; stage ms: parse {} ecc {} reassemble {} decode {} verify {} total {}",
                report.blocks_parsed,
                report.blocks_corrupted,
                report.blocks_missing,
                report.data_erasures,
                report.stage_timing.parse_ms,
                report.stage_timing.ecc_ms,
                report.stage_timing.reassemble_ms,
                report.stage_timing.decode_ms,
                report.stage_timing.verify_ms,
                report.stage_timing.total_ms,
            );
            if report.exact_match != Some(true) {
                exit(3);
            }
        }
        Commands::Benchmark {
            input: input_path,
            report,
            synthetic_sizes,
            seeds,
            ecc_k,
            ecc_r,
            fragment_size,
            sub,
            ins,
            del,
            dropout,
            dup,
            seed,
        } => {
            let samples: Vec<(String, Vec<u8>)> = match synthetic_sizes {
                Some(sizes) => parse_sizes(&sizes)?
                    .into_iter()
                    .map(|size| {
                        (
                            format!("synthetic-{size}-input-seed-{seed}"),
                            deterministic_input(size, seed),
                        )
                    })
                    .collect(),
                None => vec![("input-file".to_string(), read_file(&input_path)?)],
            };
            let seed_list: Vec<u64> = seeds
                .split(',')
                .map(|s| s.trim().parse::<u64>())
                .collect::<Result<_, _>>()?;
            if seed_list.is_empty() {
                return Err("at least one benchmark seed is required".into());
            }
            let config = PipelineConfig {
                ecc_k,
                ecc_r,
                fragment_size_bases: fragment_size,
                channel: ChannelConfig {
                    substitution_rate: sub,
                    insertion_rate: ins,
                    deletion_rate: del,
                    dropout_rate: dropout,
                    duplication_rate: dup,
                    seed,
                },
                ..Default::default()
            };
            let mut rows = Vec::new();
            for (label, data) in &samples {
                for &s in &seed_list {
                    let row = benchmark_once(data, label, s, &config)?;
                    rows.push(row);
                }
            }
            match report.extension().and_then(|extension| extension.to_str()) {
                Some("json") => {
                    let summary = BenchmarkSummary::new(rows.clone());
                    write_json_summary(&report, &summary)?;
                    let rows_len = rows.len();
                    println!(
                        "benchmark: {} experiments -> {}",
                        rows_len,
                        report.display()
                    );
                }
                Some("csv") => {
                    let rows_len = rows.len();
                    write_csv(&report, &rows)?;
                    println!(
                        "benchmark: {} experiments -> {}",
                        rows_len,
                        report.display()
                    );
                }
                _ => return Err("benchmark report extension must be .csv or .json".into()),
            }
        }
        Commands::Inspect { input: source } => {
            let manifest_path = source.input.join("manifest.json");
            let manifest: ArchiveManifest =
                serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)
                    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            println!("archive: {}", source.input.display());
            println!("  format: {} v{}", manifest.format, manifest.version);
            println!(
                "  original: {} bytes, sha256 {}",
                manifest.original_size_bytes, manifest.original_sha256
            );
            println!("  dna bases: {}", manifest.dna_bases);
            println!(
                "  fragments: {} total ({} data + {} parity)",
                manifest.fragment_count, manifest.total_data, manifest.total_parity
            );
            println!(
                "  fragment size: {} bases, block payload: {} bytes, block size: {} bytes",
                manifest.fragment_size_bases, manifest.block_payload_size, manifest.block_size
            );
            println!("  ecc: k={} r={}", manifest.ecc_k, manifest.ecc_r);
            if let Some(gc_min) = manifest.gc_min {
                println!(
                    "  constraints: gc [{}, {}], max homopolymer {:?}",
                    gc_min,
                    manifest.gc_max.unwrap_or(0.0),
                    manifest.max_homopolymer
                );
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
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let hi = (state >> 32) as u32;
        let word = ((hi >> ((state >> 62) as u32)) ^ hi).rotate_left(hi);
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.truncate(size);
    out
}

fn parse_sizes(s: &str) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
    s.split(',').map(|part| parse_size(part.trim())).collect()
}

fn parse_size(s: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let value = s.trim().to_ascii_lowercase();
    let (digits, multiplier) = if let Some(digits) = value.strip_suffix("kib") {
        (digits, 1024u64)
    } else if let Some(digits) = value.strip_suffix("mib") {
        (digits, 1024u64.pow(2))
    } else if let Some(digits) = value.strip_suffix("gib") {
        (digits, 1024u64.pow(3))
    } else if let Some(digits) = value.strip_suffix("kb") {
        (digits, 1000u64)
    } else if let Some(digits) = value.strip_suffix("mb") {
        (digits, 1000u64.pow(2))
    } else if let Some(digits) = value.strip_suffix("gb") {
        (digits, 1000u64.pow(3))
    } else {
        (value.as_str(), 1u64)
    };
    let count = digits.parse::<u64>()?;
    let bytes = count
        .checked_mul(multiplier)
        .ok_or_else(|| std::io::Error::other("benchmark size overflows u64"))?;
    usize::try_from(bytes)
        .map_err(|_| std::io::Error::other("benchmark size exceeds platform limits").into())
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn parses_decimal_and_binary_units_without_float_rounding() {
        assert_eq!(parse_size("12").unwrap(), 12);
        assert_eq!(parse_size("2KiB").unwrap(), 2 * 1024);
        assert_eq!(parse_size("3MB").unwrap(), 3_000_000);
    }

    #[test]
    fn rejects_invalid_or_overflowing_sizes() {
        for value in ["-1", "NaN", "1.5KiB", "18446744073709551615GiB"] {
            assert!(parse_size(value).is_err(), "{value}");
        }
    }
}
