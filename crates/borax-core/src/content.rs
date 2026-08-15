//! Identifying a file by what is in it rather than what it is called.
//!
//! borax renames files, so a path is not a stable name for one. The
//! content hash is: it survives renaming, tells two files with the same
//! name apart, and lets a second run recognise a file it has already
//! resolved. It is what the cache's content index and the rename
//! journal both key on.

use std::fmt;

use sha2::{Digest, Sha256};

/// The SHA-256 of a byte stream, rendered as `sha256-` followed by 64
/// lowercase hex digits.
///
/// The algorithm prefix is part of the value, so a stored hash stays
/// readable if borax ever hashes with something else: entries written
/// by a different algorithm are distinguishable rather than silently
/// compared.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    /// The hash in its `sha256-<hex>` form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The [`ContentHash`] of `bytes`.
pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    let _ = bytes;
    todo!("hash the bytes")
}

/// A hash being computed over a stream of chunks.
///
/// Feeding a file through this rather than reading it into memory keeps
/// the cost of hashing a large PDF constant in memory; the result is
/// identical to [`hash_bytes`] over the concatenation of the chunks.
#[derive(Debug, Clone, Default)]
pub struct Hasher {
    digest: Sha256,
}

impl Hasher {
    /// A hasher that has consumed nothing.
    pub fn new() -> Hasher {
        Hasher::default()
    }

    /// Append `chunk` to the stream being hashed.
    pub fn update(&mut self, chunk: &[u8]) {
        let _ = chunk;
        todo!("feed the chunk to the digest")
    }

    /// Consume the hasher and produce the hash of everything fed to it.
    pub fn finish(self) -> ContentHash {
        todo!("render the digest as sha256-<hex>")
    }
}
