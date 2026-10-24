//! Error correction: Reed-Solomon erasure coding over GF(256).
//!
//! This is a genuine error-correcting code, not a checksum. It provides
//! *erasure correction*: when the positions of lost/corrupt fragments are
//! known (detected by the checksum layer in [`crate::recovery`]), the missing
//! fragment contents are reconstructed from the surviving fragments and the
//! Reed-Solomon parity fragments.
//!
//! Construction: RS[n, k] over GF(256) with primitive polynomial 0x11d and
//! generator alpha = 2. Data fragments are message symbols; parity fragments
//! are polynomial evaluations at the remaining n - k points. Any k of the n
//! symbols suffice to reconstruct the whole codeword (MDS property), so up to
//! r = n - k erasures can be corrected.
//!
//! Each byte position within a fragment block is an independent GF(256)
//! symbol, so a block of N bytes encodes as N parallel RS symbols.
//!
//! Limitation: n = k + r <= 255 because GF(256) has 255 nonzero evaluation
//! points. This code corrects erasures only; symbol-level substitution
//! correction (errors at unknown positions) is not implemented. Within a
//! fragment, substitutions/indels are detected by SHA-256 (see
//! [`crate::recovery`]) and then treated as erasures.

/// GF(256) exponent and log tables for primitive polynomial 0x11d, generator 2.
struct GfTables {
    exp: [u8; 512],
    log: [u32; 256],
}

impl GfTables {
    fn new() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u32; 256];
        let mut x: u32 = 1;
        for i in 0..255 {
            exp[i as usize] = x as u8;
            log[x as usize] = i;
            // multiply x by alpha (2) modulo primitive polynomial 0x11d
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11d;
            }
        }
        for i in 255..512 {
            exp[i as usize] = exp[(i - 255) as usize];
        }
        log[0] = u32::MAX; // log(0) undefined
        Self { exp, log }
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.exp[(self.log[a as usize] + self.log[b as usize]) as usize]
    }

    fn div(&self, a: u8, b: u8) -> u8 {
        if a == 0 {
            return 0;
        }
        if b == 0 {
            panic!("gf: division by zero");
        }
        self.exp[((self.log[a as usize] + 255 - self.log[b as usize]) % 255) as usize]
    }
}

static TABLES: std::sync::LazyLock<GfTables> = std::sync::LazyLock::new(GfTables::new);

fn gf_mul(a: u8, b: u8) -> u8 {
    TABLES.mul(a, b)
}

fn gf_div(a: u8, b: u8) -> u8 {
    TABLES.div(a, b)
}

/// Alpha^j (generator to the j-th power), j in 0..254.
fn alpha_pow(j: usize) -> u8 {
    TABLES.exp[j % 255]
}

/// Errors produced by the Reed-Solomon coder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReedSolomonError {
    /// Too many erasures: only `have` symbols known, `need` required.
    TooFewErasureSymbols { have: usize, need: usize },
    /// Fragment block count does not match k + r.
    BlockCountMismatch { expected: usize, got: usize },
    /// Fragment block size does not match the coder configuration.
    BlockSizeMismatch { expected: usize, got: usize },
    /// Total fragments n = k + r would exceed 255 GF(256) points.
    TooManyFragments { k: usize, r: usize },
}

impl std::fmt::Display for ReedSolomonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReedSolomonError::TooFewErasureSymbols { have, need } => {
                write!(
                    f,
                    "reed-solomon: only {have} symbols known, {need} required"
                )
            }
            ReedSolomonError::BlockCountMismatch { expected, got } => {
                write!(f, "reed-solomon: expected {expected} blocks, got {got}")
            }
            ReedSolomonError::BlockSizeMismatch { expected, got } => {
                write!(f, "reed-solomon: expected block size {expected}, got {got}")
            }
            ReedSolomonError::TooManyFragments { k, r } => {
                write!(
                    f,
                    "reed-solomon: k + r = {} exceeds 255 GF(256) points",
                    k + r
                )
            }
        }
    }
}

impl std::error::Error for ReedSolomonError {}

