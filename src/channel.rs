//! Channel simulator: configurable, reproducible DNA channel errors.
//!
//! Models substitution, insertion, deletion, fragment dropout, and duplication
//! with independent per-base / per-fragment probabilities and a seeded PRNG.
//! All simulations are deterministic for a fixed seed and fragment input.
//!
//! This is a computational channel model, not a validated laboratory model:
//! real synthesis/sequencing errors are sequence-context and technology
//! dependent.

use crate::codec::Base;
use rand::{Rng, SeedableRng};

/// Configuration for the DNA channel simulator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelConfig {
    /// Probability of substituting each base (0..=1).
    pub substitution_rate: f64,
    /// Probability of inserting a random base before each base (0..=1).
    pub insertion_rate: f64,
    /// Probability of deleting each base (0..=1).
    pub deletion_rate: f64,
    /// Probability of dropping an entire fragment (0..=1).
    pub dropout_rate: f64,
    /// Probability of duplicating an emitted fragment (0..=1).
    pub duplication_rate: f64,
    /// Random seed; identical seed + input => identical output.
    pub seed: u64,
}

impl ChannelConfig {
    pub fn validate(&self) -> Result<(), ChannelError> {
        for (name, value) in [
            ("substitution_rate", self.substitution_rate),
            ("insertion_rate", self.insertion_rate),
            ("deletion_rate", self.deletion_rate),
            ("dropout_rate", self.dropout_rate),
            ("duplication_rate", self.duplication_rate),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(ChannelError::InvalidRate { name, value });
            }
        }
        Ok(())
    }
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            substitution_rate: 0.0,
            insertion_rate: 0.0,
            deletion_rate: 0.0,
            dropout_rate: 0.0,
            duplication_rate: 0.0,
            seed: 0,
        }
    }
}

/// A single recorded error event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorEvent {
    Substitution {
        fragment_id: u64,
        position: usize,
        from: Base,
        to: Base,
    },
    Insertion {
        fragment_id: u64,
        position: usize,
        base: Base,
    },
    Deletion {
        fragment_id: u64,
        position: usize,
        base: Base,
    },
    Dropout { fragment_id: u64 },
    Duplication { fragment_id: u64 },
}

/// Per-run error statistics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorStats {
    pub substitutions: u64,
    pub insertions: u64,
    pub deletions: u64,
    pub dropouts: u64,
    pub duplications: u64,
    pub input_bases: usize,
    pub output_bases: usize,
    pub input_fragments: usize,
    pub output_fragments: usize,
}

/// Requested vs actual error information for reporting.
#[derive(Clone, Debug, PartialEq)]
pub struct SimulationReport {
    pub config: ChannelConfig,
    pub stats: ErrorStats,
    pub events: Vec<ErrorEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelError {
    InvalidRate { name: &'static str, value: f64 },
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::InvalidRate { name, value } => {
                write!(f, "channel: {name} must be in [0, 1], got {value}")
            }
        }
    }
}

impl std::error::Error for ChannelError {}

/// Simulate the channel over a vector of fragments (each a vector of bases).
/// Fragments are identified by their index in the input slice used as the
/// fragment id.
pub fn simulate(fragments: &[Vec<Base>], config: &ChannelConfig) -> Result<SimulationOutput, ChannelError> {
    config.validate()?;
    let mut rng = rand::rngs::StdRng::seed_from_u64(config.seed);
    let mut output: Vec<Option<Vec<Base>>> = Vec::with_capacity(fragments.len());
    let mut events = Vec::new();
    let mut stats = ErrorStats {
        input_fragments: fragments.len(),
        input_bases: fragments.iter().map(|f| f.len()).sum(),
        ..Default::default()
    };

    for (frag_id, fragment) in fragments.iter().enumerate() {
        // Dropout: whole fragment lost.
        if rng.gen_bool(config.dropout_rate) {
            stats.dropouts += 1;
            events.push(ErrorEvent::Dropout { fragment_id: frag_id as u64 });
            output.push(None);
            continue;
        }

        let (mut mutated, (mut events2, estats)) = process_sequence(fragment, &mut rng, frag_id as u64, config);
        stats.substitutions += estats.substitutions;
        stats.insertions += estats.insertions;
        stats.deletions += estats.deletions;
        stats.output_bases += mutated.len();
        events.append(&mut events2);

        output.push(Some(mutated.clone()));

        // Duplication: emit an extra identical copy.
        if rng.gen_bool(config.duplication_rate) {
            stats.duplications += 1;
            events.push(ErrorEvent::Duplication { fragment_id: frag_id as u64 });
            output.push(Some(mutated));
        }
    }

    stats.output_fragments = output.iter().filter(|o| o.is_some()).count();

    Ok(SimulationOutput {
        fragments: output,
        events,
        stats,
    })
}

struct EventStats {
    substitutions: u64,
    insertions: u64,
    deletions: u64,
}

impl Default for EventStats {
    fn default() -> Self {
        Self { substitutions: 0, insertions: 0, deletions: 0 }
    }
}

