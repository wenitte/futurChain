use crate::crypto;
use crate::ledger::Ledger;
use crate::types::{Address, Block, BlockHeader, Event, Hash, Slot, Transaction};

pub struct Chain {
    pub blocks:      Vec<Block>,
    pub ledger:      Ledger,
    pub slot:        Slot,
    pub poh_hash:    Hash,
    /// Global event log across all slots — indexed for fast lookup
    pub event_log:   Vec<Event>,
}

impl Chain {
    pub fn new() -> Self {
        let genesis  = Block::genesis();
        let poh_hash = genesis.header.poh_hash;
        Self { blocks: vec![genesis], ledger: Ledger::new(), slot: 0, poh_hash, event_log: vec![] }
    }

    pub fn tip_hash(&self) -> Hash {
        self.blocks.last().map(|b| b.hash).unwrap_or([0u8; 32])
    }

    pub fn get_block(&self, slot: Slot) -> Option<&Block> {
        self.blocks.get(slot as usize)
    }

    /// Produce the next block, apply valid transactions, advance PoH
    pub fn produce_block(&mut self, transactions: Vec<Transaction>, proposer: Address) -> Block {
        self.slot    += 1;
        self.poh_hash = crypto::poh_tick(self.poh_hash);

        let parent_hash = self.tip_hash();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Execute transactions; skip invalid ones (they stay out of the block)
        let mut valid_txs = Vec::new();
        for tx in transactions {
            if self.ledger.apply_transaction(&tx).is_ok() {
                valid_txs.push(tx);
            }
        }

        let tx_root    = crypto::hash_transactions(&valid_txs);
        let state_root = self.ledger.state_root();

        let header = BlockHeader {
            slot: self.slot,
            parent_hash,
            poh_hash: self.poh_hash,
            tx_root,
            state_root,
            proposer,
            timestamp,
            tx_count: valid_txs.len() as u32,
        };

        let hash  = Block::compute_hash(&header);
        let block = Block { header, transactions: valid_txs, events: vec![], hash };
        self.blocks.push(block.clone());
        block
    }

    pub fn height(&self) -> usize { self.blocks.len() }

    /// All events emitted by programs in a given slot's block
    pub fn events_at_slot(&self, slot: Slot) -> &[Event] {
        self.blocks.get(slot as usize)
            .map(|b| b.events.as_slice())
            .unwrap_or(&[])
    }

    /// Most recent N events across all slots (newest first)
    pub fn recent_events(&self, limit: usize) -> Vec<&Event> {
        self.event_log.iter().rev().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    #[test]
    fn test_genesis() {
        let chain = Chain::new();
        assert_eq!(chain.slot, 0);
        assert_eq!(chain.height(), 1);
        let genesis = chain.get_block(0).unwrap();
        assert_eq!(genesis.header.slot, 0);
    }

    #[test]
    fn test_block_production() {
        let mut chain = Chain::new();
        let proposer = Keypair::generate();
        chain.ledger.airdrop(proposer.address(), 1_000_000);

        let block = chain.produce_block(vec![], proposer.address());
        assert_eq!(block.header.slot, 1);
        assert_eq!(chain.height(), 2);
        // PoH must have advanced
        assert_ne!(chain.poh_hash, [0u8; 32]);
    }

    #[test]
    fn test_chain_with_transfer() {
        let mut chain = Chain::new();
        let alice     = Keypair::generate();
        let bob       = Keypair::generate();
        chain.ledger.airdrop(alice.address(), 1_000);

        // Build a signed transfer
        let mut tx = crate::types::Transaction {
            nonce: 0, sender: alice.address(), recipient: bob.address(),
            amount: 250, fee: 0, instructions: vec![],
            recent_blockhash: chain.tip_hash(), signature: [0u8; 64],
        };
        tx.signature = alice.sign(&tx.signable_bytes());

        let block = chain.produce_block(vec![tx], alice.address());
        assert_eq!(block.header.tx_count, 1);
        assert_eq!(chain.ledger.get(&alice.address()).unwrap().balance, 750);
        assert_eq!(chain.ledger.get(&bob.address()).unwrap().balance,   250);
    }

    #[test]
    fn test_poh_chain() {
        let mut chain = Chain::new();
        let node = Keypair::generate();
        let poh0 = chain.poh_hash;
        chain.produce_block(vec![], node.address());
        let poh1 = chain.poh_hash;
        chain.produce_block(vec![], node.address());
        let poh2 = chain.poh_hash;

        // Each slot's PoH hash is derived from the previous
        assert_eq!(poh1, crypto::poh_tick(poh0));
        assert_eq!(poh2, crypto::poh_tick(poh1));
    }
}