/// Reed-Solomon erasure coder configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReedSolomonConfig {
    /// Number of data fragments (message symbols per byte position).
    pub k: usize,
    /// Number of parity fragments (erasures tolerable).
    pub r: usize,
}

impl ReedSolomonConfig {
    pub fn new(k: usize, r: usize) -> Result<Self, ReedSolomonError> {
        if k == 0 || k.checked_add(r).is_none_or(|n| n > 255) {
            return Err(ReedSolomonError::TooManyFragments { k, r });
        }
        Ok(Self { k, r })
    }

    pub fn n(self) -> usize {
        self.k + self.r
    }
}

/// Reed-Solomon erasure coder.
pub struct ReedSolomon {
    config: ReedSolomonConfig,
    /// eval_points[j] = alpha^j for j in 0..n
    eval_points: Vec<u8>,
    /// T[i][j] = Lagrange coefficient L_i(alpha^(k+j)) used to compute parity
    /// fragment j from data fragment i: parity_j[b] = sum_i data_i[b] * T[i][j].
    t: Vec<Vec<u8>>,
}

impl ReedSolomon {
    pub fn new(config: ReedSolomonConfig) -> Result<Self, ReedSolomonError> {
        let n = config.n();
        let mut eval_points = Vec::with_capacity(n);
        for j in 0..n {
            eval_points.push(alpha_pow(j));
        }

        let k = config.k;
        // T[i][j] = L_i(alpha^(k+j)), L_i(x) = prod_{m != i} (x - alpha^m) / (alpha^i - alpha^m)
        let mut t = vec![vec![0u8; config.r]; k];
        for (i, row) in t.iter_mut().enumerate() {
            for (j, coefficient) in row.iter_mut().enumerate() {
                let x = alpha_pow(k + j);
                let mut num: u8 = 1;
                let mut den: u8 = 1;
                for m in 0..k {
                    if m == i {
                        continue;
                    }
                    num = gf_mul(num, gf_sub(x, alpha_pow(m)));
                    den = gf_mul(den, gf_sub(alpha_pow(i), alpha_pow(m)));
                }
                *coefficient = gf_div(num, den);
            }
        }

        Ok(Self {
            config,
            eval_points,
            t,
        })
    }

    pub fn config(&self) -> ReedSolomonConfig {
        self.config
    }

    /// Encode `data` (exactly k blocks of equal size) into r parity blocks.
    pub fn encode(&self, data: &[&[u8]]) -> Result<Vec<Vec<u8>>, ReedSolomonError> {
        if data.len() != self.config.k {
            return Err(ReedSolomonError::BlockCountMismatch {
                expected: self.config.k,
                got: data.len(),
            });
        }
        let block_size = data[0].len();
        for block in data {
            if block.len() != block_size {
                return Err(ReedSolomonError::BlockSizeMismatch {
                    expected: block_size,
                    got: block.len(),
                });
            }
        }
        let mut parity = vec![vec![0u8; block_size]; self.config.r];
        for (b, _) in data[0].iter().enumerate() {
            for (j, parity_block) in parity.iter_mut().enumerate() {
                let acc = data
                    .iter()
                    .zip(&self.t)
                    .fold(0, |acc, (block, coeffs)| acc ^ gf_mul(block[b], coeffs[j]));
                parity_block[b] = acc;
            }
        }
        Ok(parity)
    }

    /// Reconstruct all n blocks from `blocks`, where `None` marks an erasure.
    /// Requires at least k known blocks. Returns the n recovered blocks in
    /// order (data first, then parity), with known blocks copied through.
    pub fn recover(&self, blocks: &[Option<&[u8]>]) -> Result<Vec<Vec<u8>>, ReedSolomonError> {
        if blocks.len() != self.config.n() {
            return Err(ReedSolomonError::BlockCountMismatch {
                expected: self.config.n(),
                got: blocks.len(),
            });
        }
        let block_size = blocks.iter().flatten().next().map_or(0, |b| b.len());
        for b in blocks {
            match b {
                None => {}
                Some(block) if block.len() != block_size => {
                    return Err(ReedSolomonError::BlockSizeMismatch {
                        expected: block_size,
                        got: block.len(),
                    });
                }
                Some(_) => {}
            }
        }
        let k = self.config.k;
        let n = self.config.n();

        let known: Vec<usize> = blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.is_some().then_some(i))
            .collect();
        if known.len() < k {
            return Err(ReedSolomonError::TooFewErasureSymbols {
                have: known.len(),
                need: k,
            });
        }
        if block_size == 0 {
            return Ok(vec![vec![]; n]);
        }
        // Use the first k known positions as interpolation points.
        let pts: &[usize] = &known[..k];

