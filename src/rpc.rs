use std::sync::{Arc, Mutex};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use crate::{chain::Chain, mempool::Mempool, types::*};

pub type SharedChain   = Arc<Mutex<Chain>>;
pub type SharedMempool = Arc<Mutex<Mempool>>;
pub type AppState      = (SharedChain, SharedMempool);

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub status:       &'static str,
    pub slot:         Slot,
    pub block_height: usize,
    pub total_supply: TokenAmount,
    pub pending_txs:  usize,
}

#[derive(Serialize)]
pub struct SlotResponse {
    pub slot:     Slot,
    pub poh_hash: String,
}

#[derive(Serialize)]
pub struct AccountResponse {
    pub address: String,
    pub balance: TokenAmount,
    pub nonce:   u64,
}

#[derive(Deserialize)]
pub struct SubmitTxRequest {
    pub transaction: Transaction,
}

#[derive(Serialize)]
pub struct SubmitTxResponse {
    pub accepted: bool,
    pub tx_hash:  String,
    pub reason:   Option<String>,
}

// ── Router ────────────────────────────────────────────────────────────────────

/// On-chain event response
#[derive(Serialize)]
pub struct EventResponse {
    pub slot:       Slot,
    pub program_id: String,
    pub name:       String,
    pub data_hex:   String,
}

/// PDA derivation request/response
#[derive(Deserialize)]
pub struct PdaRequest {
    pub seeds:      Vec<String>, // hex-encoded seed bytes
    pub program_id: String,
}

#[derive(Serialize)]
pub struct PdaResponse {
    pub address: String,
    pub bump:    u8,
}

pub fn router(chain: SharedChain, mempool: SharedMempool) -> Router {
    Router::new()
        .route("/health",              get(health))
        .route("/slot",                get(current_slot))
        .route("/block/:slot",         get(get_block))
        .route("/account/:address",    get(get_account))
        .route("/transaction",         post(submit_tx))
        .route("/events",              get(get_recent_events))
        .route("/events/:slot",        get(get_events_at_slot))
        .route("/pda",                 post(derive_pda))
        .with_state((chain, mempool))
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn health(State((chain, mempool)): State<AppState>) -> Json<HealthResponse> {
    let c = chain.lock().unwrap();
    let m = mempool.lock().unwrap();
    Json(HealthResponse {
        status:       "ok",
        slot:         c.slot,
        block_height: c.height(),
        total_supply: c.ledger.total_supply(),
        pending_txs:  m.len(),
    })
}

async fn current_slot(State((chain, _)): State<AppState>) -> Json<SlotResponse> {
    let c = chain.lock().unwrap();
    Json(SlotResponse { slot: c.slot, poh_hash: hex_hash(&c.poh_hash) })
}

async fn get_block(
    State((chain, _)): State<AppState>,
    Path(slot): Path<u64>,
) -> Result<Json<Block>, (StatusCode, String)> {
    let c = chain.lock().unwrap();
    c.get_block(slot)
        .cloned()
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("block {slot} not found")))
}

async fn get_account(
    State((chain, _)): State<AppState>,
    Path(address_hex): Path<String>,
) -> Result<Json<AccountResponse>, (StatusCode, String)> {
    let bytes = hex::decode(&address_hex)
        .map_err(|_| (StatusCode::BAD_REQUEST, "address must be hex".into()))?;
    if bytes.len() != 32 {
        return Err((StatusCode::BAD_REQUEST, "address must be 32 bytes (64 hex chars)".into()));
    }
    let mut addr = [0u8; 32];
    addr.copy_from_slice(&bytes);

    let c = chain.lock().unwrap();
    c.ledger.get(&addr)
        .map(|a| Json(AccountResponse {
            address: hex_address(&a.address),
            balance: a.balance,
            nonce:   a.nonce,
        }))
        .ok_or((StatusCode::NOT_FOUND, "account not found".into()))
}

async fn submit_tx(
    State((_, mempool)): State<AppState>,
    Json(req): Json<SubmitTxRequest>,
) -> Json<SubmitTxResponse> {
    let tx_hash = hex_hash(&req.transaction.hash());
    let accepted = mempool.lock().unwrap().push(req.transaction);
    Json(SubmitTxResponse {
        accepted,
        tx_hash,
        reason: if accepted { None } else { Some("mempool full or duplicate".into()) },
    })
}

async fn get_recent_events(State((chain, _)): State<AppState>) -> Json<Vec<EventResponse>> {
    let c = chain.lock().unwrap();
    let events = c.recent_events(50).into_iter().map(event_to_resp).collect();
    Json(events)
}

async fn get_events_at_slot(
    State((chain, _)): State<AppState>,
    Path(slot): Path<u64>,
) -> Json<Vec<EventResponse>> {
    let c = chain.lock().unwrap();
    let events = c.events_at_slot(slot).iter().map(event_to_resp).collect();
    Json(events)
}

async fn derive_pda(
    State((_, _)): State<AppState>,
    Json(req): Json<PdaRequest>,
) -> Result<Json<PdaResponse>, (StatusCode, String)> {
    use crate::crypto::find_pda;
    let prog_bytes = hex::decode(&req.program_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "program_id must be hex".into()))?;
    if prog_bytes.len() != 32 {
        return Err((StatusCode::BAD_REQUEST, "program_id must be 32 bytes".into()));
    }
    let mut program_id = [0u8; 32];
    program_id.copy_from_slice(&prog_bytes);

    let decoded_seeds: Result<Vec<Vec<u8>>, _> = req.seeds.iter().map(|s| hex::decode(s)).collect();
    let decoded_seeds = decoded_seeds
        .map_err(|_| (StatusCode::BAD_REQUEST, "seeds must be hex-encoded".into()))?;
    let seed_slices: Vec<&[u8]> = decoded_seeds.iter().map(|v| v.as_slice()).collect();

    let (addr, bump) = find_pda(&seed_slices, &program_id);
    Ok(Json(PdaResponse { address: hex_address(&addr), bump }))
}

fn event_to_resp(e: &crate::types::Event) -> EventResponse {
    EventResponse {
        slot:       e.slot,
        program_id: hex_address(&e.program_id),
        name:       e.name.clone(),
        data_hex:   hex::encode(&e.data),
    }
}
