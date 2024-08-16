use structopt::StructOpt;

use crate::signers::{aws_kms::AwsOpt, yubihsm::YubiOpt};

#[derive(StructOpt)]
pub struct Opt {
    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    pub cmd: Command,
}

#[derive(StructOpt)]
pub enum Command {
    Yubihsm(YubiOpt),
    AwsKms(AwsOpt),
}