        // Precompute Lagrange denominators d_l = prod_{m != l} (alpha^pts[l] - alpha^pts[m])
        let mut denom = vec![0u8; k];
        for l in 0..k {
            let mut d: u8 = 1;
            for m in 0..k {
                if m == l {
                    continue;
                }
                d = gf_mul(d, gf_sub(alpha_pow(pts[l]), alpha_pow(pts[m])));
            }
            denom[l] = d;
        }

        let mut result = vec![vec![0u8; block_size]; n];
        for (i, b) in blocks.iter().enumerate() {
            if let Some(block) = b {
                result[i].copy_from_slice(block);
            }
        }

        let missing: Vec<usize> = (0..n).filter(|i| blocks[*i].is_none()).collect();
        if missing.is_empty() {
            return Ok(result);
        }

        // For each byte position independently, interpolate the polynomial and
        // evaluate at each missing point.
        for &j in &missing {
            let x = self.eval_points[j];
            for (b, output_byte) in result[j].iter_mut().enumerate() {
                // values at the k known points
                let known_vals: Vec<u8> =
                    pts.iter().map(|point| blocks[*point].unwrap()[b]).collect();
                let mut acc: u8 = 0;
                for (l, _) in pts.iter().enumerate() {
                    // L_l(x) = prod_{m != l} (x - alpha^pts[m]) / denom[l]
                    let mut num: u8 = 1;
                    for (m, &other_point) in pts.iter().enumerate() {
                        if m == l {
                            continue;
                        }
                        num = gf_mul(num, gf_sub(x, alpha_pow(other_point)));
                    }
                    let basis = gf_div(num, denom[l]);
                    acc ^= gf_mul(known_vals[l], basis);
                }
                *output_byte = acc;
            }
        }
        Ok(result)
    }
}

