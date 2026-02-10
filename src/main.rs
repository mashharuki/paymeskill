mod error;
mod onchain;
mod types;
mod utils;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use prometheus::{Encoder, TextEncoder};
use sqlx::types::Json as DbJson;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::types::*;
use crate::utils::*;

fn build_app(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/profiles", post(create_profile).get(list_profiles))
        .route("/register", post(register_user))
        .route("/campaigns", post(create_campaign).get(list_campaigns))
        .route("/tasks/complete", post(complete_task))
        .route("/tool/:service/run", post(run_tool))
        .route("/proxy/:service/run", post(run_proxy))
        .route(
            "/sponsored-apis",
            post(create_sponsored_api).get(list_sponsored_apis),
        )
        .route("/sponsored-apis/:api_id", get(get_sponsored_api))
        .route("/sponsored-apis/:api_id/run", post(run_sponsored_api))
        .route(
            "/webhooks/x402scan/settlement",
            post(ingest_x402scan_settlement),
        )
        .route("/dashboard/sponsor/:campaign_id", get(sponsor_dashboard))
        .route("/creator/metrics/event", post(record_creator_metric_event))
        .route("/creator/metrics", get(creator_metrics))
        .route("/metrics", get(prometheus_metrics))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "payloadexchange_mvp=info,tower_http=info".to_string()),
        )
        .with_target(false)
        .compact()
        .init();

    let state = SharedState {
        inner: Arc::new(RwLock::new(AppState::new())),
    };

    if let Some(db) = {
        let state = state.inner.read().await;
        state.db.clone()
    } {
        sqlx::migrate!("./migrations")
            .run(&db)
            .await
            .expect("database migrations should run");
    }

    let app = build_app(state);

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let address = SocketAddr::from(([0, 0, 0, 0], port));

    info!("payloadexchange-mvp listening on http://{}", address);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("bind should succeed");

    if let Err(err) = axum::serve(listener, app).await {
        eprintln!("server error: {err}");
    }
}

async fn health(State(state): State<SharedState>) -> Response {
    let state = state.inner.read().await;
    respond(
        &state.metrics,
        "/health",
        Ok((
            StatusCode::OK,
            Json(MessageResponse {
                message: "ok".to_string(),
            }),
        )),
    )
}

async fn create_profile(
    State(state): State<SharedState>,
    Json(payload): Json<CreateUserRequest>,
) -> Response {
    let mut state = state.inner.write().await;
    let profile = UserProfile {
        id: Uuid::new_v4(),
        email: payload.email,
        region: payload.region,
        roles: payload.roles,
        tools_used: payload.tools_used,
        attributes: payload.attributes,
        created_at: Utc::now(),
    };

    state.users.insert(profile.id, profile.clone());
    respond(
        &state.metrics,
        "/profiles",
        Ok((StatusCode::CREATED, Json(profile))),
    )
}

async fn list_profiles(State(state): State<SharedState>) -> Response {
    let state = state.inner.read().await;
    let mut profiles: Vec<UserProfile> = state.users.values().cloned().collect();
    profiles.sort_by_key(|profile| profile.created_at);
    respond(
        &state.metrics,
        "/profiles",
        Ok((StatusCode::OK, Json(profiles))),
    )
}

async fn register_user(
    State(state): State<SharedState>,
    Json(payload): Json<CreateUserRequest>,
) -> Response {
    let metrics = {
        let state = state.inner.read().await;
        state.metrics.clone()
    };

    let result: ApiResult<(StatusCode, Json<UserProfile>)> = async {
        let db = {
            let state = state.inner.read().await;
            state.db.clone()
        }
        .ok_or_else(|| ApiError::config("Postgres not configured; set DATABASE_URL"))?;

        if payload.email.trim().is_empty() {
            return Err(ApiError::validation("email is required"));
        }

        if payload.region.trim().is_empty() {
            return Err(ApiError::validation("region is required"));
        }

        let profile = UserProfile {
            id: Uuid::new_v4(),
            email: payload.email,
            region: payload.region,
            roles: payload.roles,
            tools_used: payload.tools_used,
            attributes: payload.attributes,
            created_at: Utc::now(),
        };

        let inserted = sqlx::query_as::<_, UserProfile>(
            r#"
            insert into users (id, email, region, roles, tools_used, attributes, created_at)
            values ($1, $2, $3, $4, $5, $6, $7)
            returning id, email, region, roles, tools_used, attributes, created_at
            "#,
        )
        .bind(profile.id)
        .bind(profile.email)
        .bind(profile.region)
        .bind(profile.roles)
        .bind(profile.tools_used)
        .bind(DbJson(profile.attributes))
        .bind(profile.created_at)
        .fetch_one(&db)
        .await
        .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        Ok((StatusCode::CREATED, Json(inserted)))
    }
    .await;

    respond(&metrics, "/register", result)
}

