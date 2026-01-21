//! Axum HTTP RPC handlers for the op-conductor.
//!
//! Provides JSON-RPC style endpoints for interacting with the conductor:
//! - `GET /health` - Health check
//! - `GET /leader` - Current leader status
//! - `POST /commit` - Submit payload (sequencer only)
//! - `POST /acknowledge` - Validator acknowledgment
//! - `GET /latest` - Latest certified payload
//! - `GET /payload/:height` - Get payload by height

use arturo::{Conductor, Payload};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use commonware_cryptography::ed25519;
use serde::{Deserialize, Serialize};

use crate::{
    epoch::HealthBasedEpochManager,
    health::{HealthState, health_handler},
    payload::OpPayload,
};

/// Type alias for the conductor with our concrete types.
pub type OpConductor = Conductor<OpPayload, HealthBasedEpochManager, ed25519::PrivateKey>;

/// Shared application state for axum handlers.
#[derive(Clone)]
pub struct AppState {
    /// The conductor instance.
    pub conductor: OpConductor,
    /// Health state for the /health endpoint.
    pub health: HealthState,
}

/// Leader status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderStatus {
    /// Whether this node is currently the leader.
    pub is_leader: bool,
    /// Current epoch.
    pub epoch: u64,
    /// Next expected payload height.
    pub next_height: u64,
}

/// Commit request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRequest {
    /// The payload to commit.
    pub payload: OpPayload,
}

/// Commit response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResponse {
    /// Whether the commit was successful.
    pub success: bool,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Acknowledge response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcknowledgeResponse {
    /// Whether a payload was certified as a result of this acknowledgment.
    pub certified: bool,
    /// The certified payload height if certified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
}

/// Error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
}

/// Creates the axum router with all RPC endpoints.
pub fn create_router(conductor: OpConductor, health_state: HealthState) -> Router {
    let state = AppState { conductor, health: health_state.clone() };

    Router::new()
        .route("/health", get(health_handler))
        .with_state(health_state)
        .route("/leader", get(leader_handler))
        .route("/commit", post(commit_handler))
        .route("/acknowledge", post(acknowledge_handler))
        .route("/latest", get(latest_handler))
        .route("/payload/{height}", get(payload_by_height_handler))
        .with_state(state)
}

/// Handler for `GET /leader`.
async fn leader_handler(State(state): State<AppState>) -> impl IntoResponse {
    let is_leader = state.conductor.leader().await;
    let epoch = state.conductor.current_epoch().await;
    let next_height = state.conductor.next_height().await;

    Json(LeaderStatus { is_leader, epoch, next_height })
}

/// Handler for `POST /commit`.
async fn commit_handler(
    State(state): State<AppState>,
    Json(request): Json<CommitRequest>,
) -> impl IntoResponse {
    match state.conductor.commit(request.payload).await {
        Ok(()) => (StatusCode::OK, Json(CommitResponse { success: true, error: None })),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(CommitResponse { success: false, error: Some(e.to_string()) }),
        ),
    }
}

/// Handler for `POST /acknowledge`.
async fn acknowledge_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.conductor.acknowledge().await {
        Some(payload) => (
            StatusCode::OK,
            Json(AcknowledgeResponse { certified: true, height: Some(payload.height()) }),
        ),
        None => (StatusCode::OK, Json(AcknowledgeResponse { certified: false, height: None })),
    }
}

/// Handler for `GET /latest`.
async fn latest_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.conductor.latest().await {
        Some(payload) => (StatusCode::OK, Json(payload)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: "no certified payloads".to_string() }),
        )
            .into_response(),
    }
}

/// Handler for `GET /payload/:height`.
async fn payload_by_height_handler(
    State(state): State<AppState>,
    Path(height): Path<u64>,
) -> impl IntoResponse {
    match state.conductor.get_by_height(height).await {
        Some(payload) => (StatusCode::OK, Json(payload)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: format!("payload at height {height} not found") }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_response_serde() {
        let response = CommitResponse { success: true, error: None };
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("error")); // error should be skipped when None

        let response = CommitResponse { success: false, error: Some("test error".to_string()) };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test error"));
    }

    #[test]
    fn test_leader_status_serde() {
        let status = LeaderStatus { is_leader: true, epoch: 42, next_height: 100 };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: LeaderStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.is_leader, status.is_leader);
        assert_eq!(parsed.epoch, status.epoch);
        assert_eq!(parsed.next_height, status.next_height);
    }
}
