mod app_types;

use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{hex, Address},
    rlp::Encodable,
    rpc::types::eth::request::TransactionRequest,
    signers::local::{
        yubihsm::{
            asymmetric::Algorithm::EcK256, device::SerialNumber, Capability, Client, Connector,
            Credentials, Domain, HttpConfig, UsbConfig,
        },
        YubiSigner,
    },
};
use anyhow::{anyhow, Result as AnyhowResult};
use app_types::{AppError, AppJson, AppResult};
use axum::{
    debug_handler,
    extract::{Path, State},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};
use structopt::StructOpt;
use strum::{EnumString, VariantNames};
use tokio::{net::TcpListener, signal, sync::Mutex};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_USB_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_HTTP_TIMEOUT_MS: u64 = 5000;
const API_TIMEOUT_SECS: u64 = 10;

#[derive(EnumString, VariantNames, Debug)]
#[strum(serialize_all = "kebab_case")]
enum Mode {
    Usb,
    Http,
}

#[derive(StructOpt)]
struct Opt {
    /// Connection mode (usb or http)
    #[structopt(short, long, possible_values = Mode::VARIANTS, case_insensitive = true, default_value = "usb")]
    mode: Mode,

    /// YubiHSM device serial ID (for USB mode)
    #[structopt(
        short,
        long = "device-serial",
        env = "YUBIHSM_DEVICE_SERIAL_ID",
        required_if("mode", "usb")
    )]
    device_serial_id: Option<String>,

    /// YubiHSM HTTP address (for HTTP mode)
    #[structopt(
        long = "addr",
        env = "YUBIHSM_HTTP_ADDRESS",
        required_if("mode", "http")
    )]
    http_address: Option<String>,

    /// YubiHSM HTTP port (for HTTP mode)
    #[structopt(long = "port", env = "YUBIHSM_HTTP_PORT", required_if("mode", "http"))]
    http_port: Option<u16>,

    /// YubiHSM auth key ID
    #[structopt(short, long = "auth-key", env = "YUBIHSM_AUTH_KEY_ID")]
    auth_key_id: u16,

    /// YubiHSM auth key password
    #[structopt(short, long = "pass", env = "YUBIHSM_PASSWORD", hide_env_values = true)]
    password: String,

    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    Serve,
    GenerateKey {
        #[structopt(short, long, default_value)]
        label: String,
        #[structopt(short, long)]
        exportable: bool,
    },
}

#[derive(Clone)]
struct AppState {
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
    let eth_signer = get_signer(state.clone(), key_id).await?;

    let result = match method {
        "eth_signTransaction" => handle_eth_sign_transaction(payload, eth_signer).await,
        _ => Err(anyhow!(
            "method not supported (eth_signTransaction only): {}",
            method
        )),
    };

    result.map(AppJson).map_err(AppError)
}

async fn get_signer(state: Arc<AppState>, key_id: u16) -> AnyhowResult<EthereumWallet> {
    let mut signers = state.signers.lock().await;

    if let Some(signer) = signers.get(&key_id) {
        return Ok(signer.clone());
    }

    let yubi_signer =
        YubiSigner::connect(state.connector.clone(), state.credentials.clone(), key_id)?;
    let eth_signer = EthereumWallet::from(yubi_signer);

    signers.insert(key_id, eth_signer.clone());

    Ok(eth_signer)
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

fn generate_new_key(
    connector: Connector,
    credentials: Credentials,
    label: String,
    exportable: bool,
) -> AnyhowResult<(u16, Address)> {
    let client = Client::open(connector.clone(), credentials.clone(), true)?;
    let capabilities = if exportable {
        Capability::SIGN_ECDSA | Capability::EXPORTABLE_UNDER_WRAP
    } else {
        Capability::SIGN_ECDSA
    };
    let id = client.generate_asymmetric_key(
        0,
        label.as_str().into(),
        Domain::all(),
        capabilities,
        EcK256,
    )?;
    let signer = YubiSigner::connect(connector, credentials, id)?;

    Ok((id, signer.address()))
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

    match opt.cmd {
        Command::Serve => {
            let connector = match opt.mode {
                Mode::Usb => {
                    let serial = SerialNumber::from_str(
                        &opt.device_serial_id
                            .expect("USB mode requires a device serial ID"),
                    )
                    .unwrap();
                    Connector::usb(&UsbConfig {
                        serial: Some(serial),
                        timeout_ms: DEFAULT_USB_TIMEOUT_MS,
                    })
                }
                Mode::Http => {
                    let addr = opt.http_address.expect("HTTP mode requires an address");
                    let port = opt.http_port.expect("HTTP mode requires a port");
                    Connector::http(&HttpConfig {
                        addr,
                        port,
                        timeout_ms: DEFAULT_HTTP_TIMEOUT_MS,
                    })
                }
            };

            let credentials = Credentials::from_password(opt.auth_key_id, opt.password.as_bytes());
            let shared_state = Arc::new(AppState {
                connector,
                credentials,
                signers: Arc::new(Mutex::new(HashMap::new())),
            });

            let app = Router::new()
                .route("/key/:key_id", post(handle_request))
                .with_state(shared_state)
                .layer((
                    TraceLayer::new_for_http(),
                    TimeoutLayer::new(Duration::from_secs(API_TIMEOUT_SECS)),
                ));

            let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
            debug!("listening on {}", listener.local_addr().unwrap());
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap();
        }
        Command::GenerateKey { label, exportable } => {
            println!("Generating new key...");
            println!("Label: {}", label);
            println!("Exportable: {}", exportable);
            let (id, address) = generate_new_key(
                match opt.mode {
                    Mode::Usb => {
                        let serial = SerialNumber::from_str(
                            &opt.device_serial_id
                                .expect("USB mode requires a device serial ID"),
                        )
                        .unwrap();
                        Connector::usb(&UsbConfig {
                            serial: Some(serial),
                            timeout_ms: DEFAULT_USB_TIMEOUT_MS,
                        })
                    }
                    Mode::Http => {
                        let addr = opt.http_address.expect("HTTP mode requires an address");
                        let port = opt.http_port.expect("HTTP mode requires a port");
                        Connector::http(&HttpConfig {
                            addr,
                            port,
                            timeout_ms: DEFAULT_HTTP_TIMEOUT_MS,
                        })
                    }
                },
                Credentials::from_password(opt.auth_key_id, opt.password.as_bytes()),
                label,
                exportable,
            )
            .unwrap();

            println!("Key ID: {}", id);
            println!("Address: {}", address);
        }
    }
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
