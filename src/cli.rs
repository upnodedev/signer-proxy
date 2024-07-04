use structopt::StructOpt;
use strum::{EnumString, VariantNames};

#[derive(EnumString, VariantNames, Debug)]
#[strum(serialize_all = "kebab_case")]
pub enum Mode {
    Usb,
    Http,
}

#[derive(StructOpt)]
pub struct Opt {
    /// Connection mode (usb or http)
    #[structopt(short, long, possible_values = Mode::VARIANTS, case_insensitive = true, default_value = "usb")]
    pub mode: Mode,

    /// YubiHSM device serial ID (for USB mode)
    #[structopt(
        short,
        long = "device-serial",
        env = "YUBIHSM_DEVICE_SERIAL_ID",
        required_if("mode", "usb")
    )]
    pub device_serial_id: Option<String>,

    /// YubiHSM HTTP address (for HTTP mode)
    #[structopt(
        long = "addr",
        env = "YUBIHSM_HTTP_ADDRESS",
        required_if("mode", "http")
    )]
    pub http_address: Option<String>,

    /// YubiHSM HTTP port (for HTTP mode)
    #[structopt(long = "port", env = "YUBIHSM_HTTP_PORT", required_if("mode", "http"))]
    pub http_port: Option<u16>,

    /// YubiHSM auth key ID
    #[structopt(short, long = "auth-key", env = "YUBIHSM_AUTH_KEY_ID")]
    pub auth_key_id: u16,

    /// YubiHSM auth key password
    #[structopt(short, long = "pass", env = "YUBIHSM_PASSWORD", hide_env_values = true)]
    pub password: String,

    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    pub cmd: Command,
}

#[derive(StructOpt)]
pub enum Command {
    Serve,
    GenerateKey {
        /// Key label
        #[structopt(short, long, default_value)]
        label: String,
        /// The key will be exportable or not
        #[structopt(short, long)]
        exportable: bool,
    },
}