async fn create_campaign(
    State(state): State<SharedState>,
    Json(payload): Json<CreateCampaignRequest>,
) -> Response {
    let mut state = state.inner.write().await;

    let campaign = Campaign {
        id: Uuid::new_v4(),
        name: payload.name,
        sponsor: payload.sponsor,
        target_roles: payload.target_roles,
        target_tools: payload.target_tools,
        required_task: payload.required_task,
        subsidy_per_call_cents: payload.subsidy_per_call_cents,
        budget_remaining_cents: payload.budget_cents,
        active: true,
        created_at: Utc::now(),
    };

    state.campaigns.insert(campaign.id, campaign.clone());
    respond(
        &state.metrics,
        "/campaigns",
        Ok((StatusCode::CREATED, Json(campaign))),
    )
}

async fn list_campaigns(State(state): State<SharedState>) -> Response {
    let state = state.inner.read().await;
    let mut campaigns: Vec<Campaign> = state.campaigns.values().cloned().collect();
    campaigns.sort_by_key(|campaign| campaign.created_at);
    respond(
        &state.metrics,
        "/campaigns",
        Ok((StatusCode::OK, Json(campaigns))),
    )
}

async fn complete_task(
    State(state): State<SharedState>,
    Json(payload): Json<TaskCompletionRequest>,
) -> Response {
    let mut state = state.inner.write().await;

    if !state.campaigns.contains_key(&payload.campaign_id) {
        return respond(
            &state.metrics,
            "/tasks/complete",
            Err::<Response, ApiError>(ApiError::not_found("campaign not found")),
        );
    }

    if !state.users.contains_key(&payload.user_id) {
        return respond(
            &state.metrics,
            "/tasks/complete",
            Err::<Response, ApiError>(ApiError::not_found("user not found")),
        );
    }

    let completion = TaskCompletion {
        id: Uuid::new_v4(),
        campaign_id: payload.campaign_id,
        user_id: payload.user_id,
        task_name: payload.task_name,
        details: payload.details,
        created_at: Utc::now(),
    };

    state.task_completions.push(completion.clone());
    respond(
        &state.metrics,
        "/tasks/complete",
        Ok((StatusCode::CREATED, Json(completion))),
    )
}

async fn run_tool(
    State(state): State<SharedState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<ServiceRunRequest>,
) -> Response {
    let (price, metrics, http, config) = {
        let state = state.inner.read().await;
        (
            state.service_price(&service),
            state.metrics.clone(),
            state.http.clone(),
            state.config.clone(),
        )
    };

    let resource_path = format!("/tool/{service}/run");
    let result: ApiResult<Response> = match verify_x402_payment(
        &http,
        &config,
        &service,
        price,
        &resource_path,
        &headers,
    )
    .await
    {
        Ok(payment) => {
            metrics
                .payment_events_total
                .with_label_values(&["user_direct", "settled"])
                .inc();

            Ok(build_paid_tool_response(
                service,
                payload,
                "user_direct".to_string(),
                None,
                payment.tx_hash,
                Some(payment.payment_response_header.as_str()),
            ))
        }
        Err(err) => Err(err),
    };

    respond(&metrics, "/tool/:service/run", result)
}

