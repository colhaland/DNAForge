//! Report: experiment export to CSV and JSON.
//!
//! Nothing is fabricated: files contain only measured values from actual runs.

use crate::metrics::{ResilienceMetrics, RunMetrics, StorageMetrics};
use serde::Serialize;
use std::fmt::Write;
use std::path::Path;
use std::time::SystemTime;

/// A single benchmark experiment row, suitable for CSV export.
#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkRow {
    pub experiment: String,
    pub seed: u64,
    pub input_bytes: u64,
    pub dna_bases: u64,
    pub fragment_size_bases: u64,
    pub ecc_k: u64,
    pub ecc_r: u64,
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
        "experiment,seed,input_bytes,dna_bases,fragment_size_bases,ecc_k,ecc_r,encode_ms,decode_ms,simulate_ms,recover_ms,total_ms,encode_mib_s,decode_mib_s,fragments_total,fragments_dropped,fragments_corrupted,fragments_recovered,fragments_unrecovered,exact_file_recovery,recovery_rate,encoded_bytes,overhead_percent,peak_rss_kb"
    ).unwrap();
    for row in rows {
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{},{},{},{},{},{},{},{:.6},{:.6},{:?}",
            row.experiment,
            row.seed,
            row.input_bytes,
            row.dna_bases,
            row.fragment_size_bases,
            row.ecc_k,
            row.ecc_r,
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
        ).unwrap();
    }
    out
}

/// JSON summary of a benchmark run set.
#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkSummary {
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
        }];
        let csv = benchmarks_to_csv(&rows);
        assert!(csv.starts_with("experiment,seed,"));
        assert!(csv.contains("t1,1,100,400,64,4,2"));
        assert!(csv.contains("true"));
    }

    #[test]
    fn summary_aggregates() {
        let rows = vec![
            BenchmarkRow { exact_file_recovery: true, recovery_rate: 1.0, ..Default::default() },
            BenchmarkRow { exact_file_recovery: false, recovery_rate: 0.5, ..Default::default() },
        ];
        let summary = BenchmarkSummary::new(rows);
        assert_eq!(summary.count, 2);
        assert_eq!(summary.exact_recoveries, 1);
        assert!((summary.mean_recovery_rate - 0.75).abs() < 1e-9);
    }
}

impl Default for BenchmarkRow {
    fn default() -> Self {
        Self {
            experiment: String::new(),
            seed: 0,
            input_bytes: 0,
            dna_bases: 0,
            fragment_size_bases: 0,
            ecc_k: 0,
            ecc_r: 0,
            encode_ms: 0,
            decode_ms: 0,
            simulate_ms: 0,
            recover_ms: 0,
            total_ms: 0,
            encode_mib_s: 0.0,
            decode_mib_s: 0.0,
            fragments_total: 0,
            fragments_dropped: 0,
            fragments_corrupted: 0,
            fragments_recovered: 0,
            fragments_unrecovered: 0,
            exact_file_recovery: false,
            recovery_rate: 0.0,
            encoded_bytes: 0,
            overhead_percent: 0.0,
            peak_rss_kb: None,
        }
    }
}
