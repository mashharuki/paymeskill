use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::types::{AppConfig, X402PaymentRequirement, X402SettleResponse, X402VerifyResponse};

#[derive(Debug, Clone)]
pub struct VerifiedX402Payment {
    pub tx_hash: Option<String>,
    pub payment_response_header: String,
}

pub async fn verify_and_settle_x402_payment(
    http: &reqwest::Client,
    config: &AppConfig,
    payment_signature: &str,
    requirement: &X402PaymentRequirement,
) -> ApiResult<VerifiedX402Payment> {
    let payment_payload = decode_payment_signature(payment_signature)?;

    let verify_response: X402VerifyResponse = post_to_facilitator(
        http,
        config,
        &config.x402_verify_path,
        &payment_payload,
        requirement,
    )
    .await?;

    if !verify_response.is_valid {
        return Err(ApiError::validation(
            verify_response
                .invalid_reason
                .unwrap_or_else(|| "facilitator rejected payment signature".to_string()),
        ));
    }

    let settle_response: X402SettleResponse = post_to_facilitator(
        http,
        config,
        &config.x402_settle_path,
        &payment_payload,
        requirement,
    )
    .await?;

    if !settle_response.success {
        return Err(ApiError::validation(
            settle_response
                .error_reason
                .unwrap_or_else(|| "facilitator could not settle payment".to_string()),
        ));
    }

    let payment_response_header =
        STANDARD
            .encode(serde_json::to_vec(&settle_response).map_err(|err| {
                ApiError::internal(format!("failed to encode settlement: {err}"))
            })?);

    Ok(VerifiedX402Payment {
        tx_hash: settle_response.transaction,
        payment_response_header,
    })
}

fn decode_payment_signature(payment_signature: &str) -> ApiResult<Value> {
    let decoded = STANDARD
        .decode(payment_signature)
        .map_err(|err| ApiError::validation(format!("PAYMENT-SIGNATURE must be base64: {err}")))?;

    serde_json::from_slice::<Value>(&decoded).map_err(|err| {
        ApiError::validation(format!(
            "PAYMENT-SIGNATURE payload must decode to JSON: {err}"
        ))
    })
}

async fn post_to_facilitator<T: DeserializeOwned>(
    http: &reqwest::Client,
    config: &AppConfig,
    path: &str,
    payment_payload: &Value,
    requirement: &X402PaymentRequirement,
) -> ApiResult<T> {
    let body = serde_json::json!({
        "x402Version": 2,
        "paymentPayload": payment_payload,
        "paymentRequirements": requirement
    });

    let url = join_url(&config.x402_facilitator_url, path);
    let mut request = http.post(&url).json(&body);
    if let Some(token) = config.x402_facilitator_bearer_token.as_deref() {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .map_err(|err| ApiError::upstream(StatusCode::BAD_GATEWAY, err.to_string()))?;

    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|err| ApiError::upstream(StatusCode::BAD_GATEWAY, err.to_string()))?;

    if !status.is_success() {
        let message = format!("facilitator call {url} failed with status={status}: {raw}");
        if status.is_client_error() {
            return Err(ApiError::validation(message));
        }
        return Err(ApiError::upstream(StatusCode::BAD_GATEWAY, message));
    }

    serde_json::from_str::<T>(&raw).map_err(|err| {
        ApiError::upstream(
            StatusCode::BAD_GATEWAY,
            format!("invalid facilitator JSON response: {err}; raw={raw}"),
        )
    })
}

fn join_url(base: &str, path: &str) -> String {
    let trimmed_base = base.trim_end_matches('/');
    if path.starts_with('/') {
        format!("{trimmed_base}{path}")
    } else {
        format!("{trimmed_base}/{path}")
    }
}
