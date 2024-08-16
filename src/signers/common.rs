use alloy::{
    hex,
    network::{EthereumWallet, TransactionBuilder},
    rlp::Encodable,
    rpc::types::TransactionRequest,
};
use anyhow::{anyhow, Result as AnyhowResult};
use serde_json::Value;

use crate::{
    app_types::{AppError, AppJson, AppResult},
    jsonrpc::{JsonRpcReply, JsonRpcRequest, JsonRpcResult},
};

pub async fn handle_eth_sign_transaction(
    payload: JsonRpcRequest<Vec<Value>>,
    signer: EthereumWallet,
) -> AnyhowResult<JsonRpcReply<Value>> {
    if payload.params.is_empty() {
        return Err(anyhow!("params is empty"));
    }

    let tx_object = payload.params[0].clone();
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

pub async fn handle_eth_sign_jsonrpc(
    payload: JsonRpcRequest<Vec<Value>>,
    signer: EthereumWallet,
) -> AppResult<JsonRpcReply<Value>> {
    let method = payload.method.as_str();

    let result = match method {
        "eth_signTransaction" => handle_eth_sign_transaction(payload, signer).await,
        _ => Err(anyhow!(
            "method not supported (eth_signTransaction only): {}",
            method
        )),
    };

    result.map(AppJson).map_err(AppError)
}
