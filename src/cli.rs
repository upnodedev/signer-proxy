use structopt::StructOpt;

use crate::yubihsm::YubiOpt;

#[derive(StructOpt)]
pub struct Opt {
    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    pub cmd: Command,
}

#[derive(StructOpt)]
pub enum Command {
    Yubihsm(YubiOpt),
}