async fn run_proxy(
    State(state): State<SharedState>,
    Path(service): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<ServiceRunRequest>,
) -> Response {
    let has_header = headers.contains_key(PAYMENT_SIGNATURE_HEADER);

    if has_header {
        let (user_exists, price, metrics, http, config) = {
            let state = state.inner.read().await;
            (
                state.users.contains_key(&payload.user_id),
                state.service_price(&service),
                state.metrics.clone(),
                state.http.clone(),
                state.config.clone(),
            )
        };

        if !user_exists {
            return respond(
                &metrics,
                "/proxy/:service/run",
                Err::<Response, ApiError>(ApiError::not_found(
                    "user profile is required before proxy usage",
                )),
            );
        }

        let resource_path = format!("/proxy/{service}/run");
        let result =
            match verify_x402_payment(&http, &config, &service, price, &resource_path, &headers)
                .await
            {
                Ok(payment) => {
                    metrics
                        .payment_events_total
                        .with_label_values(&["user_direct", "settled"])
                        .inc();

                    Ok(build_paid_tool_response(
                        service,
                        payload,
                        "user_direct".to_string(),
                        None,
                        payment.tx_hash,
                        Some(payment.payment_response_header.as_str()),
                    ))
                }
                Err(err) => Err(err),
            };

        return respond(&metrics, "/proxy/:service/run", result);
    }

    let mut state = state.inner.write().await;

    if !state.users.contains_key(&payload.user_id) {
        return respond(
            &state.metrics,
            "/proxy/:service/run",
            Err::<Response, ApiError>(ApiError::not_found(
                "user profile is required before proxy usage",
            )),
        );
    }

    let price = state.service_price(&service);

    let user = match state.users.get(&payload.user_id) {
        Some(user) => user,
        None => {
            return respond(
                &state.metrics,
                "/proxy/:service/run",
                Err::<Response, ApiError>(ApiError::not_found(
                    "user profile is required before proxy usage",
                )),
            );
        }
    };

    let mut match_without_task: Option<Campaign> = None;
    let mut match_with_task: Option<Campaign> = None;

    let campaigns: Vec<Campaign> = state.campaigns.values().cloned().collect();
    for campaign in campaigns {
        if !campaign.active || campaign.budget_remaining_cents < price {
            continue;
        }

        if !user_matches_campaign(user, &campaign) {
            continue;
        }

        if has_completed_task(
            &state,
            campaign.id,
            payload.user_id,
            &campaign.required_task,
        ) {
            match_with_task = Some(campaign);
            break;
        }

        if match_without_task.is_none() {
            match_without_task = Some(campaign);
        }
    }

    if let Some(campaign) = match_with_task {
        if let Some(persisted) = state.campaigns.get_mut(&campaign.id) {
            persisted.budget_remaining_cents =
                persisted.budget_remaining_cents.saturating_sub(price);
            if persisted.budget_remaining_cents == 0 {
                persisted.active = false;
            }
        }

        let tx_hash = format!("sponsor-{}", Uuid::new_v4());
        state.payments.insert(
            tx_hash.clone(),
            PaymentRecord {
                tx_hash: tx_hash.clone(),
                campaign_id: Some(campaign.id),
                service: service.clone(),
                amount_cents: price,
                payer: campaign.sponsor.clone(),
                source: PaymentSource::Sponsor,
                status: PaymentStatus::Settled,
                created_at: Utc::now(),
            },
        );

        state
            .metrics
            .payment_events_total
            .with_label_values(&["sponsored", "settled"])
            .inc();
        state.metrics.sponsor_spend_cents_total.inc_by(price);

        return respond(
            &state.metrics,
            "/proxy/:service/run",
            Ok(build_paid_tool_response(
                service,
                payload,
                "sponsored".to_string(),
                Some(campaign.sponsor),
                Some(tx_hash),
                None,
            )),
        );
    }

    if let Some(campaign) = match_without_task {
        return respond(
            &state.metrics,
            "/proxy/:service/run",
            Err::<Response, ApiError>(ApiError::precondition(format!(
                "complete sponsor task '{}' for campaign '{}' before sponsored usage",
                campaign.required_task, campaign.name
            ))),
        );
    }

    respond(
        &state.metrics,
        "/proxy/:service/run",
        Err::<Response, ApiError>(payment_required_error(
            &state.config,
            &service,
            price,
            &format!("/proxy/{service}/run"),
            "no eligible sponsor campaign found",
            "either complete a sponsor task or pay with PAYMENT-SIGNATURE",
        )),
    )
}

