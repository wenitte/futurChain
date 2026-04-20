// futurchain — Solana-inspired blockchain
// This crate is also the SDK that FL-compiled programs link against.

pub mod chain;
pub mod crypto;
pub mod ledger;
pub mod mempool;
pub mod rpc;
pub mod types;

/// Prelude for FL-compiled programs
pub mod prelude {
    pub use crate::types::*;
    pub use crate::crypto::{sha256, poh_tick, verify_signature};

    pub type Result<T> = std::result::Result<T, ProgramError>;

    #[derive(Debug, thiserror::Error)]
    pub enum ProgramError {
        #[error("account not found at index {0}")]
        AccountNotFound(usize),
        #[error("not a signer")]
        NotSigner,
        #[error("insufficient funds")]
        InsufficientFunds,
        #[error("arithmetic overflow")]
        Overflow,
        #[error("{0}")]
        Custom(String),
    }

    impl From<&str> for ProgramError {
        fn from(s: &str) -> Self { ProgramError::Custom(s.to_string()) }
    }

    /// Execution context passed to every instruction handler
    pub struct Context {
        pub program_id: Address,
        pub accounts:   Vec<AccountInfo>,
    }

    /// Live view of an account passed into an instruction
    #[derive(Debug, Clone)]
    pub struct AccountInfo {
        pub address:     Address,
        pub balance:     TokenAmount,
        pub is_signer:   bool,
        pub is_writable: bool,
        pub data:        Vec<u8>,
    }

    impl Context {
        pub fn signer(&self, idx: usize) -> Result<Address> {
            let acc = self.accounts.get(idx).ok_or(ProgramError::AccountNotFound(idx))?;
            if !acc.is_signer { return Err(ProgramError::NotSigner); }
            Ok(acc.address)
        }

        pub fn account(&self, idx: usize) -> Result<&AccountInfo> {
            self.accounts.get(idx).ok_or(ProgramError::AccountNotFound(idx))
        }

        pub fn account_mut(&mut self, idx: usize) -> Result<&mut AccountInfo> {
            self.accounts.get_mut(idx).ok_or(ProgramError::AccountNotFound(idx))
        }
    }

    /// Every FL program implements this trait
    pub trait Program {
        fn process(ctx: &mut Context, data: &[u8]) -> Result<()>;
    }

    /// require!(condition, ErrorVariant) — guard macro used by FL programs
    #[macro_export]
    macro_rules! require {
        ($cond:expr, $err:expr) => {
            if !($cond) { return Err($err.into()); }
        };
    }

    /// program_id!("hex") — declare program address
    #[macro_export]
    macro_rules! program_id {
        ($hex:literal) => {
            pub const PROGRAM_ID: $crate::prelude::Address = [0u8; 32]; // set after deployment
        };
    }
}

/// Runtime module for node/chain code (not programs)
pub mod runtime {
    pub use crate::chain::Chain;
    pub use crate::crypto::{poh_tick, sha256, Keypair};
    pub use crate::ledger::{Ledger, LedgerError};
    pub use crate::mempool::Mempool;
    pub use crate::types::*;
}
