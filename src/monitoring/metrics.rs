//! HTTP metrics/dashboard endpoint (JSON).

use crate::learning::{LearningPolicy, Lesson};
use axum::{extract::State, routing::get, Json, Router};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::info;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub mode: String,
    pub equity: f64,
    pub peak_equity: f64,
    pub open_positions: u32,
    pub daily_pnl: f64,
    pub trades_today: u64,
    pub signals_today: u64,
    pub llm_go: u64,
    pub llm_nogo: u64,
    pub llm_wait: u64,
    pub llm_avg_confidence: f64,
    pub llm_avg_latency_ms: u64,
    pub llm_offline_fallbacks: u64,
    pub active_lessons: u64,
    pub last_update_ts: i64,
}

pub struct MetricsState {
    inner: RwLock<MetricsSnapshot>,
}

impl MetricsState {
    pub fn new(mode: &str) -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(MetricsSnapshot {
                mode: mode.to_string(),
                ..Default::default()
            }),
        })
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        self.inner.read().clone()
    }

    pub fn update<F: FnOnce(&mut MetricsSnapshot)>(&self, f: F) {
        let mut w = self.inner.write();
        f(&mut w);
        w.last_update_ts = chrono::Utc::now().timestamp();
    }
}

#[derive(Clone)]
pub struct DashboardState {
    pub metrics: Arc<MetricsState>,
    pub policy: Option<LearningPolicy>,
}

#[derive(Debug, Clone, Serialize)]
struct DashboardResponse {
    metrics: MetricsSnapshot,
    lessons: Vec<Lesson>,
}

pub fn spawn_metrics_server(state: Arc<MetricsState>, bind: SocketAddr) -> JoinHandle<()> {
    spawn_dashboard_server(
        DashboardState {
            metrics: state,
            policy: None,
        },
        bind,
    )
}

pub fn spawn_dashboard_server(state: DashboardState, bind: SocketAddr) -> JoinHandle<()> {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/healthz", get(|| async { "ok" }))
        .route("/metrics", get(metrics_handler))
        .route("/lessons", get(lessons_handler))
        .route("/dashboard", get(dashboard_handler))
        .with_state(state);
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(bind).await {
            Ok(listener) => {
                info!(%bind, "metrics server listening");
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!(error = %e, "metrics server");
                }
            }
            Err(e) => {
                tracing::error!(%bind, error = %e, "cannot bind metrics server");
            }
        }
    })
}

async fn root_handler() -> &'static str {
    "ARIA metrics — see /metrics, /lessons, /dashboard, /healthz"
}

async fn metrics_handler(State(state): State<DashboardState>) -> Json<MetricsSnapshot> {
    Json(state.metrics.snapshot())
}

async fn lessons_handler(State(state): State<DashboardState>) -> Json<Vec<Lesson>> {
    Json(
        state
            .policy
            .as_ref()
            .map(|p| p.active_lessons())
            .unwrap_or_default(),
    )
}

async fn dashboard_handler(State(state): State<DashboardState>) -> Json<DashboardResponse> {
    Json(DashboardResponse {
        metrics: state.metrics.snapshot(),
        lessons: state
            .policy
            .as_ref()
            .map(|p| p.active_lessons())
            .unwrap_or_default(),
    })
}