async fn create_sponsored_api(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<CreateSponsoredApiRequest>,
) -> Response {
    let metrics = {
        let state = state.inner.read().await;
        state.metrics.clone()
    };

    let result: ApiResult<(StatusCode, Json<SponsoredApi>)> = async {
        let (db, http, config) = {
            let state = state.inner.read().await;
            (state.db.clone(), state.http.clone(), state.config.clone())
        };

        let db = db.ok_or_else(|| ApiError::config("Postgres not configured; set DATABASE_URL"))?;

        if payload.name.trim().is_empty() {
            return Err(ApiError::validation("name is required"));
        }
        if payload.sponsor.trim().is_empty() {
            return Err(ApiError::validation("sponsor is required"));
        }
        if payload.budget_cents == 0 {
            return Err(ApiError::validation("budget_cents must be greater than 0"));
        }

        let price_cents = payload.price_cents.unwrap_or(DEFAULT_PRICE_CENTS);
        if price_cents == 0 {
            return Err(ApiError::validation("price_cents must be greater than 0"));
        }

        let upstream_method = normalize_upstream_method(payload.upstream_method)?;
        reqwest::Url::parse(payload.upstream_url.trim())
            .map_err(|_| ApiError::validation("upstream_url must be a valid URL"))?;

        for (header, value) in &payload.upstream_headers {
            HeaderName::from_bytes(header.as_bytes())
                .map_err(|_| ApiError::validation(format!("invalid upstream header: {header}")))?;
            HeaderValue::from_str(value).map_err(|_| {
                ApiError::validation(format!("invalid upstream header value for: {header}"))
            })?;
        }

        if config.sponsored_api_create_price_cents > 0 {
            let resource_path = "/sponsored-apis".to_string();
            verify_x402_payment(
                &http,
                &config,
                SPONSORED_API_CREATE_SERVICE,
                config.sponsored_api_create_price_cents,
                &resource_path,
                &headers,
            )
            .await?;
            metrics
                .payment_events_total
                .with_label_values(&["user_direct", "settled"])
                .inc();
        }

        let api_id = Uuid::new_v4();
        let api = SponsoredApi {
            id: api_id,
            name: payload.name,
            sponsor: payload.sponsor,
            description: payload.description,
            upstream_url: payload.upstream_url,
            upstream_method,
            upstream_headers: payload.upstream_headers,
            price_cents,
            budget_total_cents: payload.budget_cents,
            budget_remaining_cents: payload.budget_cents,
            active: true,
            service_key: sponsored_api_service_key(api_id),
            created_at: Utc::now(),
        };

        let inserted_row = sqlx::query_as::<_, SponsoredApiRow>(
            r#"
            insert into sponsored_apis (
                id, name, sponsor, description, upstream_url, upstream_method,
                upstream_headers, price_cents, budget_total_cents, budget_remaining_cents,
                active, service_key, created_at
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            returning id, name, sponsor, description, upstream_url, upstream_method,
                upstream_headers, price_cents, budget_total_cents, budget_remaining_cents,
                active, service_key, created_at
            "#,
        )
        .bind(api.id)
        .bind(api.name)
        .bind(api.sponsor)
        .bind(api.description)
        .bind(api.upstream_url)
        .bind(api.upstream_method)
        .bind(DbJson(api.upstream_headers))
        .bind(api.price_cents as i64)
        .bind(api.budget_total_cents as i64)
        .bind(api.budget_remaining_cents as i64)
        .bind(api.active)
        .bind(api.service_key)
        .bind(api.created_at)
        .fetch_one(&db)
        .await
        .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        let inserted = SponsoredApi::try_from(inserted_row)
            .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err))?;
        Ok((StatusCode::CREATED, Json(inserted)))
    }
    .await;

    respond(&metrics, "/sponsored-apis", result)
}

