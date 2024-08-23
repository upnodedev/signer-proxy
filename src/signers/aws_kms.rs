use std::{collections::HashMap, sync::Arc, time::Duration};

use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::signers::{aws::AwsSigner, Signer};
use anyhow::Result as AnyhowResult;
use aws_config::BehaviorVersion;
use aws_sdk_kms::Client;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Json;
use axum::{
    debug_handler,
    extract::{Path, State},
    routing::post,
    Router,
};
use serde_json::Value;
use structopt::StructOpt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::info;

use crate::jsonrpc::AddressResponse;
use crate::{
    app_types::{AppJson, AppResult},
    jsonrpc::{JsonRpcReply, JsonRpcRequest},
    shutdown_signal::shutdown_signal,
    signers::common::handle_eth_sign_jsonrpc,
};

#[derive(StructOpt)]
pub struct AwsOpt {
    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    pub cmd: AwsCommand,
}

#[derive(StructOpt)]
pub enum AwsCommand {
    Serve,
}

#[derive(Clone)]
struct AppState {
    client: Client,
    signers: Arc<Mutex<HashMap<String, EthereumWallet>>>,
}

const API_TIMEOUT_SECS: u64 = 30;

#[debug_handler]
async fn handle_ping() -> &'static str {
    "pong"
}

#[debug_handler]
async fn handle_request(
    Path(key_id): Path<String>,
    State(state): State<Arc<AppState>>,
    AppJson(payload): AppJson<JsonRpcRequest<Vec<Value>>>,
) -> AppResult<JsonRpcReply<Value>> {
    let eth_signer = get_signer(state.clone(), key_id).await?;
    handle_eth_sign_jsonrpc(payload, eth_signer).await
}

async fn get_signer(state: Arc<AppState>, key_id: String) -> AnyhowResult<EthereumWallet> {
    let mut signers = state.signers.lock().await;

    if let Some(signer) = signers.get(&key_id) {
        return Ok(signer.clone());
    }

    let signer = AwsSigner::new(state.client.clone(), key_id.clone(), None).await?;
    let eth_signer = EthereumWallet::from(signer);

    signers.insert(key_id.clone(), eth_signer.clone());

    Ok(eth_signer)
}

#[debug_handler]
async fn handle_address_request(
    Path(key_id): Path<String>,
    State(state): State<Arc<AppState>>,
    AppJson(_payload): AppJson<JsonRpcRequest<Vec<Value>>>,
) -> Result<Json<AddressResponse>, StatusCode> {
    match get_address(state.clone(), key_id).await {
        Ok(address) => Ok(Json(AddressResponse {
            address: address.to_string(),
        })),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_address(state: Arc<AppState>, key_id: String) -> AnyhowResult<Address> {
    let signer = AwsSigner::new(state.client.clone(), key_id.clone(), None).await?;

    Ok(signer.address())
}

pub async fn handle_aws_kms(opt: AwsOpt) {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = aws_sdk_kms::Client::new(&config);

    match opt.cmd {
        AwsCommand::Serve => {
            let shared_state = Arc::new(AppState {
                client,
                signers: Arc::new(Mutex::new(HashMap::new())),
            });

            let app = Router::new()
                .route("/ping", get(handle_ping))
                .route("/key/:key_id", post(handle_request))
                .route("/key/:key_id/address", get(handle_address_request))
                .with_state(shared_state)
                .layer((
                    TraceLayer::new_for_http(),
                    TimeoutLayer::new(Duration::from_secs(API_TIMEOUT_SECS)),
                ));

            let listener = TcpListener::bind("0.0.0.0:4000").await.unwrap();
            info!("listening on {}", listener.local_addr().unwrap());
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap();
        }
    }
}
