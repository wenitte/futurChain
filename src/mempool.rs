use std::collections::{HashMap, VecDeque};
use crate::types::{Hash, Transaction};

pub struct Mempool {
    queue:    VecDeque<Transaction>,
    seen:     HashMap<Hash, bool>, // dedup by tx hash
    max_size: usize,
}

impl Mempool {
    pub fn new(max_size: usize) -> Self {
        Self { queue: VecDeque::new(), seen: HashMap::new(), max_size }
    }

    /// Returns false if full or duplicate
    pub fn push(&mut self, tx: Transaction) -> bool {
        if self.queue.len() >= self.max_size { return false; }
        let h = tx.hash();
        if self.seen.contains_key(&h) { return false; }
        self.seen.insert(h, true);
        self.queue.push_back(tx);
        true
    }

    /// Drain up to `max` transactions for block production
    pub fn drain(&mut self, max: usize) -> Vec<Transaction> {
        let count = max.min(self.queue.len());
        self.queue.drain(..count).collect()
    }

    pub fn len(&self) -> usize { self.queue.len() }
    pub fn is_empty(&self) -> bool { self.queue.is_empty() }
}