fn process_sequence(
    bases: &[Base],
    rng: &mut impl Rng,
    fragment_id: u64,
    config: &ChannelConfig,
) -> (Vec<Base>, (Vec<ErrorEvent>, EventStats)) {
    let mut out = Vec::with_capacity(bases.len() + 2);
    let mut events = Vec::new();
    let mut stats = EventStats::default();
    let mut position: usize = 0;
    for &b in bases {
        // Insertion before this base.
        if rng.gen_bool(config.insertion_rate) {
            let ins = random_base_different(rng, b);
            events.push(ErrorEvent::Insertion {
                fragment_id,
                position,
                base: ins,
            });
            out.push(ins);
            stats.insertions += 1;
            position += 1;
        }

        // Deletion of this base.
        if rng.gen_bool(config.deletion_rate) {
            events.push(ErrorEvent::Deletion {
                fragment_id,
                position,
                base: b,
            });
            stats.deletions += 1;
            continue;
        }

        let mut base = b;
        if rng.gen_bool(config.substitution_rate) {
            let sub = random_base_different(rng, b);
            events.push(ErrorEvent::Substitution {
                fragment_id,
                position,
                from: b,
                to: sub,
            });
            base = sub;
            stats.substitutions += 1;
        }
        out.push(base);
        position += 1;
    }

    (out, (events, stats))
}

fn random_base(rng: &mut impl Rng) -> Base {
    Base(rng.gen_range(0..4))
}

fn random_base_different(rng: &mut impl Rng, exclude: Base) -> Base {
    loop {
        let b = random_base(rng);
        if b != exclude {
            return b;
        }
    }
}

/// Output of a simulation run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationOutput {
    pub fragments: Vec<Option<Vec<Base>>>,
    pub events: Vec<ErrorEvent>,
    pub stats: ErrorStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_a(n: usize) -> Vec<Base> {
        vec![Base(0); n]
    }

    #[test]
    fn zero_rates_is_identity() {
        let frags = vec![all_a(10), vec![Base(1); 5]];
        let cfg = ChannelConfig::default();
        let out = simulate(&frags, &cfg).unwrap();
        assert_eq!(out.fragments.len(), 2);
        assert_eq!(out.fragments[0], Some(all_a(10)));
        assert_eq!(out.fragments[1], Some(vec![Base(1); 5]));
        assert!(out.events.is_empty());
    }

    #[test]
    fn deterministic_seed() {
        let frags = vec![all_a(50)];
        let cfg = ChannelConfig {
            substitution_rate: 0.2,
            insertion_rate: 0.1,
            deletion_rate: 0.1,
            dropout_rate: 0.3,
            duplication_rate: 0.3,
            seed: 12345,
        };
        let a = simulate(&frags, &cfg).unwrap();
        let b = simulate(&frags, &cfg).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_seed_differs_likely() {
        let frags = vec![all_a(100)];
        let c1 = ChannelConfig { substitution_rate: 0.5, seed: 1, ..Default::default() };
        let c2 = ChannelConfig { substitution_rate: 0.5, seed: 2, ..Default::default() };
        let a = simulate(&frags, &c1).unwrap();
        let b = simulate(&frags, &c2).unwrap();
        assert_ne!(a.fragments, b.fragments);
    }

    #[test]
    fn full_dropout() {
        let frags = vec![all_a(10), all_a(10)];
        let cfg = ChannelConfig { dropout_rate: 1.0, seed: 7, ..Default::default() };
        let out = simulate(&frags, &cfg).unwrap();
        assert_eq!(out.fragments, vec![None, None]);
        assert_eq!(out.stats.dropouts, 2);
        assert_eq!(out.stats.output_fragments, 0);
    }

    #[test]
    fn duplication_keeps_identity() {
        let frags = vec![vec![Base(0), Base(1), Base(2)]];
        let cfg = ChannelConfig {
            duplication_rate: 1.0,
            seed: 3,
            ..Default::default()
        };
        let out = simulate(&frags, &cfg).unwrap();
        assert_eq!(out.fragments.len(), 2);
        assert_eq!(out.fragments[0], Some(frags[0].clone()));
        assert_eq!(out.fragments[1], Some(frags[0].clone()));
        assert_eq!(out.stats.duplications, 1);
    }

    #[test]
    fn indels_change_length() {
        let frags = vec![all_a(100)];
        let cfg = ChannelConfig {
            insertion_rate: 0.5,
            deletion_rate: 0.5,
            seed: 9,
            ..Default::default()
        };
        let out = simulate(&frags, &cfg).unwrap();
        let out_bases = out.fragments[0].as_ref().unwrap().len();
        assert_ne!(out_bases, 100);
        assert!(out.stats.insertions > 0 || out.stats.deletions > 0);
    }

    #[test]
    fn invalid_rate_rejected() {
        let cfg = ChannelConfig { substitution_rate: 1.5, ..Default::default() };
        let err = simulate(&vec![all_a(4)], &cfg).unwrap_err();
        assert!(matches!(err, ChannelError::InvalidRate { .. }));
    }
}
