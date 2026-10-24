//! Storage: archive reading/writing and streaming helpers.
//!
//! An archive is a directory containing `manifest.json` plus one file per
//! fragment block: `frag_{id:06}.bin` for data/parity fragments and
//! `frag_{id:06}_dup{N}.bin` for duplicates. This layout keeps fragments
//! individually addressable and lets the channel simulator drop or duplicate
//! files naturally.

use crate::fragment::{Fragment, HEADER_SIZE, deserialize_block, serialize_block};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Archive manifest describing an encoded dataset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub format: String,
    pub version: u16,
    pub original_size_bytes: u64,
    pub original_sha256: String,
    pub dna_bases: u64,
    pub fragment_size_bases: u64,
    pub block_payload_size: u64,
    pub header_size: u64,
    pub block_size: u64,
    pub ecc_k: u64,
    pub ecc_r: u64,
    pub total_data: u64,
    pub total_parity: u64,
    pub fragment_count: u64,
    pub gc_min: Option<f64>,
    pub gc_max: Option<f64>,
    pub max_homopolymer: Option<usize>,
    pub created_at: Option<String>,
}

impl ArchiveManifest {
    pub fn original_hash(&self) -> Option<[u8; 32]> {
        let s = &self.original_sha256;
        if s.len() != 64 {
            return None;
        }
        let bytes: Result<Vec<u8>, _> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect();
        let bytes = bytes.ok()?;
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(arr)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FragmentFileInfo {
    pub id: u64,
    pub kind: String,
    pub path: PathBuf,
}

/// Write an archive directory: manifest + fragment blocks.
pub fn write_archive(
    dir: &Path,
    manifest: &ArchiveManifest,
    fragments: &[Fragment],
    block_payload_size: usize,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let manifest_path = dir.join("manifest.json");
    let json = serde_json::to_string_pretty(manifest).unwrap();
    std::fs::write(&manifest_path, json)?;

    for fragment in fragments {
        let block = serialize_block(fragment, block_payload_size);
        let name = match fragment.meta.kind {
            crate::fragment::FragmentKind::Data => format!("frag_{:06}.bin", fragment.meta.id),
            crate::fragment::FragmentKind::Parity => format!("frag_{:06}.bin", fragment.meta.id),
        };
        std::fs::write(dir.join(&name), block)?;
    }
    Ok(())
}

/// Append a duplicate fragment file to an archive directory.
pub fn append_duplicate(dir: &Path, id: u64, copy: usize, block: &[u8]) -> std::io::Result<()> {
    let name = format!("frag_{:06}_dup{}.bin", id, copy);
    std::fs::write(dir.join(name), block)
}

/// Read all fragment blocks from an archive directory, keyed by fragment id.
// Duplicates for the same id are collapsed: the first validly parsed block wins;
// others are ignored here (the caller decides duplicate handling).
pub fn read_archive_blocks(dir: &Path) -> Result<BTreeMap<u64, Vec<u8>>, StorageError> {
    let manifest_path = dir.join("manifest.json");
    let manifest: ArchiveManifest = serde_json::from_str(
        &std::fs::read_to_string(manifest_path).map_err(|e| StorageError::Io(e.to_string()))?,
    )
    .map_err(|e| StorageError::Manifest(e.to_string()))?;
    let block_payload_size = manifest.block_payload_size as usize;
    let expected_size = HEADER_SIZE + block_payload_size;

    let mut blocks = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !file_name.starts_with("frag_") || !file_name.ends_with(".bin") {
            continue;
        }
        // Parse id from frag_{id:06}.bin or frag_{id:06}_dupN.bin
        let id_part = file_name
            .strip_prefix("frag_")
            .and_then(|name| name.strip_suffix(".bin"))
            .unwrap_or("");
        let id_str = id_part.split('_').next().unwrap_or("");
        let id: u64 = match id_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if blocks.contains_key(&id) {
            continue;
        }
        let data = std::fs::read(&path)?;
        if data.len() != expected_size {
            return Err(StorageError::BlockSizeMismatch {
                expected: expected_size,
                got: data.len(),
            });
        }
        blocks.insert(id, data);
    }

    Ok(blocks)
}

/// Read blocks into a dense vec indexed by id: `blocks[id]`, missing = None.
pub fn read_archive_blocks_dense(dir: &Path) -> Result<Vec<Option<Vec<u8>>>, StorageError> {
    let map = read_archive_blocks(dir)?;
    let manifest_path = dir.join("manifest.json");
    let manifest: ArchiveManifest = serde_json::from_str(
        &std::fs::read_to_string(manifest_path).map_err(|e| StorageError::Io(e.to_string()))?,
    )
    .map_err(|e| StorageError::Manifest(e.to_string()))?;
    let count = usize::try_from(manifest.fragment_count)
        .map_err(|_| StorageError::Manifest("fragment_count exceeds platform limits".into()))?;
    if map.keys().any(|id| *id >= manifest.fragment_count) {
        return Err(StorageError::Manifest(
            "fragment id is outside manifest fragment_count".into(),
        ));
    }
    let mut blocks = vec![None; count];
    for (id, data) in map {
        blocks[id as usize] = Some(data);
    }
    Ok(blocks)
}

/// Read raw bytes from a file.
pub fn read_file(path: &Path) -> Result<Vec<u8>, StorageError> {
    std::fs::read(path).map_err(|e| StorageError::Io(e.to_string()))
}

/// Write raw bytes to a file, creating parent directories.
pub fn write_file(path: &Path, data: &[u8]) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StorageError::Io(e.to_string()))?;
    }
    std::fs::write(path, data).map_err(|e| StorageError::Io(e.to_string()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    Io(String),
    Manifest(String),
    Format(String),
    BlockSizeMismatch { expected: usize, got: usize },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Io(msg) => write!(f, "storage: I/O error: {msg}"),
            StorageError::Manifest(msg) => write!(f, "storage: manifest error: {msg}"),
            StorageError::Format(msg) => write!(f, "storage: format error: {msg}"),
            StorageError::BlockSizeMismatch { expected, got } => {
                write!(
                    f,
                    "storage: block size mismatch: expected {expected}, got {got}"
                )
            }
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        StorageError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        StorageError::Manifest(e.to_string())
    }
}

