//! Report: experiment export to CSV and JSON.
//!
//! Nothing is fabricated: files contain only measured values from actual runs.

use crate::metrics::{ResilienceMetrics, RunMetrics, StorageMetrics};
use serde::Serialize;
use std::fmt::Write;
use std::path::Path;
use std::time::SystemTime;

/// A single benchmark experiment row, suitable for CSV export.
#[derive(Clone, Debug, Default, Serialize)]
pub struct BenchmarkRow {
    pub experiment: String,
    pub seed: u64,
    pub input_bytes: u64,
    pub dna_bases: u64,
    pub fragment_size_bases: u64,
    pub ecc_k: u64,
    pub ecc_r: u64,
    pub archive_format_version: u16,
    pub channel_model_version: String,
    pub substitution_rate: f64,
    pub insertion_rate: f64,
    pub deletion_rate: f64,
    pub dropout_rate: f64,
    pub duplication_rate: f64,
    pub encode_ms: u128,
    pub decode_ms: u128,
    pub simulate_ms: u128,
    pub recover_ms: u128,
    pub total_ms: u128,
    pub encode_mib_s: f64,
    pub decode_mib_s: f64,
    pub fragments_total: u64,
    pub fragments_dropped: u64,
    pub fragments_corrupted: u64,
    pub fragments_recovered: u64,
    pub fragments_unrecovered: u64,
    pub exact_file_recovery: bool,
    pub recovery_rate: f64,
    pub encoded_bytes: u64,
    pub overhead_percent: f64,
    pub peak_rss_kb: Option<u64>,
}

/// CSV serialization for a vector of benchmark rows.
pub fn benchmarks_to_csv(rows: &[BenchmarkRow]) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "experiment,seed,input_bytes,dna_bases,fragment_size_bases,ecc_k,ecc_r,archive_format_version,channel_model_version,substitution_rate,insertion_rate,deletion_rate,dropout_rate,duplication_rate,encode_ms,decode_ms,simulate_ms,recover_ms,total_ms,encode_mib_s,decode_mib_s,fragments_total,fragments_dropped,fragments_corrupted,fragments_recovered,fragments_unrecovered,exact_file_recovery,recovery_rate,encoded_bytes,overhead_percent,peak_rss_kb"
    ).unwrap();
    for row in rows {
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{:.8},{:.8},{:.8},{:.8},{:.8},{},{},{},{},{},{:.6},{:.6},{},{},{},{},{},{},{:.6},{},{:.6},{:?}",
            csv_field(&row.experiment),
            row.seed,
            row.input_bytes,
            row.dna_bases,
            row.fragment_size_bases,
            row.ecc_k,
            row.ecc_r,
            row.archive_format_version,
            csv_field(&row.channel_model_version),
            row.substitution_rate,
            row.insertion_rate,
            row.deletion_rate,
            row.dropout_rate,
            row.duplication_rate,
            row.encode_ms,
            row.decode_ms,
            row.simulate_ms,
            row.recover_ms,
            row.total_ms,
            row.encode_mib_s,
            row.decode_mib_s,
            row.fragments_total,
            row.fragments_dropped,
            row.fragments_corrupted,
            row.fragments_recovered,
            row.fragments_unrecovered,
            row.exact_file_recovery,
            row.recovery_rate,
            row.encoded_bytes,
            row.overhead_percent,
            row.peak_rss_kb,
        )
        .unwrap();
    }
    out
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// JSON summary of a benchmark run set.
#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkSummary {
    pub schema_version: String,
    pub application_version: String,
    pub target_os: String,
    pub target_arch: String,
    pub build_profile: String,
    pub created_at: String,
    pub rows: Vec<BenchmarkRow>,
    pub count: usize,
    pub exact_recoveries: usize,
    pub mean_recovery_rate: f64,
}

impl BenchmarkSummary {
    pub fn new(rows: Vec<BenchmarkRow>) -> Self {
        let exact_recoveries = rows.iter().filter(|r| r.exact_file_recovery).count();
        let mean_recovery_rate = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|r| r.recovery_rate).sum::<f64>() / rows.len() as f64
        };
        Self {
            schema_version: "dnaforge-benchmark-v1".into(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            target_os: std::env::consts::OS.into(),
            target_arch: std::env::consts::ARCH.into(),
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
            created_at: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default(),
            count: rows.len(),
            exact_recoveries,
            mean_recovery_rate,
            rows,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}

/// Export a full run result to JSON.
#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    pub run_metrics: RunMetrics,
    pub resilience: ResilienceMetrics,
    pub storage: StorageMetrics,
}

impl RunReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}

/// Write benchmark rows as CSV to `path`.
pub fn write_csv(path: &Path, rows: &[BenchmarkRow]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, benchmarks_to_csv(rows))
}

/// Write a benchmark summary as JSON to `path`.
pub fn write_json_summary(path: &Path, summary: &BenchmarkSummary) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, summary.to_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_header_and_row() {
        let rows = vec![BenchmarkRow {
            experiment: "t1".into(),
            seed: 1,
            input_bytes: 100,
            dna_bases: 400,
            fragment_size_bases: 64,
            ecc_k: 4,
            ecc_r: 2,
            encode_ms: 5,
            decode_ms: 3,
            simulate_ms: 2,
            recover_ms: 4,
            total_ms: 14,
            encode_mib_s: 10.0,
            decode_mib_s: 20.0,
            fragments_total: 10,
            fragments_dropped: 1,
            fragments_corrupted: 0,
            fragments_recovered: 9,
            fragments_unrecovered: 0,
            exact_file_recovery: true,
            recovery_rate: 0.9,
            encoded_bytes: 150,
            overhead_percent: 50.0,
            peak_rss_kb: Some(1024),
            ..Default::default()
        }];
        let csv = benchmarks_to_csv(&rows);
        assert!(csv.starts_with("experiment,seed,"));
        assert!(csv.contains("t1,1,100,400,64,4,2"));
        assert!(csv.contains("true"));
    }

    #[test]
    fn csv_escapes_delimiters_quotes_and_newlines() {
        let row = BenchmarkRow {
            experiment: "sample,\"one\"\nnext".into(),
            ..Default::default()
        };
        let csv = benchmarks_to_csv(&[row]);
        assert!(csv.contains("\"sample,\"\"one\"\"\nnext\""));
    }

    #[test]
    fn json_summary_includes_schema_and_environment() {
        let json = BenchmarkSummary::new(Vec::new()).to_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema_version"], "dnaforge-benchmark-v1");
        assert_eq!(value["application_version"], env!("CARGO_PKG_VERSION"));
        assert!(value["target_os"].is_string());
        assert!(value["target_arch"].is_string());
    }

    #[test]
    fn summary_aggregates() {
        let rows = vec![
            BenchmarkRow {
                exact_file_recovery: true,
                recovery_rate: 1.0,
                ..Default::default()
            },
            BenchmarkRow {
                exact_file_recovery: false,
                recovery_rate: 0.5,
                ..Default::default()
            },
        ];
        let summary = BenchmarkSummary::new(rows);
        assert_eq!(summary.count, 2);
        assert_eq!(summary.exact_recoveries, 1);
        assert!((summary.mean_recovery_rate - 0.75).abs() < 1e-9);
    }
}
