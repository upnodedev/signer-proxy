# yubihsm-signer-proxy

An RPC signer proxy server that listens for the `eth_signTransaction` requests and performs transaction signing using the YubiHSM2 hardware signer.

## Subcommands

### help

Global help command.

```bash
cargo r -r -- help
```

#### Global options for `generate-key` and `serve` subcommands

> [!NOTE]  
> You can connect to YubiHSM2 using two methods: usb or http via `-m, --mode` option.

````bash
-a, --auth-key <auth-key-id>              YubiHSM auth key ID [env: YUBIHSM_AUTH_KEY_ID=]
-d, --device-serial <device-serial-id>    YubiHSM device serial ID (for USB mode) [env: YUBIHSM_DEVICE_SERIAL_ID=]
    --addr <http-address>                 YubiHSM HTTP address (for HTTP mode) [env: YUBIHSM_HTTP_ADDRESS=]
    --port <http-port>                    YubiHSM HTTP port (for HTTP mode) [env: YUBIHSM_HTTP_PORT=]
-m, --mode <mode>                         Connection mode (usb or http) [default: usb]  [possible values: usb, http]
-p, --pass <password>                     YubiHSM auth key password [env: YUBIHSM_PASSWORD]
````

### generate-key

Generates a valid secp256k1 key for signing eth transactions with capability `SIGN_ECDSA` and `EXPORTABLE_UNDER_WRAP` (if flag `-e, --exportable`). See docs about Capability [here](https://docs.yubico.com/hardware/yubihsm-2/hsm-2-user-guide/hsm2-core-concepts.html#capability).

```bash
cargo r -r -- -d <device-serial-id> -a <auth-key-id> -p <password> generate-key -l <label> -e
```

#### Options/flags for `generate-key` subcommand

```bash
cargo r -r -- generate-key -h
```

```bash
-e, --exportable       The key will be exportable or not
-l, --label <label>    Key label [default: ]
```

## serve

Starts a proxy server and listens for `eth_signTransaction` requests.

```bash
cargo r -r -- -d <device-serial-id> -a <auth-key-id> -p <password> serve
```

No additional options and flags for `serve` subcommand.

## Example of valid JSON-RPC request

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
}' http://localhost:3000/key/{id}
```
