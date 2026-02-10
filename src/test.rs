use super::*;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, header};
use tower::ServiceExt;

fn required_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        panic!(
            "missing required env var {key}; set TESTNET_RPC_URL, TESTNET_CONFIRMED_TX_HASH, and TESTNET_FAILED_TX_HASH to run testnet tests"
        )
    })
}

fn test_app() -> (Router, SharedState) {
    let state = SharedState {
        inner: Arc::new(RwLock::new(AppState::new())),
    };
    (build_app(state.clone()), state)
}

async fn post_json(app: &Router, uri: &str, body: serde_json::Value) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request should build"),
        )
        .await
        .expect("router should handle request")
}

async fn read_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice::<serde_json::Value>(&bytes).expect("response should be valid json")
}

async fn configure_testnet_rpc_from_env(state: &SharedState) {
    let rpc = required_env("TESTNET_RPC_URL");
    let mut locked = state.inner.write().await;
    locked.config.testnet_rpc_url = Some(rpc);
    locked.config.testnet_min_confirmations = 1;
}

async fn create_testnet_payment_signature(
    app: &Router,
    tx_hash: &str,
    service: &str,
) -> (String, axum::response::Response) {
    let payment_response = post_json(
        app,
        "/payments/testnet/direct",
        serde_json::json!({
            "payer": "dev@testnet.example",
            "service": service,
            "tx_hash": tx_hash
        }),
    )
    .await;
    assert_eq!(payment_response.status(), StatusCode::CREATED);

    let payment_json = read_json(payment_response).await;
    let signature = payment_json["payment_signature"]
        .as_str()
        .expect("signature should exist")
        .to_string();

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/tool/{service}/run"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(PAYMENT_SIGNATURE_HEADER, signature.clone())
                .body(Body::from(
                    serde_json::json!({
                        "user_id": Uuid::new_v4(),
                        "input": "testnet paid run"
                    })
                    .to_string(),
                ))
                .expect("request should build"),
        )
        .await
        .expect("router should handle request");

    (signature, run_response)
}

#[tokio::test]
async fn tool_requires_x402_payment_proof() {
    let (app, _state) = test_app();
    let response = post_json(
        &app,
        "/tool/design/run",
        serde_json::json!({
            "user_id": Uuid::new_v4(),
            "input": "test payload"
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    assert_eq!(
        response
            .headers()
            .get(X402_VERSION_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );

    let json = read_json(response).await;
    assert_eq!(json["accepted_header"], "payment-signature");
    assert_eq!(json["service"], "design");
}

#[tokio::test]
async fn testnet_payment_signature_unlocks_tool() {
    let (app, state) = test_app();
    configure_testnet_rpc_from_env(&state).await;
    let confirmed_tx = required_env("TESTNET_CONFIRMED_TX_HASH");

    let (_sig, run_response) =
        create_testnet_payment_signature(&app, &confirmed_tx, "design").await;

    assert_eq!(run_response.status(), StatusCode::OK);
    assert!(run_response.headers().contains_key(PAYMENT_RESPONSE_HEADER));
    let body = read_json(run_response).await;
    assert_eq!(body["payment_mode"], "user_direct");
    assert_eq!(body["service"], "design");
}

#[tokio::test]
async fn testnet_payment_requires_confirmations() {
    let (app, state) = test_app();
    configure_testnet_rpc_from_env(&state).await;
    let confirmed_tx = required_env("TESTNET_CONFIRMED_TX_HASH");

    let response = post_json(
        &app,
        "/payments/testnet/direct",
        serde_json::json!({
            "payer": "dev@testnet.example",
            "service": "design",
            "tx_hash": confirmed_tx,
            "min_confirmations": u64::MAX
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = read_json(response).await;
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("insufficient confirmations")
    );
}

#[tokio::test]
async fn testnet_payment_proof_service_mismatch_is_rejected() {
    let (app, state) = test_app();
    configure_testnet_rpc_from_env(&state).await;
    let confirmed_tx = required_env("TESTNET_CONFIRMED_TX_HASH");

    let (signature, _ok) = create_testnet_payment_signature(&app, &confirmed_tx, "design").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/tool/storage/run")
                .header(header::CONTENT_TYPE, "application/json")
                .header(PAYMENT_SIGNATURE_HEADER, signature)
                .body(Body::from(
                    serde_json::json!({
                        "user_id": Uuid::new_v4(),
                        "input": "store object"
                    })
                    .to_string(),
                ))
                .expect("request should build"),
        )
        .await
        .expect("router should handle request");

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    let json = read_json(response).await;
    assert!(
        json["message"]
            .as_str()
            .unwrap_or_default()
            .contains("service mismatch")
    );
}

#[tokio::test]
async fn testnet_failed_transaction_is_rejected() {
    let (app, state) = test_app();
    configure_testnet_rpc_from_env(&state).await;
    let failed_tx = required_env("TESTNET_FAILED_TX_HASH");

    let response = post_json(
        &app,
        "/payments/testnet/direct",
        serde_json::json!({
            "payer": "dev@testnet.example",
            "service": "design",
            "tx_hash": failed_tx
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = read_json(response).await;
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("not successful")
    );
}
