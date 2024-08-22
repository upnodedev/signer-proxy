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
    let params = payload.params.ok_or_else(|| anyhow!("params is empty"))?;

    if params.is_empty() {
        return Err(anyhow!("params is empty"));
    }

    let tx_object = params[0].clone();
    let tx_request = serde_json::from_value::<TransactionRequest>(tx_object)?;
    let tx_envelope = tx_request.build(&signer).await?;
    println!("tx_envelope: {:?}", tx_envelope.tx_type());
    println!("tx_envelope: {:?}", tx_envelope);
    tx_envelope.signature_hash();

    let mut encoded_tx = vec![];
    encoded_tx.push(tx_envelope.tx_type() as u8);
    tx_envelope.encode(&mut encoded_tx);
    println!("encoded_tx: {:?}", encoded_tx);
    let rlp_hex = hex::encode_prefixed(encoded_tx);

    println!("rlp_hex: {:?}", rlp_hex);

    Ok(JsonRpcReply {
        id: payload.id,
        jsonrpc: payload.jsonrpc,
        result: JsonRpcResult::Result(rlp_hex.into()),
    })
}

pub async fn handle_health_status(
    payload: JsonRpcRequest<Vec<Value>>,
) -> AnyhowResult<JsonRpcReply<Value>> {
    Ok(JsonRpcReply {
        id: payload.id,
        jsonrpc: payload.jsonrpc,
        result: JsonRpcResult::Result(env!("CARGO_PKG_VERSION").into()),
    })
}

pub async fn handle_eth_sign_jsonrpc(
    payload: JsonRpcRequest<Vec<Value>>,
    signer: EthereumWallet,
) -> AppResult<JsonRpcReply<Value>> {
    let method = payload.method.as_str();

    let result = match method {
        "eth_signTransaction" => handle_eth_sign_transaction(payload, signer).await,
        "health_status" => handle_health_status(payload).await,
        _ => Err(anyhow!(
            "method not supported (only eth_signTransaction and health_status): {}",
            method
        )),
    };

    result.map(AppJson).map_err(AppError)
}
