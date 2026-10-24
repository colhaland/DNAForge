//! DNAForge library: computational DNA data storage baseline.
//!
//! Modules:
//!   codec      - byte <-> DNA two-bit encoding
//!   constraints - GC content and homopolymer validation
//!   ecc        - Reed-Solomon erasure coding over GF(256)
//!   fragment   - versioned fragment blocks with SHA-256 integrity
//!   channel    - reproducible DNA channel error simulator
//!   recovery   - validation, erasure correction, reconstruction
//!   metrics    - performance and resilience measurements
//!   storage    - archive reading/writing
//!   report     - CSV/JSON experiment export
//!   pipeline   - end-to-end orchestration

pub mod channel;
pub mod codec;
pub mod constraints;
pub mod ecc;
pub mod fragment;
pub mod metrics;
pub mod pipeline;
pub mod recovery;
pub mod report;
pub mod storage;
