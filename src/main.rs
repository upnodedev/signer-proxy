mod app_types;

use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::hex,
    rlp::Encodable,
    rpc::types::eth::request::TransactionRequest,
    signers::local::YubiSigner,
};
use anyhow::{anyhow, Result as AnyhowResult};
use app_types::{AppError, AppJson, AppResult};
use axum::{
    debug_handler,
    extract::{Path, State},
    routing::post,
    Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
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
}

#[derive(Clone)]
struct AppState {
    rpc_url: String,
    connector: Connector,
    credentials: Credentials,
    signers: Arc<Mutex<HashMap<u16, EthereumWallet>>>,
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
    Path(key_id): Path<u16>,
    State(state): State<Arc<AppState>>,
    AppJson(payload): AppJson<JsonRpcRequest<Vec<Value>>>,
) -> AppResult<JsonRpcReply<Value>> {
    let method = payload.method.as_str();

    let eth_signer = match get_signer(state.clone(), key_id).await {
        Ok(signer) => signer,
        Err(e) => return Err(AppError(e)),
    };

    let result = match method {
        "eth_signTransaction" => handle_eth_sign_transaction(payload, eth_signer).await,
        _ => handle_other_methods(payload, &state.rpc_url).await,
    };

    result.map(AppJson).map_err(AppError)
}

async fn get_signer(state: Arc<AppState>, key_id: u16) -> AnyhowResult<EthereumWallet> {
    let mut signers = state.signers.lock().unwrap();

    if let Some(signer) = signers.get(&key_id) {
        return Ok(signer.clone());
    } else {
        let yubi_signer =
            YubiSigner::connect(state.connector.clone(), state.credentials.clone(), key_id)?;
        let eth_signer = EthereumWallet::from(yubi_signer);

        signers.insert(key_id, eth_signer.clone());

        Ok(eth_signer)
    }
}

async fn handle_eth_sign_transaction(
    payload: JsonRpcRequest<Vec<Value>>,
    signer: EthereumWallet,
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
    let shared_state = Arc::new(AppState {
        rpc_url: opt.rpc_url,
        connector,
        credentials,
        signers: Arc::new(Mutex::new(HashMap::new())),
    });

    let app = Router::new()
        .route("/key/:key_id", post(handle_request))
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
