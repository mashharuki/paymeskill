use chrono::{DateTime, Utc};
use prometheus::{IntCounter, IntCounterVec, Opts, Registry};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use uuid::Uuid;

pub const PAYMENT_SIGNATURE_HEADER: &str = "payment-signature";
pub const PAYMENT_RESPONSE_HEADER: &str = "payment-response";
pub const X402_VERSION_HEADER: &str = "x402-version";
pub const DEFAULT_PRICE_CENTS: u64 = 5;
pub const SPONSORED_API_CREATE_SERVICE: &str = "sponsored-api-create";
pub const SPONSORED_API_SERVICE_PREFIX: &str = "sponsored-api";
pub const DEFAULT_SPONSORED_API_CREATE_PRICE_CENTS: u64 = 25;
pub const DEFAULT_SPONSORED_API_TIMEOUT_SECS: u64 = 12;
pub const DEFAULT_TESTNET_MIN_CONFIRMATIONS: u64 = 1;

#[derive(Clone)]
pub struct AppConfig {
    pub sponsored_api_create_price_cents: u64,
    pub sponsored_api_timeout_secs: u64,
    pub testnet_rpc_url: Option<String>,
    pub testnet_min_confirmations: u64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            sponsored_api_create_price_cents: read_env_u64(
                "SPONSORED_API_CREATE_PRICE_CENTS",
                DEFAULT_SPONSORED_API_CREATE_PRICE_CENTS,
            ),
            sponsored_api_timeout_secs: read_env_u64(
                "SPONSORED_API_TIMEOUT_SECS",
                DEFAULT_SPONSORED_API_TIMEOUT_SECS,
            ),
            testnet_rpc_url: std::env::var("TESTNET_RPC_URL").ok(),
            testnet_min_confirmations: read_env_u64(
                "TESTNET_MIN_CONFIRMATIONS",
                DEFAULT_TESTNET_MIN_CONFIRMATIONS,
            ),
        }
    }
}

#[derive(Clone)]
pub struct SharedState {
    pub inner: Arc<RwLock<AppState>>,
}

pub struct AppState {
    pub users: HashMap<Uuid, UserProfile>,
    pub campaigns: HashMap<Uuid, Campaign>,
    pub task_completions: Vec<TaskCompletion>,
    pub payments: HashMap<String, PaymentRecord>,
    pub creator_events: Vec<CreatorEvent>,
    pub service_prices: HashMap<String, u64>,
    pub metrics: Metrics,
    pub db: Option<PgPool>,
    pub http: Client,
    pub config: AppConfig,
}

