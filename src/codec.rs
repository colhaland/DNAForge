//! Codec: byte <-> DNA base conversion using a two-bit baseline mapping.
//!
//! Mapping (baseline, not synthesis-ready):
//!   00 -> A, 01 -> C, 10 -> G, 11 -> T
//!
//! Each byte yields four DNA bases. This module performs pure base conversion
//! and input validation. Biological constraints (GC content, homopolymers) are
//! handled in [`crate::constraints`].

/// A DNA base encoded as two bits: 0 = A, 1 = C, 2 = G, 3 = T.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, PartialOrd, Ord)]
pub struct Base(pub u8);

impl Base {
    /// The four valid bases in canonical order.
    pub const ALL: [Base; 4] = [Base(0), Base(1), Base(2), Base(3)];

    /// Returns true if `0 <= self.0 <= 3`.
    pub fn is_valid(self) -> bool {
        self.0 <= 3
    }
}

/// Errors produced by the codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    /// The decoded base stream length is not a multiple of four bases,
    /// so it cannot be split into whole bytes.
    IncompleteByte {
        bases: usize,
        remainder: usize,
    },
    /// A base symbol outside the range 0..3 was encountered.
    InvalidBase {
        position: usize,
        value: u8,
    },
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::IncompleteByte { bases, remainder } => write!(
                f,
                "codec: {bases} bases is not a whole number of bytes (remainder {remainder})"
            ),
            CodecError::InvalidBase { position, value } => {
                write!(f, "codec: invalid base value {value} at position {position}")
            }
        }
    }
}

impl std::error::Error for CodecError {}

/// Convert a numeric base value (0..=3) to its ASCII character.
pub fn base_to_char(base: Base) -> char {
    match base.0 {
        0 => 'A',
        1 => 'C',
        2 => 'G',
        3 => 'T',
        _ => 'N',
    }
}

/// Convert an ASCII base character to its numeric value. Returns `None` for
/// anything other than A, C, G, T (case-insensitive).
pub fn char_to_base(ch: char) -> Option<Base> {
    match ch {
        'A' | 'a' => Some(Base(0)),
        'C' | 'c' => Some(Base(1)),
        'G' | 'g' => Some(Base(2)),
        'T' | 't' => Some(Base(3)),
        _ => None,
    }
}

/// Encode raw bytes into DNA bases using the two-bit mapping.
///
/// Each input byte produces exactly four bases, most significant two bits
/// first. Output length is `4 * input.len()`.
pub fn encode_bytes(bytes: &[u8]) -> Vec<Base> {
    let mut out = Vec::with_capacity(bytes.len() * 4);
    for &byte in bytes {
        for shift in (0..4).rev() {
            let nibble = (byte >> (shift * 2)) & 0b11;
            out.push(Base(nibble));
        }
    }
    out
}

/// Decode DNA bases back into raw bytes.
///
/// The base count must be a multiple of four. Groups of four bases are
/// reassembled into one byte (most significant two bits first).
pub fn decode_bases(bases: &[Base]) -> Result<Vec<u8>, CodecError> {
    if bases.len() % 4 != 0 {
        return Err(CodecError::IncompleteByte {
            bases: bases.len(),
            remainder: bases.len() % 4,
        });
    }
    let mut out = Vec::with_capacity(bases.len() / 4);
    for chunk in bases.chunks(4) {
        let mut byte: u8 = 0;
        for (i, base) in chunk.iter().enumerate() {
            if !base.is_valid() {
                return Err(CodecError::InvalidBase {
                    position: i,
                    value: base.0,
                });
            }
            byte = (byte << 2) | base.0;
        }
        out.push(byte);
    }
    Ok(out)
}

/// Encode raw bytes into an ASCII DNA string (A/C/G/T).
pub fn encode_bytes_to_string(bytes: &[u8]) -> String {
    let bases = encode_bytes(bytes);
    bases.iter().map(|b| base_to_char(*b)).collect()
}

/// Decode an ASCII DNA string into raw bytes.
pub fn decode_string(s: &str) -> Result<Vec<u8>, CodecError> {
    let bases: Result<Vec<Base>, _> = s
        .chars()
        .enumerate()
        .map(|(i, ch)| char_to_base(ch).ok_or_else(|| CodecError::InvalidBase {
            position: i,
            value: ch as u8,
        }))
        .collect();
    let bases = bases?;
    decode_bases(&bases).map_err(|e| match e {
        CodecError::IncompleteByte { bases, remainder } => {
            CodecError::IncompleteByte { bases, remainder }
        }
        CodecError::InvalidBase { position, value } => CodecError::InvalidBase {
            position,
            value,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let data: Vec<u8> = (0..=255u8).collect();
        let bases = encode_bytes(&data);
        assert_eq!(bases.len(), data.len() * 4);
        let decoded = decode_bases(&bases).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn empty_input_round_trip() {
        let bases = encode_bytes(&[]);
        assert!(bases.is_empty());
        let decoded = decode_bases(&bases).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn mapping_values() {
        assert_eq!(base_to_char(Base(0)), 'A');
        assert_eq!(base_to_char(Base(1)), 'C');
        assert_eq!(base_to_char(Base(2)), 'G');
        assert_eq!(base_to_char(Base(3)), 'T');
    }

    #[test]
    fn char_to_base_case_insensitive() {
        assert_eq!(char_to_base('a'), Some(Base(0)));
        assert_eq!(char_to_base('C'), Some(Base(1)));
        assert_eq!(char_to_base('g'), Some(Base(2)));
        assert_eq!(char_to_base('T'), Some(Base(3)));
        assert_eq!(char_to_base('N'), None);
        assert_eq!(char_to_base(' '), None);
    }

    #[test]
    fn incomplete_byte_rejected() {
        let err = decode_bases(&[Base(0), Base(1), Base(2)]).unwrap_err();
        assert!(matches!(err, CodecError::IncompleteByte { .. }));
    }

    #[test]
    fn invalid_base_rejected() {
        let err = decode_bases(&[Base(0), Base(4), Base(1), Base(2)]).unwrap_err();
        assert!(matches!(err, CodecError::InvalidBase { value: 4, .. }));
    }

    #[test]
    fn string_round_trip() {
        let data = b"Hello, DNAForge!";
        let s = encode_bytes_to_string(data);
        let decoded = decode_string(&s).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn string_rejects_invalid_characters() {
        let err = decode_string("ACGN").unwrap_err();
        assert!(matches!(err, CodecError::InvalidBase { .. }));
    }
}
