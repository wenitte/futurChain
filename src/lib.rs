// futurchain — proof-native blockchain (Solana-inspired)
// This crate doubles as the SDK that FL-compiled programs link against.

pub mod chain;
pub mod crypto;
pub mod ledger;
pub mod mempool;
pub mod rpc;
pub mod types;

/// Prelude for FL-compiled programs (smart contracts / on-chain code)
pub mod prelude {
    pub use crate::types::*;
    pub use crate::crypto::{sha256, poh_tick, verify_signature, pda_derive, find_pda};
    pub use serde::{Serialize, Deserialize};

    pub type Result<T> = std::result::Result<T, ProgramError>;

    // ── Program errors ────────────────────────────────────────────────────────

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
        #[error("account is not writable")]
        NotWritable,
        #[error("invalid PDA seeds")]
        InvalidSeeds,
        #[error("cpi error: {0}")]
        CpiError(String),
        #[error("{0}")]
        Custom(String),
    }

    impl From<&str> for ProgramError { fn from(s: &str) -> Self { ProgramError::Custom(s.to_string()) } }
    impl From<String> for ProgramError { fn from(s: String) -> Self { ProgramError::Custom(s) } }

    // ── Sysvar clock (slot-based time) ────────────────────────────────────────

    #[derive(Debug, Clone, Copy)]
    pub struct Clock {
        pub slot:      Slot,
        pub timestamp: u64,
    }

    // ── Live account view ─────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    pub struct AccountInfo {
        pub address:     Address,
        pub balance:     TokenAmount,
        pub is_signer:   bool,
        pub is_writable: bool,
        pub owner:       Address,
        pub data:        Vec<u8>,
    }

    impl AccountInfo {
        /// Deserialize account data as a typed state struct.
        pub fn deserialize<T: for<'de> serde::Deserialize<'de>>(&self) -> Result<T> {
            serde_json::from_slice(&self.data)
                .map_err(|e| ProgramError::Custom(e.to_string()))
        }

        /// Serialize typed state back into account data.
        pub fn serialize<T: serde::Serialize>(&mut self, state: &T) -> Result<()> {
            self.data = serde_json::to_vec(state)
                .map_err(|e| ProgramError::Custom(e.to_string()))?;
            Ok(())
        }
    }

    // ── CPI invocation descriptor ─────────────────────────────────────────────

    pub struct CpiInstruction {
        pub program_id: Address,
        pub accounts:   Vec<AccountInfo>,
        pub data:       Vec<u8>,
    }

    // ── Emitted event ─────────────────────────────────────────────────────────

    pub struct EmittedEvent {
        pub name: String,
        pub data: Vec<u8>,
    }

    // ── Execution context ─────────────────────────────────────────────────────

    pub struct Context {
        pub program_id: Address,
        pub accounts:   Vec<AccountInfo>,
        pub clock:      Clock,
        pub events:     Vec<EmittedEvent>,
        pub cpi_calls:  Vec<CpiInstruction>,
    }

    impl Context {
        pub fn new(program_id: Address, accounts: Vec<AccountInfo>, slot: Slot, timestamp: u64) -> Self {
            Self {
                program_id,
                accounts,
                clock: Clock { slot, timestamp },
                events: vec![],
                cpi_calls: vec![],
            }
        }

        /// Get the address of a signer account at `idx`.
        pub fn signer(&self, idx: usize) -> Result<Address> {
            let acc = self.accounts.get(idx).ok_or(ProgramError::AccountNotFound(idx))?;
            if !acc.is_signer { return Err(ProgramError::NotSigner); }
            Ok(acc.address)
        }

        /// Borrow account at `idx` immutably.
        pub fn account(&self, idx: usize) -> Result<&AccountInfo> {
            self.accounts.get(idx).ok_or(ProgramError::AccountNotFound(idx))
        }

        /// Borrow account at `idx` mutably (checks writable flag).
        pub fn account_mut(&mut self, idx: usize) -> Result<&mut AccountInfo> {
            let acc = self.accounts.get_mut(idx).ok_or(ProgramError::AccountNotFound(idx))?;
            if !acc.is_writable { return Err(ProgramError::NotWritable); }
            Ok(acc)
        }

        /// Deserialize typed state from account at `idx`.
        pub fn load<T: for<'de> serde::Deserialize<'de>>(&self, idx: usize) -> Result<T> {
            self.account(idx)?.deserialize::<T>()
        }

        /// Save typed state back to account at `idx`.
        pub fn save<T: serde::Serialize>(&mut self, idx: usize, state: &T) -> Result<()> {
            let acc = self.accounts.get_mut(idx).ok_or(ProgramError::AccountNotFound(idx))?;
            if !acc.is_writable { return Err(ProgramError::NotWritable); }
            acc.serialize(state)
        }

        /// Derive a PDA from seeds relative to this program.
        pub fn pda(&self, seeds: &[&[u8]]) -> Address {
            pda_derive(seeds, &self.program_id)
        }

        /// Emit a named event (recorded in block event log).
        pub fn emit(&mut self, name: &str, data: Vec<u8>) {
            self.events.push(EmittedEvent { name: name.to_string(), data });
        }

        /// Queue a cross-program invocation.
        pub fn cpi(&mut self, ix: CpiInstruction) {
            self.cpi_calls.push(ix);
        }

        /// Native token transfer between two accounts within this instruction.
        pub fn transfer(&mut self, from_idx: usize, to_idx: usize, amount: TokenAmount) -> Result<()> {
            let from_bal = {
                let from = self.accounts.get(from_idx).ok_or(ProgramError::AccountNotFound(from_idx))?;
                if !from.is_writable { return Err(ProgramError::NotWritable); }
                from.balance
            };
            if from_bal < amount { return Err(ProgramError::InsufficientFunds); }
            self.accounts[from_idx].balance -= amount;
            let to = self.accounts.get_mut(to_idx).ok_or(ProgramError::AccountNotFound(to_idx))?;
            to.balance = to.balance.checked_add(amount).ok_or(ProgramError::Overflow)?;
            Ok(())
        }
    }

    // ── Program trait ─────────────────────────────────────────────────────────

    pub trait Program {
        fn process(ctx: &mut Context, data: &[u8]) -> Result<()>;
    }

    // ── Macros ────────────────────────────────────────────────────────────────

    /// require!(condition, ErrorVariant) — guard; returns program error on failure
    #[macro_export]
    macro_rules! require {
        ($cond:expr, $err:expr) => {
            if !($cond) { return Err($err.into()); }
        };
    }

    /// emit!(ctx, "EventName", &payload) — emit a serializable event
    #[macro_export]
    macro_rules! emit {
        ($ctx:expr, $name:expr, $data:expr) => {{
            let bytes = serde_json::to_vec($data).unwrap_or_default();
            $ctx.emit($name, bytes);
        }};
    }

    /// cpi!(ctx, program_id, accounts, data) — cross-program invocation
    #[macro_export]
    macro_rules! cpi {
        ($ctx:expr, $program_id:expr, $accounts:expr, $data:expr) => {{
            $ctx.cpi($crate::prelude::CpiInstruction {
                program_id: $program_id,
                accounts:   $accounts,
                data:       $data,
            });
        }};
    }

    /// program_id!("hex") — declare program address constant
    #[macro_export]
    macro_rules! program_id {
        ($hex:literal) => {
            pub const PROGRAM_ID: $crate::prelude::Address = [0u8; 32];
        };
    }
}

/// Runtime module — imported by node software, validators, consensus code
pub mod runtime {
    pub use crate::chain::Chain;
    pub use crate::crypto::{poh_tick, sha256, pda_derive, find_pda, Keypair};
    pub use crate::ledger::{Ledger, LedgerError};
    pub use crate::mempool::Mempool;
    pub use crate::types::*;
}