fn gf_sub(a: u8, b: u8) -> u8 {
    a ^ b // characteristic 2: subtraction == addition == XOR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(k: usize, r: usize) -> ReedSolomon {
        ReedSolomon::new(ReedSolomonConfig::new(k, r).unwrap()).unwrap()
    }

    #[test]
    fn encode_recover_no_erasures() {
        let rs = cfg(4, 2);
        let data: Vec<Vec<u8>> = vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
            vec![10, 11, 12],
        ];
        let data_refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
        let parity = rs.encode(&data_refs).unwrap();
        assert_eq!(parity.len(), 2);
        let mut blocks: Vec<Option<&[u8]>> = data.iter().map(|v| Some(v.as_slice())).collect();
        blocks.extend(parity.iter().map(|v| Some(v.as_slice())));
        let recovered = rs.recover(&blocks).unwrap();
        assert_eq!(recovered.len(), 6);
        for i in 0..4 {
            assert_eq!(recovered[i], data[i]);
        }
        for j in 0..2 {
            assert_eq!(recovered[4 + j], parity[j]);
        }
    }

    #[test]
    fn recover_one_data_erasure() {
        let rs = cfg(4, 2);
        let data: Vec<Vec<u8>> = (0..4).map(|i| vec![i as u8 + 1, 100, 200]).collect();
        let data_refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
        let parity = rs.encode(&data_refs).unwrap();
        let blocks: Vec<Option<&[u8]>> = vec![
            Some(&data[0]),
            None,
            Some(&data[2]),
            Some(&data[3]),
            Some(&parity[0]),
            Some(&parity[1]),
        ];
        let recovered = rs.recover(&blocks).unwrap();
        assert_eq!(recovered[1], data[1]);
    }

    #[test]
    fn recover_all_parity_erasures_is_identity() {
        let rs = cfg(3, 2);
        let data: Vec<Vec<u8>> = vec![vec![1, 2], vec![3, 4], vec![5, 6]];
        let data_refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
        let parity = rs.encode(&data_refs).unwrap();
        let blocks: Vec<Option<&[u8]>> = data
            .iter()
            .map(|v| Some(v.as_slice()))
            .chain(parity.iter().map(|v| Some(v.as_slice())))
            .collect();
        let recovered = rs.recover(&blocks).unwrap();
        assert_eq!(recovered.len(), 5);
        for i in 0..3 {
            assert_eq!(recovered[i], data[i]);
        }
        for j in 0..2 {
            assert_eq!(recovered[3 + j], parity[j]);
        }
    }

    #[test]
    fn recover_two_erasures_at_limit() {
        let rs = cfg(3, 2);
        let data: Vec<Vec<u8>> = vec![vec![9, 8, 7], vec![6, 5, 4], vec![3, 2, 1]];
        let data_refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
        let parity = rs.encode(&data_refs).unwrap();
        // Erase data[1] and parity[1]
        let blocks: Vec<Option<&[u8]>> =
            vec![Some(&data[0]), None, Some(&data[2]), Some(&parity[0]), None];
        let recovered = rs.recover(&blocks).unwrap();
        assert_eq!(recovered[1], data[1]);
        assert_eq!(recovered[4], parity[1]);
    }

    #[test]
    fn too_many_erasures_rejected() {
        let rs = cfg(3, 2);
        let data: Vec<Vec<u8>> = vec![vec![1; 4], vec![2; 4], vec![3; 4]];
        let blocks: Vec<Option<&[u8]>> = vec![Some(&data[0]), None, None, None, None];
        let err = rs.recover(&blocks).unwrap_err();
        assert!(matches!(
            err,
            ReedSolomonError::TooFewErasureSymbols { have: 1, need: 3 }
        ));
    }

    #[test]
    fn k_one_is_repetition_parity() {
        // k=1, r=2: parity[j] = data[0] (single evaluation point => constant poly)
        let rs = cfg(1, 2);
        let data = [vec![42u8; 5]];
        let data_refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
        let parity = rs.encode(&data_refs).unwrap();
        assert_eq!(parity[0], vec![42; 5]);
        assert_eq!(parity[1], vec![42; 5]);
    }

    #[test]
    fn max_fragments_is_255() {
        assert!(ReedSolomonConfig::new(200, 55).is_ok()); // n = 255
        assert!(ReedSolomonConfig::new(200, 56).is_err()); // n = 256
    }

    #[test]
    fn random_round_trip_properties() {
        use rand::seq::SliceRandom;
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for (k, r) in [(2, 1), (3, 3), (5, 2), (8, 4)] {
            let rs = cfg(k, r);
            let n = k + r;
            let bs = 17;
            let data: Vec<Vec<u8>> = (0..k)
                .map(|_| (0..bs).map(|_| rng.r#gen()).collect())
                .collect();
            let data_refs: Vec<&[u8]> = data.iter().map(|v| v.as_slice()).collect();
            let parity = rs.encode(&data_refs).unwrap();
            let mut all: Vec<Vec<u8>> = data.clone();
            all.extend(parity);
            // Erase up to r distinct blocks, ensure recovery matches originals.
            let mut erased: Vec<bool> = vec![false; n];
            let count = rng.gen_range(0..=r);
            let mut idxs: Vec<usize> = (0..n).collect();
            idxs.shuffle(&mut rng);
            for &i in &idxs[..count] {
                erased[i] = true;
            }
            let blocks: Vec<Option<&[u8]>> = (0..n)
                .map(|i| {
                    if erased[i] {
                        None
                    } else {
                        Some(all[i].as_slice())
                    }
                })
                .collect();
            let recovered = rs.recover(&blocks).unwrap();
            for i in 0..n {
                assert_eq!(recovered[i], all[i], "block {i} mismatch k={k} r={r}");
            }
        }
    }
}
