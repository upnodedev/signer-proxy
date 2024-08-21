use crate::signers::yubihsm::AppState;
use alloy::primitives::hex;
use alloy::{network::EthereumWallet, signers::local::yubihsm::Domain, signers::local::YubiSigner};
use anyhow::Result as AnyhowResult;

use std::sync::Arc;

pub const MOCK_KEYS: &[(u16, [u8; 32], &str)] = &[
    (
        1,
        hex!("25b1759e8eabc06b7d097550dffd7d8c92407fb818c5e9e33b81ef92d4afa2b7"),
        "0x54E0602AfA63cFD1eAED15Ba4a778cD252AB925A",
    ),
    (
        2,
        hex!("5bcaa0de81a26da01ba9e347e8093f2463a3f8e35626914c4984cae19b38288c"),
        "0xe673243b0573080B20E55C62f4d4b685B00427B9",
    ),
];

pub async fn add_mock_wallets(
    state: Arc<AppState>,
    keys: Vec<(u16, [u8; 32], String)>,
) -> AnyhowResult<()> {
    let mut signers = state.signers.lock().await;

    let keys_to_use = if keys.is_empty() {
        MOCK_KEYS
            .iter()
            .map(|&(key_id, private_key, address)| (key_id, private_key, address.to_string()))
            .collect()
    } else {
        keys
    };

    for (key_id, private_key, _address) in keys_to_use {
        let yubi_signer = YubiSigner::from_key(
            state.connector.clone(),
            state.credentials.clone(),
            key_id,
            "".into(),
            Domain::all(),
            private_key,
        )?;
        let eth_signer = EthereumWallet::from(yubi_signer);

        signers.insert(key_id, eth_signer.clone());
    }

    Ok(())
}
