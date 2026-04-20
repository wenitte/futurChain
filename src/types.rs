use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ── Primitive types ───────────────────────────────────────────────────────────

pub type Hash        = [u8; 32];
pub type Address     = [u8; 32];
pub type Signature   = [u8; 64];
pub type Slot        = u64;
pub type Epoch       = u64;
pub type TokenAmount = u64;

// ── Serde helpers for fixed-size byte arrays ──────────────────────────────────

pub mod serde_addr {
    use super::*;
    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let h = String::deserialize(d)?;
        let v = hex::decode(&h).map_err(serde::de::Error::custom)?;
        v.try_into().map_err(|_| serde::de::Error::custom("address must be 32 bytes"))
    }
}

pub mod serde_hash {
    use super::*;
    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let h = String::deserialize(d)?;
        let v = hex::decode(&h).map_err(serde::de::Error::custom)?;
        v.try_into().map_err(|_| serde::de::Error::custom("hash must be 32 bytes"))
    }
}

pub mod serde_sig {
    use super::*;
    pub fn serialize<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let h = String::deserialize(d)?;
        let v = hex::decode(&h).map_err(serde::de::Error::custom)?;
        v.try_into().map_err(|_| serde::de::Error::custom("signature must be 64 bytes"))
    }
}

// ── Account ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    #[serde(with = "serde_addr")]
    pub address:    Address,
    pub balance:    TokenAmount,
    pub nonce:      u64,
    pub data:       Vec<u8>,
    #[serde(with = "serde_addr")]
    pub owner:      Address,
    pub executable: bool,
}

impl Account {
    pub fn new(address: Address) -> Self {
        Self { address, balance: 0, nonce: 0, data: vec![], owner: [0u8; 32], executable: false }
    }
}

// ── Transaction ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountMeta {
    #[serde(with = "serde_addr")]
    pub address:     Address,
    pub is_signer:   bool,
    pub is_writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    #[serde(with = "serde_addr")]
    pub program_id: Address,
    pub accounts:   Vec<AccountMeta>,
    pub data:       Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub nonce:                          u64,
    #[serde(with = "serde_addr")]
    pub sender:                         Address,
    #[serde(with = "serde_addr")]
    pub recipient:                      Address,
    pub amount:                         TokenAmount,
    pub fee:                            TokenAmount,
    pub instructions:                   Vec<Instruction>,
    #[serde(with = "serde_hash")]
    pub recent_blockhash:               Hash,
    #[serde(with = "serde_sig")]
    pub signature:                      Signature,
}

impl Transaction {
    /// Bytes the sender must sign — everything except the signature field
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.nonce.to_le_bytes());
        b.extend_from_slice(&self.sender);
        b.extend_from_slice(&self.recipient);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.fee.to_le_bytes());
        b.extend_from_slice(&self.recent_blockhash);
        b
    }

    pub fn hash(&self) -> Hash {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(self.signable_bytes());
        h.update(self.signature);
        h.finalize().into()
    }
}

// ── Block ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub slot:                    Slot,
    #[serde(with = "serde_hash")]
    pub parent_hash:             Hash,
    #[serde(with = "serde_hash")]
    pub poh_hash:                Hash,
    #[serde(with = "serde_hash")]
    pub tx_root:                 Hash,
    #[serde(with = "serde_hash")]
    pub state_root:              Hash,
    #[serde(with = "serde_addr")]
    pub proposer:                Address,
    pub timestamp:               u64,
    pub tx_count:                u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header:       BlockHeader,
    pub transactions: Vec<Transaction>,
    #[serde(with = "serde_hash")]
    pub hash:         Hash,
}

impl Block {
    pub fn compute_hash(header: &BlockHeader) -> Hash {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(header.slot.to_le_bytes());
        h.update(header.parent_hash);
        h.update(header.poh_hash);
        h.update(header.tx_root);
        h.update(header.state_root);
        h.update(header.proposer);
        h.update(header.timestamp.to_le_bytes());
        h.finalize().into()
    }

    pub fn genesis() -> Self {
        let header = BlockHeader {
            slot: 0, parent_hash: [0u8; 32], poh_hash: [0u8; 32],
            tx_root: [0u8; 32], state_root: [0u8; 32], proposer: [0u8; 32],
            timestamp: 0, tx_count: 0,
        };
        let hash = Self::compute_hash(&header);
        Self { header, transactions: vec![], hash }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn hex_address(addr: &Address) -> String { hex::encode(addr) }
pub fn hex_hash(h: &Hash) -> String { hex::encode(h) }
