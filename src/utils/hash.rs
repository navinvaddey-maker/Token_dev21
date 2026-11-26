//! Canonical cryptographic (SHA-256) and deterministic FNV-1a hashing helpers.
//!
//! // ARCH: ARCHITECTURE.md §3, §6 — Centralizes idempotent SHA-256 content hashing and
//! // cross-session FNV-1a primary/secondary feature hashing.

use sha2::{Digest, Sha256};

/// Stateless deterministic hashing utility.
#[derive(Debug, Clone, Copy, Default)]
pub struct OryxHasher;

impl OryxHasher {
    /// Computes the lowercase 64-character hexadecimal SHA-256 digest of `bytes`.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn compute_sha256_hex_digest(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Computes deterministic 64-bit FNV-1a hash using standard FNV offset basis and prime.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn compute_fnv1a_primary_u64(text: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    /// Computes secondary deterministic 64-bit hash with distinct seed/multiplier for sign hashing.
    ///
    /// Matches the secondary projection hash used by the deterministic fallback embedder.
    ///
    /// # Panics
    /// Never.
    #[must_use]
    pub fn compute_fnv1a_secondary_u64(text: &str) -> u64 {
        let mut hash: u64 = 0x517c_c1b7_2722_0a95;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_known_vectors() {
        assert_eq!(
            OryxHasher::compute_sha256_hex_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            OryxHasher::compute_sha256_hex_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_fnv1a_primary_and_secondary_deterministic() {
        let p1 = OryxHasher::compute_fnv1a_primary_u64("token_compress");
        let p2 = OryxHasher::compute_fnv1a_primary_u64("token_compress");
        assert_eq!(p1, p2);
        assert_ne!(p1, OryxHasher::compute_fnv1a_primary_u64("token_compress2"));

        let s1 = OryxHasher::compute_fnv1a_secondary_u64("token_compress");
        let s2 = OryxHasher::compute_fnv1a_secondary_u64("token_compress");
        assert_eq!(s1, s2);
        assert_ne!(p1, s1);
        assert_eq!(
            OryxHasher::compute_fnv1a_primary_u64(""),
            0xcbf2_9ce4_8422_2325
        );
        assert_eq!(
            OryxHasher::compute_fnv1a_secondary_u64(""),
            0x517c_c1b7_2722_0a95
        );
    }
}
