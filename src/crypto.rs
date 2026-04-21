use crate::types::{Address, Hash, Signature};
use ed25519_dalek::{Signer, Verifier, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

// ── Hashing ───────────────────────────────────────────────────────────────────

pub fn sha256(data: &[u8]) -> Hash {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// Proof-of-History: one SHA-256 tick
pub fn poh_tick(prev: Hash) -> Hash {
    sha256(&prev)
}

pub fn hash_transactions(txs: &[crate::types::Transaction]) -> Hash {
    if txs.is_empty() { return [0u8; 32]; }
    let mut h = Sha256::new();
    for tx in txs { h.update(tx.hash()); }
    h.finalize().into()
}

// ── Keypair ───────────────────────────────────────────────────────────────────

pub struct Keypair {
    inner: SigningKey,
}

impl Keypair {
    pub fn generate() -> Self {
        Self { inner: SigningKey::generate(&mut rand::rngs::OsRng) }
    }

    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self { inner: SigningKey::from_bytes(bytes) }
    }

    pub fn address(&self) -> Address {
        self.inner.verifying_key().to_bytes()
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.inner.sign(data).to_bytes()
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }
}

// ── Program Derived Addresses ─────────────────────────────────────────────────

/// Derive a deterministic PDA from seeds + program_id (no bump — pure hash).
/// Seeds are hashed together with the program_id to produce an off-curve address.
pub fn pda_derive(seeds: &[&[u8]], program_id: &Address) -> Address {
    let mut h = Sha256::new();
    for seed in seeds { h.update(seed); }
    h.update(program_id);
    h.update(b"pda");
    h.finalize().into()
}

/// PDA with explicit bump byte — mirrors Solana's find_program_address convention.
pub fn find_pda(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    for bump in (0u8..=255).rev() {
        let mut all_seeds: Vec<&[u8]> = seeds.to_vec();
        let bump_bytes = [bump];
        all_seeds.push(&bump_bytes);
        let addr = pda_derive(&all_seeds, program_id);
        // Naively accept first result — production would check curve exclusion
        return (addr, bump);
    }
    (pda_derive(seeds, program_id), 0)
}

// ── Verification ─────────────────────────────────────────────────────────────

pub fn verify_signature(address: &Address, data: &[u8], sig: &Signature) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(address) else { return false; };
    let dalek_sig = ed25519_dalek::Signature::from_bytes(sig);
    vk.verify(data, &dalek_sig).is_ok()
}
