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
