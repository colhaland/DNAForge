//! Metrics: trustworthy performance and resilience measurements.
//!
//! All times are wall-clock measurements from actual executions; nothing here
//! is fabricated or projected. Throughput is computed from real bytes
//! processed and real elapsed time.

use std::time::Instant;

/// Aggregated timing for a pipeline stage.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct StageMetric {
    pub name: String,
    pub elapsed_ms: u128,
    pub bytes_processed: u64,
}

impl StageMetric {
    pub fn throughput_mib_per_s(&self) -> f64 {
        if self.elapsed_ms == 0 {
            return 0.0;
        }
        let seconds = self.elapsed_ms as f64 / 1000.0;
        self.bytes_processed as f64 / (1024.0 * 1024.0) / seconds
    }
}

/// Result of a single measured run.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct RunMetrics {
    pub encode: StageMetric,
    pub decode: StageMetric,
    pub fragment: StageMetric,
    pub simulate: StageMetric,
    pub recover: StageMetric,
    pub verify: StageMetric,
    pub total_ms: u128,
}

/// Resilience outcome of a run.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct ResilienceMetrics {
    pub fragments_total: u64,
    pub fragments_dropped: u64,
    pub fragments_corrupted: u64,
    pub fragments_recovered: u64,
    pub fragments_unrecovered: u64,
    pub exact_file_recovery: bool,
    pub recovery_rate_fragments: f64,
}

impl ResilienceMetrics {
    pub fn recovery_rate_fragments(&self) -> f64 {
        if self.fragments_total == 0 {
            return 0.0;
        }
        self.fragments_recovered as f64 / self.fragments_total as f64
    }
}

/// Storage efficiency numbers.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct StorageMetrics {
    pub original_bytes: u64,
    pub dna_bases: u64,
    pub encoded_bytes: u64,
    pub overhead_percent: f64,
    pub bases_per_byte: f64,
}

impl StorageMetrics {
    pub fn new(original_bytes: u64, dna_bases: u64, encoded_bytes: u64) -> Self {
        let overhead_percent = if original_bytes == 0 {
            0.0
        } else {
            (encoded_bytes as f64 - original_bytes as f64) / original_bytes as f64 * 100.0
        };
        let bases_per_byte = if original_bytes == 0 {
            0.0
        } else {
            dna_bases as f64 / original_bytes as f64
        };
        Self { original_bytes, dna_bases, encoded_bytes, overhead_percent, bases_per_byte }
    }
}

/// Peak RSS in kilobytes, read from /proc/self/status (Linux). Returns None on
/// other platforms or if unavailable.
pub fn peak_rss_kb() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: u64 = rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .ok()?;
            return Some(kb);
        }
    }
    None
}

/// Measures a closure, returning its output and the elapsed wall-clock time.
pub fn measure<T, F: FnOnce() -> T>(f: F) -> (T, u128) {
    let start = Instant::now();
    let out = f();
    let elapsed = start.elapsed().as_millis();
    (out, elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throughput_zero_elapsed_is_zero() {
        let m = StageMetric { name: "x".into(), elapsed_ms: 0, bytes_processed: 100 };
        assert_eq!(m.throughput_mib_per_s(), 0.0);
    }

    #[test]
    fn throughput_calculation() {
        let m = StageMetric { name: "x".into(), elapsed_ms: 1000, bytes_processed: 2 * 1024 * 1024 };
        assert!((m.throughput_mib_per_s() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn overhead_percent() {
        let s = StorageMetrics::new(100, 400, 150);
        assert!((s.overhead_percent - 50.0).abs() < 1e-9);
        assert!((s.bases_per_byte - 4.0).abs() < 1e-9);
    }

    #[test]
    fn recovery_rate() {
        let r = ResilienceMetrics { fragments_total: 10, fragments_recovered: 8, ..Default::default() };
        assert!((r.recovery_rate_fragments() - 0.8).abs() < 1e-9);
        let empty = ResilienceMetrics::default();
        assert_eq!(empty.recovery_rate_fragments(), 0.0);
    }
}
