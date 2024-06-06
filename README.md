# yubihsm-signer-proxy

## Help

```bash
cargo r -r -- -h
```

```bash
yubihsm-signer-proxy 0.1.0

USAGE:
    yubihsm-signer-proxy --auth-key-id <auth-key-id> --device-serial-id <device-serial-id> --password <password> --rpc-url <rpc-url> --signing-key-id <signing-key-id>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a, --auth-key-id <auth-key-id>              YubiHSM auth key ID [env: YUBIHSM_AUTH_KEY_ID=]
    -d, --device-serial-id <device-serial-id>    YubiHSM device serial ID [env: YUBIHSM_DEVICE_SERIAL_ID=]
    -p, --password <password>                    YubiHSM auth key password [env: YUBIHSM_PASSWORD]
    -r, --rpc-url <rpc-url>                      RPC URL [env: RPC_URL=]
    -s, --signing-key-id <signing-key-id>        YubiHSM signing key ID [env: YUBIHSM_SIGNING_KEY_ID=]
```

## Run

```bash
cargo r -r -- -a 4 -d "001" -p "password" -s 1 -r "https://optimism-sepolia.drpc.org"
```

## Test

```bash
curl -X POST -H "Content-Type: application/json" -d '{
    "id": 1,
    "jsonrpc": "2.0",
    "method": "eth_signTransaction",
    "params": [
        {
            "chainId": 11155420,
            "data": "0x",
            "from": "0x",
            "gas": "0x7b0c",
            "gasPrice": "0x1250b1",
            "nonce": "0x0",
            "to": "0x",
            "value": "0x2386f26fc10000"
        }
    ]
}' http://localhost:3000
```
