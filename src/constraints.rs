//! Sequence constraints: GC content and homopolymer limits.
//!
//! These validators apply to DNA base streams (`[Base]`) and are intentionally
//! separate from the codec: the two-bit codec is a pure bijection and does not
//! guarantee any biological property. Callers decide whether a constraint
//! violation is a hard rejection or a reported warning.

use crate::codec::Base;

/// Result of a constraint check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintStatus {
    /// All constraints satisfied.
    Ok,
    /// One or more constraints violated.
    Violated,
}

/// Description of a single violated constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintViolation {
    pub kind: ConstraintKind,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstraintKind {
    GcContent,
    Homopolymer,
    Length,
}

/// Configuration for sequence constraints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstraintConfig {
    /// Allowed GC fraction range [gc_min, gc_max], inclusive.
    pub gc_min: f64,
    pub gc_max: f64,
    /// Maximum allowed run length of a single base (homopolymer limit).
    pub max_homopolymer: usize,
    /// Minimum sequence length in bases (None = unconstrained).
    pub min_length: Option<usize>,
    /// Maximum sequence length in bases (None = unconstrained).
    pub max_length: Option<usize>,
}

impl Default for ConstraintConfig {
    fn default() -> Self {
        Self {
            gc_min: 0.3,
            gc_max: 0.7,
            max_homopolymer: 5,
            min_length: None,
            max_length: None,
        }
    }
}

/// GC fraction of a sequence (G + C bases divided by total).
pub fn gc_content(bases: &[Base]) -> f64 {
    if bases.is_empty() {
        return 0.0;
    }
    let gc = bases.iter().filter(|b| matches!(b.0, 1 | 2)).count() as f64;
    gc / bases.len() as f64
}

/// Length of the longest homopolymer run.
pub fn max_homopolymer_run(bases: &[Base]) -> usize {
    let mut max = 0usize;
    let mut cur = 0usize;
    let mut prev: Option<Base> = None;
    for b in bases {
        if Some(*b) == prev {
            cur += 1;
        } else {
            max = max.max(cur);
            cur = 1;
            prev = Some(*b);
        }
    }
    max = max.max(cur);
    max
}

/// Validate a sequence against `config`. Returns the status and a list of
/// every violated constraint (not just the first).
pub fn validate(bases: &[Base], config: ConstraintConfig) -> (ConstraintStatus, Vec<ConstraintViolation>) {
    let mut violations = Vec::new();

    let gc = gc_content(bases);
    if gc < config.gc_min || gc > config.gc_max {
        violations.push(ConstraintViolation {
            kind: ConstraintKind::GcContent,
            detail: format!("GC content {gc:.4} outside [{}, {}]", config.gc_min, config.gc_max),
        });
    }

    let run = max_homopolymer_run(bases);
    if run > config.max_homopolymer {
        violations.push(ConstraintViolation {
            kind: ConstraintKind::Homopolymer,
            detail: format!("homopolymer run of {run} exceeds limit {}", config.max_homopolymer),
        });
    }

    let len = bases.len();
    if let Some(min) = config.min_length {
        if len < min {
            violations.push(ConstraintViolation {
                kind: ConstraintKind::Length,
                detail: format!("length {len} below minimum {min}"),
            });
        }
    }
    if let Some(max) = config.max_length {
        if len > max {
            violations.push(ConstraintViolation {
                kind: ConstraintKind::Length,
                detail: format!("length {len} above maximum {max}"),
            });
        }
    }

    let status = if violations.is_empty() {
        ConstraintStatus::Ok
    } else {
        ConstraintStatus::Violated
    };
    (status, violations)
}

/// Validate and return only the first violation, if any. Convenience for
/// fail-fast paths.
pub fn validate_first(bases: &[Base], config: ConstraintConfig) -> Result<ConstraintStatus, ConstraintViolation> {
    let (status, violations) = validate(bases, config);
    match violations.into_iter().next() {
        Some(v) => Err(v),
        None => Ok(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bases(s: &str) -> Vec<Base> {
        s.chars().map(|c| match c {
            'A' => Base(0),
            'C' => Base(1),
            'G' => Base(2),
            'T' => Base(3),
            _ => panic!("bad base in test"),
        }).collect()
    }

    #[test]
    fn gc_content_all_at() {
        assert_eq!(gc_content(&bases("AAAA")), 0.0);
    }

    #[test]
    fn gc_content_all_gc() {
        assert!((gc_content(&bases("CCGG")) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn gc_content_empty_is_zero() {
        assert_eq!(gc_content(&[]), 0.0);
    }

    #[test]
    fn homopolymer_runs() {
        assert_eq!(max_homopolymer_run(&bases("A")), 1);
        assert_eq!(max_homopolymer_run(&bases("AAACCAAA")), 3);
        assert_eq!(max_homopolymer_run(&bases("ACGTACGT")), 1);
        assert_eq!(max_homopolymer_run(&bases("")), 0);
    }

    #[test]
    fn valid_sequence_passes() {
        let cfg = ConstraintConfig::default();
        let (status, violations) = validate(&bases("ACGTACGTACGT"), cfg);
        assert_eq!(status, ConstraintStatus::Ok);
        assert!(violations.is_empty());
    }

    #[test]
    fn gc_out_of_range_is_reported() {
        let cfg = ConstraintConfig {
            gc_min: 0.4,
            gc_max: 0.6,
            ..Default::default()
        };
        let (status, violations) = validate(&bases("AAAAAAAA"), cfg);
        assert_eq!(status, ConstraintStatus::Violated);
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| matches!(v.kind, ConstraintKind::GcContent)));
    }

    #[test]
    fn homopolymer_over_limit_is_reported() {
        let cfg = ConstraintConfig {
            max_homopolymer: 2,
            ..Default::default()
        };
        let (status, violations) = validate(&bases("AAACGT"), cfg);
        assert_eq!(status, ConstraintStatus::Violated);
        assert!(violations.iter().any(|v| matches!(v.kind, ConstraintKind::Homopolymer)));
    }

    #[test]
    fn multiple_violations_all_reported() {
        let cfg = ConstraintConfig {
            gc_min: 0.4,
            gc_max: 0.6,
            max_homopolymer: 2,
            ..Default::default()
        };
        let (_status, violations) = validate(&bases("AAAAAAAA"), cfg);
        assert!(violations.len() >= 2);
    }

    #[test]
    fn validate_first_returns_first_violation() {
        let cfg = ConstraintConfig {
            gc_min: 0.4,
            gc_max: 0.6,
            ..Default::default()
        };
        let err = validate_first(&bases("AAAAAAAA"), cfg).unwrap_err();
        assert!(matches!(err.kind, ConstraintKind::GcContent));
    }
}
