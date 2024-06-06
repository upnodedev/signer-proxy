use alloy::primitives::hex;
use alloy::rlp::Encodable;
use alloy::{
    network::{EthereumSigner, TransactionBuilder},
    rpc::types::eth::request::TransactionRequest,
    signers::wallet::YubiWallet,
};
use axum::{debug_handler, extract::State, http::StatusCode, routing::post, Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{str::FromStr, sync::Arc, time::Duration};
use structopt::StructOpt;
use tokio::{net::TcpListener, signal};
use tower_http::timeout::TimeoutLayer;
use tracing::{debug, error, info, span, Level};
use yubihsm::{device::SerialNumber, Connector, Credentials, UsbConfig};

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
    Json(payload): Json<JsonRpcRequest<Vec<Value>>>,
) -> (StatusCode, Json<JsonRpcReply<Value>>) {
    let span = span!(Level::INFO, "handle_request", method = %payload.method);
    let _enter = span.enter();

    let jsonrpc = payload.jsonrpc.clone();
    let method = payload.method.as_str();
    let id = payload.id;
    let tx_object = payload.params[0].clone();

    let result = match method {
        "eth_signTransaction" => {
            info!("Handling eth_signTransaction: {:#?}", tx_object);
            handle_eth_sign_transaction(tx_object, state.signer.clone())
                .await
                .map(|value| JsonRpcReply {
                    id,
                    jsonrpc: jsonrpc.clone(),
                    result: JsonRpcResult::Result(value),
                })
        }
        _ => {
            debug!("Proxying request to RPC URL: {:#?}", payload);
            proxy_request_to_rpc(payload, &state.rpc_url).await
        }
    };

    result.map_or_else(
        |(status, message)| {
            error!(status = %status.as_u16(), "Error handling request: {}", message);
            (
                status,
                Json(JsonRpcReply {
                    id,
                    jsonrpc,
                    result: JsonRpcResult::Error {
                        code: status.as_u16() as i64,
                        message,
                    },
                }),
            )
        },
        |reply| {
            info!("Successfully processed request: {:#?}", reply);
            (StatusCode::OK, Json(reply))
        },
    )
}

async fn handle_eth_sign_transaction(
    tx_object: Value,
    signer: EthereumSigner,
) -> Result<Value, (StatusCode, String)> {
    let span = span!(Level::INFO, "handle_eth_sign_transaction");
    let _enter = span.enter();

    sign_transaction(signer, tx_object)
        .await
        .map(Value::String)
        .map_err(|e| {
            error!("Failed to sign transaction: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })
}

async fn sign_transaction(signer: EthereumSigner, tx_object: Value) -> Result<String, String> {
    let span = span!(Level::INFO, "sign_transaction");
    let _enter = span.enter();

    let tx_request = serde_json::from_value::<TransactionRequest>(tx_object)
        .map_err(|e| format!("Failed to parse transaction request: {}", e))?;
    let tx_envelope = tx_request
        .build(&signer)
        .await
        .map_err(|e| format!("Failed to build transaction: {}", e))?;
    let mut encoded_tx = vec![];
    tx_envelope.encode(&mut encoded_tx);
    let rlp_hex = hex::encode_prefixed(&encoded_tx);

    Ok(rlp_hex)
}

async fn proxy_request_to_rpc(
    payload: JsonRpcRequest<Vec<Value>>,
    rpc_url: &str,
) -> Result<JsonRpcReply<Value>, (StatusCode, String)> {
    let span = span!(Level::DEBUG, "proxy_request_to_rpc", rpc_url = %rpc_url);
    let _enter = span.enter();

    let client = Client::new();
    let response = client
        .post(rpc_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    response
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let opt = Opt::from_args();

    let serial =
        SerialNumber::from_str(&opt.device_serial_id).expect("Failed to parse serial number");
    let connector = Connector::usb(&UsbConfig {
        serial: Some(serial),
        timeout_ms: 20_000,
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
        .layer(TimeoutLayer::new(Duration::from_secs(10)));

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