async fn list_sponsored_apis(State(state): State<SharedState>) -> Response {
    let metrics = {
        let state = state.inner.read().await;
        state.metrics.clone()
    };

    let result: ApiResult<(StatusCode, Json<Vec<SponsoredApi>>)> = async {
        let db = {
            let state = state.inner.read().await;
            state.db.clone()
        }
        .ok_or_else(|| ApiError::config("Postgres not configured; set DATABASE_URL"))?;

        let api_rows = sqlx::query_as::<_, SponsoredApiRow>(
            r#"
            select id, name, sponsor, description, upstream_url, upstream_method,
                upstream_headers, price_cents, budget_total_cents, budget_remaining_cents,
                active, service_key, created_at
            from sponsored_apis
            order by created_at desc
            "#,
        )
        .fetch_all(&db)
        .await
        .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        let apis: Vec<SponsoredApi> = api_rows
            .into_iter()
            .map(SponsoredApi::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err))?;

        Ok((StatusCode::OK, Json(apis)))
    }
    .await;

    respond(&metrics, "/sponsored-apis", result)
}

async fn get_sponsored_api(State(state): State<SharedState>, Path(api_id): Path<Uuid>) -> Response {
    let metrics = {
        let state = state.inner.read().await;
        state.metrics.clone()
    };

    let result: ApiResult<(StatusCode, Json<SponsoredApi>)> = async {
        let db = {
            let state = state.inner.read().await;
            state.db.clone()
        }
        .ok_or_else(|| ApiError::config("Postgres not configured; set DATABASE_URL"))?;

        let api = sqlx::query_as::<_, SponsoredApiRow>(
            r#"
            select id, name, sponsor, description, upstream_url, upstream_method,
                upstream_headers, price_cents, budget_total_cents, budget_remaining_cents,
                active, service_key, created_at
            from sponsored_apis
            where id = $1
            "#,
        )
        .bind(api_id)
        .fetch_optional(&db)
        .await
        .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| ApiError::not_found("sponsored api not found"))
        .and_then(|row| {
            SponsoredApi::try_from(row)
                .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err))
        })?;

        Ok((StatusCode::OK, Json(api)))
    }
    .await;

    respond(&metrics, "/sponsored-apis/:api_id", result)
}

