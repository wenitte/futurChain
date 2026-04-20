use std::collections::HashMap;
use crate::types::{Account, Address, Hash, TokenAmount, Transaction};
use crate::crypto;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("account not found")]
    AccountNotFound,
    #[error("insufficient balance: need {need}, have {have}")]
    InsufficientBalance { need: u64, have: u64 },
    #[error("invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u64, got: u64 },
    #[error("invalid signature")]
    InvalidSignature,
    #[error("arithmetic overflow")]
    Overflow,
}

pub struct Ledger {
    accounts: HashMap<Address, Account>,
}

impl Ledger {
    pub fn new() -> Self {
        Self { accounts: HashMap::new() }
    }

    pub fn get(&self, address: &Address) -> Option<&Account> {
        self.accounts.get(address)
    }

    pub fn airdrop(&mut self, address: Address, amount: TokenAmount) {
        self.accounts
            .entry(address)
            .or_insert_with(|| Account::new(address))
            .balance += amount;
    }

    pub fn apply_transaction(&mut self, tx: &Transaction) -> Result<(), LedgerError> {
        // Validate signature
        if !crypto::verify_signature(&tx.sender, &tx.signable_bytes(), &tx.signature) {
            return Err(LedgerError::InvalidSignature);
        }

        // Validate nonce and balance (immutable reads)
        let (expected_nonce, sender_balance) = self
            .accounts
            .get(&tx.sender)
            .map(|a| (a.nonce, a.balance))
            .unwrap_or((0, 0));

        if tx.nonce != expected_nonce {
            return Err(LedgerError::InvalidNonce { expected: expected_nonce, got: tx.nonce });
        }

        let total = tx.amount.checked_add(tx.fee).ok_or(LedgerError::Overflow)?;
        if sender_balance < total {
            return Err(LedgerError::InsufficientBalance { need: total, have: sender_balance });
        }

        // Apply: debit sender
        {
            let sender = self.accounts.entry(tx.sender).or_insert_with(|| Account::new(tx.sender));
            sender.balance -= total;
            sender.nonce += 1;
        }

        // Apply: credit recipient
        {
            let recipient = self.accounts.entry(tx.recipient).or_insert_with(|| Account::new(tx.recipient));
            recipient.balance = recipient.balance.checked_add(tx.amount).ok_or(LedgerError::Overflow)?;
        }

        Ok(())
    }

    /// Deterministic hash of all account balances — state fingerprint
    pub fn state_root(&self) -> Hash {
        let mut addresses: Vec<Address> = self.accounts.keys().copied().collect();
        addresses.sort();
        let mut h = Sha256::new();
        for addr in addresses {
            let acc = &self.accounts[&addr];
            h.update(addr);
            h.update(acc.balance.to_le_bytes());
            h.update(acc.nonce.to_le_bytes());
        }
        h.finalize().into()
    }

    pub fn total_supply(&self) -> TokenAmount {
        self.accounts.values().map(|a| a.balance).sum()
    }

    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    fn make_tx(kp: &Keypair, recipient: Address, amount: u64, fee: u64, nonce: u64, blockhash: Hash) -> Transaction {
        let mut tx = Transaction {
            nonce,
            sender: kp.address(),
            recipient,
            amount,
            fee,
            instructions: vec![],
            recent_blockhash: blockhash,
            signature: [0u8; 64],
        };
        tx.signature = kp.sign(&tx.signable_bytes());
        tx
    }

    #[test]
    fn test_transfer() {
        let mut ledger = Ledger::new();
        let alice = Keypair::generate();
        let bob   = Keypair::generate();
        ledger.airdrop(alice.address(), 1_000);

        let tx = make_tx(&alice, bob.address(), 400, 0, 0, [0u8; 32]);
        ledger.apply_transaction(&tx).unwrap();

        assert_eq!(ledger.get(&alice.address()).unwrap().balance, 600);
        assert_eq!(ledger.get(&bob.address()).unwrap().balance,   400);
    }

    #[test]
    fn test_insufficient_balance() {
        let mut ledger = Ledger::new();
        let alice = Keypair::generate();
        let bob   = Keypair::generate();
        ledger.airdrop(alice.address(), 100);

        let tx = make_tx(&alice, bob.address(), 500, 0, 0, [0u8; 32]);
        assert!(ledger.apply_transaction(&tx).is_err());
    }

    #[test]
    fn test_nonce_replay_protection() {
        let mut ledger = Ledger::new();
        let alice = Keypair::generate();
        let bob   = Keypair::generate();
        ledger.airdrop(alice.address(), 1_000);

        let tx = make_tx(&alice, bob.address(), 100, 0, 0, [0u8; 32]);
        ledger.apply_transaction(&tx).unwrap();
        // replaying the same nonce must fail
        assert!(ledger.apply_transaction(&tx).is_err());
    }

    #[test]
    fn test_invalid_signature() {
        let mut ledger = Ledger::new();
        let alice = Keypair::generate();
        let bob   = Keypair::generate();
        ledger.airdrop(alice.address(), 1_000);

        let mut tx = make_tx(&alice, bob.address(), 100, 0, 0, [0u8; 32]);
        tx.signature = [0u8; 64]; // corrupt signature
        assert!(ledger.apply_transaction(&tx).is_err());
    }

    #[test]
    fn test_state_root_deterministic() {
        let mut l1 = Ledger::new();
        let mut l2 = Ledger::new();
        let a = Keypair::generate();
        let b = Keypair::generate();
        l1.airdrop(a.address(), 500);
        l1.airdrop(b.address(), 300);
        l2.airdrop(b.address(), 300);
        l2.airdrop(a.address(), 500);
        assert_eq!(l1.state_root(), l2.state_root());
    }
}