#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    pub http_requests_total: IntCounterVec,
    pub payment_events_total: IntCounterVec,
    pub creator_events_total: IntCounterVec,
    pub sponsor_spend_cents_total: IntCounter,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            Opts::new("http_requests_total", "Total HTTP requests"),
            &["endpoint", "status"],
        )
        .expect("http counter vec should build");

        let payment_events_total = IntCounterVec::new(
            Opts::new("payment_events_total", "Payment events"),
            &["mode", "status"],
        )
        .expect("payment counter vec should build");

        let creator_events_total = IntCounterVec::new(
            Opts::new("creator_events_total", "Creator skill metric events"),
            &["skill", "platform", "event_type"],
        )
        .expect("creator counter vec should build");

        let sponsor_spend_cents_total = IntCounter::new(
            "sponsor_spend_cents_total",
            "Total sponsored spend in cents",
        )
        .expect("sponsor counter should build");

        registry
            .register(Box::new(http_requests_total.clone()))
            .expect("register http counter vec");
        registry
            .register(Box::new(payment_events_total.clone()))
            .expect("register payment counter vec");
        registry
            .register(Box::new(creator_events_total.clone()))
            .expect("register creator counter vec");
        registry
            .register(Box::new(sponsor_spend_cents_total.clone()))
            .expect("register sponsor spend counter");

        Self {
            registry,
            http_requests_total,
            payment_events_total,
            creator_events_total,
            sponsor_spend_cents_total,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        let mut service_prices = HashMap::new();
        service_prices.insert("scraping".to_string(), 5);
        service_prices.insert("design".to_string(), 8);
        service_prices.insert("storage".to_string(), 3);
        service_prices.insert("data-tooling".to_string(), 4);

        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client should build");

        let config = AppConfig::from_env();
        let db = std::env::var("DATABASE_URL").ok().and_then(|url| {
            PgPoolOptions::new()
                .max_connections(10)
                .connect_lazy(&url)
                .ok()
        });

        Self {
            users: HashMap::new(),
            campaigns: HashMap::new(),
            task_completions: Vec::new(),
            payments: HashMap::new(),
            creator_events: Vec::new(),
            service_prices,
            metrics: Metrics::new(),
            db,
            http,
            config,
        }
    }

    pub fn service_price(&self, service: &str) -> u64 {
        self.service_prices
            .get(service)
            .copied()
            .unwrap_or(DEFAULT_PRICE_CENTS)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub region: String,
    pub roles: Vec<String>,
    pub tools_used: Vec<String>,
    #[sqlx(json)]
    pub attributes: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub region: String,
    pub roles: Vec<String>,
    pub tools_used: Vec<String>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id: Uuid,
    pub name: String,
    pub sponsor: String,
    pub target_roles: Vec<String>,
    pub target_tools: Vec<String>,
    pub required_task: String,
    pub subsidy_per_call_cents: u64,
    pub budget_remaining_cents: u64,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCampaignRequest {
    pub name: String,
    pub sponsor: String,
    #[serde(default)]
    pub target_roles: Vec<String>,
    #[serde(default)]
    pub target_tools: Vec<String>,
    pub required_task: String,
    pub subsidy_per_call_cents: u64,
    pub budget_cents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompletion {
    pub id: Uuid,
    pub campaign_id: Uuid,
    pub user_id: Uuid,
    pub task_name: String,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct TaskCompletionRequest {
    pub campaign_id: Uuid,
    pub user_id: Uuid,
    pub task_name: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRunRequest {
    pub user_id: Uuid,
    pub input: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceRunResponse {
    pub service: String,
    pub output: String,
    pub payment_mode: String,
    pub sponsored_by: Option<String>,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequired {
    pub service: String,
    pub amount_cents: u64,
    pub accepted_header: String,
    pub message: String,
    pub next_step: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentProof {
    pub tx_hash: String,
    pub service: String,
    pub amount_cents: u64,
    pub payer: String,
    pub sponsored_campaign_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentSource {
    User,
    Sponsor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Settled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRecord {
    pub tx_hash: String,
    pub campaign_id: Option<Uuid>,
    pub service: String,
    pub amount_cents: u64,
    pub payer: String,
    pub source: PaymentSource,
    pub status: PaymentStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct PaymentSettlement {
    pub tx_hash: String,
    pub status: PaymentStatus,
    pub settled_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTestnetPaymentRequest {
    pub payer: String,
    pub service: String,
    pub tx_hash: String,
    pub amount_cents: Option<u64>,
    pub min_confirmations: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TestnetPaymentResponse {
    pub tx_hash: String,
    pub payment_signature: String,
    pub confirmations: u64,
}

#[derive(Debug, Deserialize)]
pub struct X402ScanSettlementRequest {
    pub tx_hash: String,
    pub service: String,
    pub amount_cents: u64,
    pub payer: String,
    pub source: PaymentSource,
    pub status: PaymentStatus,
    pub campaign_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatorEvent {
    pub id: Uuid,
    pub skill_name: String,
    pub platform: String,
    pub event_type: String,
    pub duration_ms: Option<u64>,
    pub success: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatorMetricEventRequest {
    pub skill_name: String,
    pub platform: String,
    pub event_type: String,
    pub duration_ms: Option<u64>,
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct CreatorMetricSummary {
    pub total_events: usize,
    pub success_events: usize,
    pub success_rate: f64,
    pub per_skill: Vec<SkillMetrics>,
}

#[derive(Debug, Serialize)]
pub struct SkillMetrics {
    pub skill_name: String,
    pub total_events: usize,
    pub success_events: usize,
    pub avg_duration_ms: Option<f64>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SponsorDashboard {
    pub campaign: Campaign,
    pub tasks_completed: usize,
    pub sponsored_calls: usize,
    pub spend_cents: u64,
    pub remaining_budget_cents: u64,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsoredApi {
    pub id: Uuid,
    pub name: String,
    pub sponsor: String,
    pub description: Option<String>,
    pub upstream_url: String,
    pub upstream_method: String,
    #[serde(default)]
    pub upstream_headers: HashMap<String, String>,
    pub price_cents: u64,
    pub budget_total_cents: u64,
    pub budget_remaining_cents: u64,
    pub active: bool,
    pub service_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SponsoredApiRow {
    pub id: Uuid,
    pub name: String,
    pub sponsor: String,
    pub description: Option<String>,
    pub upstream_url: String,
    pub upstream_method: String,
    pub upstream_headers: sqlx::types::Json<HashMap<String, String>>,
    pub price_cents: i64,
    pub budget_total_cents: i64,
    pub budget_remaining_cents: i64,
    pub active: bool,
    pub service_key: String,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<SponsoredApiRow> for SponsoredApi {
    type Error = String;

    fn try_from(value: SponsoredApiRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            name: value.name,
            sponsor: value.sponsor,
            description: value.description,
            upstream_url: value.upstream_url,
            upstream_method: value.upstream_method,
            upstream_headers: value.upstream_headers.0,
            price_cents: u64::try_from(value.price_cents)
                .map_err(|_| "price_cents must be non-negative".to_string())?,
            budget_total_cents: u64::try_from(value.budget_total_cents)
                .map_err(|_| "budget_total_cents must be non-negative".to_string())?,
            budget_remaining_cents: u64::try_from(value.budget_remaining_cents)
                .map_err(|_| "budget_remaining_cents must be non-negative".to_string())?,
            active: value.active,
            service_key: value.service_key,
            created_at: value.created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateSponsoredApiRequest {
    pub name: String,
    pub sponsor: String,
    pub description: Option<String>,
    pub upstream_url: String,
    #[serde(default)]
    pub upstream_method: Option<String>,
    #[serde(default)]
    pub upstream_headers: HashMap<String, String>,
    #[serde(default)]
    pub price_cents: Option<u64>,
    pub budget_cents: u64,
}

#[derive(Debug, Deserialize)]
pub struct SponsoredApiRunRequest {
    #[serde(default)]
    pub caller: Option<String>,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Serialize)]
pub struct SponsoredApiRunResponse {
    pub api_id: Uuid,
    pub payment_mode: String,
    pub sponsored_by: Option<String>,
    pub tx_hash: Option<String>,
    pub upstream_status: u16,
    pub upstream_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsoredApiCall {
    pub id: Uuid,
    pub sponsored_api_id: Uuid,
    pub payment_mode: String,
    pub amount_cents: u64,
    pub tx_hash: Option<String>,
    pub caller: Option<String>,
    pub created_at: DateTime<Utc>,
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}
