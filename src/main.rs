mod app_types;

use alloy::{
    network::{EthereumSigner, TransactionBuilder},
    primitives::hex,
    rlp::Encodable,
    rpc::types::eth::request::TransactionRequest,
    signers::wallet::YubiWallet,
};
use anyhow::{anyhow, Result as AnyhowResult};
use app_types::{AppError, AppJson, AppResult};
use axum::{debug_handler, extract::State, routing::post, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{str::FromStr, sync::Arc, time::Duration};
use structopt::StructOpt;
use tokio::{net::TcpListener, signal};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use yubihsm::{device::SerialNumber, Connector, Credentials, UsbConfig};

const USB_TIMEOUT_MS: u64 = 20_000;
const HTTP_TIMEOUT_SECS: u64 = 10;

#[derive(StructOpt, Debug)]
#[structopt(name = "yubihsm-signer-proxy")]
struct Opt {
    /// RPC URL
    #[structopt(short, long, env = "RPC_URL")]
    rpc_url: String,

    /// YubiHSM device serial ID
    #[structopt(short, long, env = "YUBIHSM_DEVICE_SERIAL_ID")]
    device_serial_id: String,

    /// YubiHSM auth key ID
    #[structopt(short, long, env = "YUBIHSM_AUTH_KEY_ID")]
    auth_key_id: u16,

    /// YubiHSM auth key password
    #[structopt(short, long, env = "YUBIHSM_PASSWORD", hide_env_values = true)]
    password: String,

    /// YubiHSM signing key ID
    #[structopt(short, long, env = "YUBIHSM_SIGNING_KEY_ID")]
    signing_key_id: u16,
}

#[derive(Clone, Debug)]
struct AppState {
    rpc_url: String,
    signer: EthereumSigner,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonRpcRequest<T> {
    jsonrpc: String,
    method: String,
    id: u64,
    params: T,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonRpcReply<T> {
    id: u64,
    jsonrpc: String,
    #[serde(flatten)]
    result: JsonRpcResult<T>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum JsonRpcResult<T> {
    Result(T),
    Error { code: i64, message: String },
}

#[debug_handler]
async fn handle_request(
    State(state): State<Arc<AppState>>,
    AppJson(payload): AppJson<JsonRpcRequest<Vec<Value>>>,
) -> AppResult<JsonRpcReply<Value>> {
    let method = payload.method.as_str();
    let eth_signer = state.signer.to_owned();

    let result = match method {
        "eth_signTransaction" => handle_eth_sign_transaction(payload, eth_signer).await,
        _ => handle_other_methods(payload, &state.rpc_url).await,
    };

    result.map(|reply| AppJson(reply)).map_err(AppError)
}

async fn handle_eth_sign_transaction(
    payload: JsonRpcRequest<Vec<Value>>,
    signer: EthereumSigner,
) -> AnyhowResult<JsonRpcReply<Value>> {
    if payload.params.is_empty() {
        return Err(anyhow!("params is empty"));
    }

    let tx_object = payload.params[0].to_owned();
    let tx_request = serde_json::from_value::<TransactionRequest>(tx_object)?;
    let tx_envelope = tx_request.build(&signer).await?;
    let mut encoded_tx = vec![];
    tx_envelope.encode(&mut encoded_tx);
    let rlp_hex = hex::encode_prefixed(encoded_tx);

    Ok(JsonRpcReply {
        id: payload.id,
        jsonrpc: payload.jsonrpc,
        result: JsonRpcResult::Result(rlp_hex.into()),
    })
}

async fn handle_other_methods(
    payload: JsonRpcRequest<Vec<Value>>,
    rpc_url: &str,
) -> AnyhowResult<JsonRpcReply<Value>> {
    let client = Client::new();
    let response = client.post(rpc_url).json(&payload).send().await?;
    let json = response.json().await?;

    Ok(json)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "yubihsm_signer_proxy=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let opt = Opt::from_args();
    let serial = SerialNumber::from_str(&opt.device_serial_id).unwrap();
    let connector = Connector::usb(&UsbConfig {
        serial: Some(serial),
        timeout_ms: USB_TIMEOUT_MS,
    });
    let credentials = Credentials::from_password(opt.auth_key_id, opt.password.as_bytes());
    let yubi_signer = YubiWallet::connect(connector, credentials, opt.signing_key_id);
    let signer = EthereumSigner::from(yubi_signer);

    let shared_state = Arc::new(AppState {
        rpc_url: opt.rpc_url,
        signer,
    });

    let app = Router::new()
        .route("/", post(handle_request))
        .with_state(shared_state)
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::new(Duration::from_secs(HTTP_TIMEOUT_SECS)),
        ));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
