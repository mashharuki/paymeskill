use axum::http::StatusCode;
use serde_json::Value;

use crate::error::{ApiError, ApiResult};

pub async fn fetch_tx_block_number(
    http: &reqwest::Client,
    rpc_url: &str,
    tx_hash: &str,
) -> ApiResult<u64> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getTransactionReceipt",
        "params": [tx_hash]
    });
    let response = rpc_call(http, rpc_url, payload).await?;
    let Some(receipt) = response.get("result").and_then(|value| value.as_object()) else {
        return Err(ApiError::validation(
            "transaction receipt not found or not yet indexed on testnet",
        ));
    };
    let status_hex = receipt
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("0x0");
    let status = parse_hex_u64(status_hex)
        .ok_or_else(|| ApiError::validation("invalid status in transaction receipt"))?;
    if status != 1 {
        return Err(ApiError::validation(
            "transaction status is not successful on testnet",
        ));
    }
    let block_hex = receipt
        .get("blockNumber")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ApiError::validation("missing blockNumber in transaction receipt"))?;
    parse_hex_u64(block_hex)
        .ok_or_else(|| ApiError::validation("invalid blockNumber in transaction receipt"))
}

pub async fn fetch_latest_block_number(http: &reqwest::Client, rpc_url: &str) -> ApiResult<u64> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "eth_blockNumber",
        "params": []
    });
    let response = rpc_call(http, rpc_url, payload).await?;
    let block_hex = response
        .get("result")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ApiError::validation("missing result for eth_blockNumber"))?;
    parse_hex_u64(block_hex).ok_or_else(|| ApiError::validation("invalid eth_blockNumber result"))
}

async fn rpc_call(http: &reqwest::Client, rpc_url: &str, payload: Value) -> ApiResult<Value> {
    let response = http
        .post(rpc_url)
        .json(&payload)
        .send()
        .await
        .map_err(|err| ApiError::upstream(StatusCode::BAD_GATEWAY, err.to_string()))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| ApiError::upstream(StatusCode::BAD_GATEWAY, err.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::upstream(
            StatusCode::BAD_GATEWAY,
            format!("rpc error status={status} body={body}"),
        ));
    }

    let value: Value = serde_json::from_str(&body)
        .map_err(|err| ApiError::upstream(StatusCode::BAD_GATEWAY, err.to_string()))?;

    if value.get("error").is_some() {
        return Err(ApiError::upstream(
            StatusCode::BAD_GATEWAY,
            format!("rpc returned error: {}", value["error"]),
        ));
    }

    Ok(value)
}

fn parse_hex_u64(value: &str) -> Option<u64> {
    let trimmed = value.trim_start_matches("0x");
    u64::from_str_radix(trimmed, 16).ok()
}