async fn run_sponsored_api(
    State(state): State<SharedState>,
    Path(api_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<SponsoredApiRunRequest>,
) -> Response {
    let metrics = {
        let state = state.inner.read().await;
        state.metrics.clone()
    };

    let result: ApiResult<Response> = async {
        let (db, http, config) = {
            let state = state.inner.read().await;
            (state.db.clone(), state.http.clone(), state.config.clone())
        };

        let db = db.ok_or_else(|| ApiError::config("Postgres not configured; set DATABASE_URL"))?;

        let api = sqlx::query_as::<_, SponsoredApiRow>(
            r#"
            select id, name, sponsor, description, upstream_url, upstream_method,
                upstream_headers, price_cents, budget_total_cents, budget_remaining_cents,
                active, service_key, created_at
            from sponsored_apis
            where id = $1
            "#,
        )
        .bind(api_id)
        .fetch_optional(&db)
        .await
        .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| ApiError::not_found("sponsored api not found"))
        .and_then(|row| {
            SponsoredApi::try_from(row)
                .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err))
        })?;

        let price = api.price_cents;
        let service_key = api.service_key.clone();
        let mut payment_mode = "sponsored".to_string();
        let mut sponsored_by = None;
        let mut tx_hash: Option<String> = None;
        let mut payment_response_header: Option<String> = None;

        if headers.contains_key(PAYMENT_SIGNATURE_HEADER) {
            let resource_path = format!("/sponsored-apis/{api_id}/run");
            let payment = verify_x402_payment(
                &http,
                &config,
                &service_key,
                price,
                &resource_path,
                &headers,
            )
            .await?;
            metrics
                .payment_events_total
                .with_label_values(&["user_direct", "settled"])
                .inc();
            payment_mode = "user_direct".to_string();
            tx_hash = payment.tx_hash;
            payment_response_header = Some(payment.payment_response_header);
        } else if api.active && api.budget_remaining_cents >= price {
            let new_remaining = api.budget_remaining_cents.saturating_sub(price);
            let still_active = new_remaining >= price && new_remaining > 0;

            sqlx::query(
                r#"
                update sponsored_apis
                set budget_remaining_cents = $1, active = $2
                where id = $3
                "#,
            )
            .bind(new_remaining as i64)
            .bind(still_active)
            .bind(api.id)
            .execute(&db)
            .await
            .map_err(|err| {
                ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            })?;

            metrics
                .payment_events_total
                .with_label_values(&["sponsored", "settled"])
                .inc();
            metrics.sponsor_spend_cents_total.inc_by(price);
            sponsored_by = Some(api.sponsor.clone());
        } else {
            return Err(payment_required_error(
                &config,
                &service_key,
                price,
                &format!("/sponsored-apis/{api_id}/run"),
                "sponsored budget exhausted",
                "pay with PAYMENT-SIGNATURE and retry",
            ));
        }

        let SponsoredApiRunRequest { caller, input } = payload;
        let (upstream_status, upstream_body) =
            call_upstream(&http, &api, input, config.sponsored_api_timeout_secs).await?;

        let call_log = SponsoredApiCall {
            id: Uuid::new_v4(),
            sponsored_api_id: api.id,
            payment_mode: payment_mode.clone(),
            amount_cents: price,
            tx_hash: tx_hash.clone(),
            caller,
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            insert into sponsored_api_calls (
                id, sponsored_api_id, payment_mode, amount_cents, tx_hash, caller, created_at
            ) values ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(call_log.id)
        .bind(call_log.sponsored_api_id)
        .bind(call_log.payment_mode)
        .bind(call_log.amount_cents as i64)
        .bind(call_log.tx_hash)
        .bind(call_log.caller)
        .bind(call_log.created_at)
        .execute(&db)
        .await
        .map_err(|err| ApiError::database(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        let response_payload = SponsoredApiRunResponse {
            api_id: api.id,
            payment_mode,
            sponsored_by,
            tx_hash,
            upstream_status,
            upstream_body,
        };

        let mut response = (StatusCode::OK, Json(response_payload)).into_response();
        response.headers_mut().insert(
            HeaderName::from_static(X402_VERSION_HEADER),
            HeaderValue::from_static("2"),
        );
        if let Some(settlement_header) = payment_response_header {
            if let Ok(header_value) = HeaderValue::from_str(&settlement_header) {
                response.headers_mut().insert(
                    HeaderName::from_static(PAYMENT_RESPONSE_HEADER),
                    header_value,
                );
            }
        }

        Ok(response)
    }
    .await;

    respond(&metrics, "/sponsored-apis/:api_id/run", result)
}

async fn ingest_x402scan_settlement(
    State(state): State<SharedState>,
    Json(payload): Json<X402ScanSettlementRequest>,
) -> Response {
    let mut state = state.inner.write().await;

    state.payments.insert(
        payload.tx_hash.clone(),
        PaymentRecord {
            tx_hash: payload.tx_hash,
            campaign_id: payload.campaign_id,
            service: payload.service,
            amount_cents: payload.amount_cents,
            payer: payload.payer,
            source: payload.source.clone(),
            status: payload.status.clone(),
            created_at: Utc::now(),
        },
    );

    let mode = match payload.source {
        PaymentSource::User => "user_direct",
        PaymentSource::Sponsor => "sponsored",
    };
    let status = match payload.status {
        PaymentStatus::Settled => "settled",
        PaymentStatus::Failed => "failed",
    };

    state
        .metrics
        .payment_events_total
        .with_label_values(&[mode, status])
        .inc();

    respond(
        &state.metrics,
        "/webhooks/x402scan/settlement",
        Ok((
            StatusCode::ACCEPTED,
            Json(MessageResponse {
                message: "settlement ingested".to_string(),
            }),
        )),
    )
}

async fn sponsor_dashboard(
    State(state): State<SharedState>,
    Path(campaign_id): Path<Uuid>,
) -> Response {
    let state = state.inner.read().await;

    let Some(campaign) = state.campaigns.get(&campaign_id).cloned() else {
        return respond(
            &state.metrics,
            "/dashboard/sponsor/:campaign_id",
            Err::<Response, ApiError>(ApiError::not_found("campaign not found")),
        );
    };

    let tasks_completed = state
        .task_completions
        .iter()
        .filter(|task| task.campaign_id == campaign_id)
        .count();

    let sponsored_payments: Vec<&PaymentRecord> = state
        .payments
        .values()
        .filter(|record| {
            record.campaign_id == Some(campaign_id)
                && record.source == PaymentSource::Sponsor
                && record.status == PaymentStatus::Settled
        })
        .collect();

    let sponsored_calls = sponsored_payments.len();
    let spend_cents: u64 = sponsored_payments
        .iter()
        .map(|record| record.amount_cents)
        .sum();

    let response = SponsorDashboard {
        remaining_budget_cents: campaign.budget_remaining_cents,
        campaign,
        tasks_completed,
        sponsored_calls,
        spend_cents,
    };

    respond(
        &state.metrics,
        "/dashboard/sponsor/:campaign_id",
        Ok((StatusCode::OK, Json(response))),
    )
}

async fn record_creator_metric_event(
    State(state): State<SharedState>,
    Json(payload): Json<CreatorMetricEventRequest>,
) -> Response {
    let mut state = state.inner.write().await;

    let event = CreatorEvent {
        id: Uuid::new_v4(),
        skill_name: payload.skill_name,
        platform: payload.platform,
        event_type: payload.event_type,
        duration_ms: payload.duration_ms,
        success: payload.success,
        created_at: Utc::now(),
    };

    state
        .metrics
        .creator_events_total
        .with_label_values(&[&event.skill_name, &event.platform, &event.event_type])
        .inc();

    state.creator_events.push(event.clone());

    respond(
        &state.metrics,
        "/creator/metrics/event",
        Ok((StatusCode::CREATED, Json(event))),
    )
}

async fn creator_metrics(State(state): State<SharedState>) -> Response {
    let state = state.inner.read().await;

    let total_events = state.creator_events.len();
    let success_events = state
        .creator_events
        .iter()
        .filter(|event| event.success)
        .count();
    let success_rate = if total_events == 0 {
        0.0
    } else {
        success_events as f64 / total_events as f64
    };

    let mut per_skill_map: HashMap<String, Vec<&CreatorEvent>> = HashMap::new();
    for event in &state.creator_events {
        per_skill_map
            .entry(event.skill_name.clone())
            .or_default()
            .push(event);
    }

    let mut per_skill: Vec<SkillMetrics> = per_skill_map
        .into_iter()
        .map(|(skill_name, events)| {
            let total = events.len();
            let success = events.iter().filter(|event| event.success).count();

            let duration_values: Vec<u64> = events
                .iter()
                .filter_map(|event| event.duration_ms)
                .collect();

            let avg_duration_ms = if duration_values.is_empty() {
                None
            } else {
                let sum: u64 = duration_values.iter().sum();
                Some(sum as f64 / duration_values.len() as f64)
            };

            let last_seen_at = events
                .iter()
                .map(|event| event.created_at)
                .max()
                .unwrap_or_else(Utc::now);

            SkillMetrics {
                skill_name,
                total_events: total,
                success_events: success,
                avg_duration_ms,
                last_seen_at,
            }
        })
        .collect();

    per_skill.sort_by(|left, right| {
        right
            .total_events
            .cmp(&left.total_events)
            .then_with(|| right.last_seen_at.cmp(&left.last_seen_at))
    });

    respond(
        &state.metrics,
        "/creator/metrics",
        Ok((
            StatusCode::OK,
            Json(CreatorMetricSummary {
                total_events,
                success_events,
                success_rate,
                per_skill,
            }),
        )),
    )
}

async fn prometheus_metrics(State(state): State<SharedState>) -> Response {
    let state = state.inner.read().await;
    let metric_families = state.metrics.registry.gather();
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    let status = match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let content_type = encoder.format_type().to_string();

    mark_request(&state.metrics, "/metrics", status);

    (
        status,
        [("content-type", content_type)],
        String::from_utf8_lossy(&buffer).to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod test;