impl From<crate::fragment::FormatError> for StorageError {
    fn from(e: crate::fragment::FormatError) -> Self {
        StorageError::Format(e.to_string())
    }
}

/// Compute SHA-256 hex digest of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in result {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Deserialize every block in an archive into fragments, validating each.
pub fn parse_archive_fragments(
    dir: &Path,
    block_payload_size: usize,
) -> Result<Vec<Fragment>, StorageError> {
    let map = read_archive_blocks(dir)?;
    let mut fragments = Vec::new();
    for (_id, block) in map {
        fragments.push(deserialize_block(&block, block_payload_size)?);
    }
    fragments.sort_by_key(|f| f.meta.offset_bases);
    Ok(fragments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Base;
    use crate::fragment::Fragment;
    use std::collections::BTreeMap;

    fn tmp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "dnaforge-test-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn archive_write_read_round_trip() {
        let dir = tmp_dir();
        let payload = vec![Base(0), Base(1), Base(2), Base(3)];
        let fragment = Fragment::new_data(0, 0, 1, 0, payload.clone());
        let manifest = ArchiveManifest {
            format: "dnaforge-archive".into(),
            version: 1,
            original_size_bytes: 0,
            original_sha256: "00".repeat(32),
            dna_bases: payload.len() as u64,
            fragment_size_bases: 4,
            block_payload_size: 16,
            header_size: HEADER_SIZE as u64,
            block_size: (HEADER_SIZE + 16) as u64,
            ecc_k: 1,
            ecc_r: 0,
            total_data: 1,
            total_parity: 0,
            fragment_count: 1,
            gc_min: None,
            gc_max: None,
            max_homopolymer: None,
            created_at: None,
        };
        write_archive(&dir, &manifest, &[fragment], 16).unwrap();
        let blocks = read_archive_blocks(&dir).unwrap();
        assert_eq!(blocks.len(), 1);
        let block = blocks.get(&0).unwrap();
        let back = deserialize_block(block, 16).unwrap();
        assert_eq!(back.payload, payload);
        let _: BTreeMap<u64, Vec<u8>> = blocks;
    }

    #[test]
    fn sha256_hex_known() {
        let digest = sha256_hex(b"");
        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn read_archive_dense_missing_ids() {
        let dir = tmp_dir();
        let manifest = ArchiveManifest {
            format: "dnaforge-archive".into(),
            version: 1,
            original_size_bytes: 0,
            original_sha256: "00".repeat(32),
            dna_bases: 0,
            fragment_size_bases: 4,
            block_payload_size: 16,
            header_size: HEADER_SIZE as u64,
            block_size: (HEADER_SIZE + 16) as u64,
            ecc_k: 3,
            ecc_r: 1,
            total_data: 3,
            total_parity: 1,
            fragment_count: 4,
            gc_min: None,
            gc_max: None,
            max_homopolymer: None,
            created_at: None,
        };
        let f0 = Fragment::new_data(0, 0, 3, 0, vec![Base(0); 4]);
        let f1 = Fragment::new_data(1, 0, 3, 4, vec![Base(1); 4]);
        let f3 = Fragment::new_parity(3, 3, 0, vec![Base(2); 4]);
        write_archive(&dir, &manifest, &[f0.clone(), f1.clone(), f3.clone()], 16).unwrap();
        // id 2 missing -> dense vec has None at index 2
        let dense = read_archive_blocks_dense(&dir).unwrap();
        assert_eq!(dense.len(), 4);
        assert_eq!(dense[0].as_ref().map(|b| b.len()), Some(HEADER_SIZE + 16));
        assert!(dense[1].is_some());
        assert!(dense[2].is_none());
        assert!(dense[3].is_some());
        let _ = f3;
    }
}
