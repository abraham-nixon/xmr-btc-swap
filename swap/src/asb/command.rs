use crate::bitcoin::Amount;
use bitcoin::util::amount::ParseAmountError;
use bitcoin::{Address, Denomination};
use rust_decimal::Decimal;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(structopt::StructOpt, Debug)]
#[structopt(
    name = "asb",
    about = "Automated Swap Backend for swapping XMR for BTC",
    author
)]
pub struct Arguments {
    #[structopt(
        long = "config",
        help = "Provide a custom path to the configuration file. The configuration file must be a toml file.",
        parse(from_os_str)
    )]
    pub config: Option<PathBuf>,

    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(structopt::StructOpt, Debug)]
#[structopt(name = "xmr_btc-swap", about = "XMR BTC atomic swap")]
pub enum Command {
    #[structopt(about = "Main command to run the ASB.")]
    Start {
        #[structopt(long = "max-buy-btc", help = "The maximum amount of BTC the ASB is willing to buy.", default_value = "0.005", parse(try_from_str = parse_btc))]
        max_buy: Amount,
        #[structopt(
            long = "ask-spread",
            help = "The spread in percent that should be applied to the asking price.",
            default_value = "0.02"
        )]
        ask_spread: Decimal,

        #[structopt(
            long = "resume-only",
            help = "For maintenance only. When set, no new swap requests will be accepted, but existing unfinished swaps will be resumed."
        )]
        resume_only: bool,
    },
    #[structopt(about = "Prints swap-id and the state of each swap ever made.")]
    History,
    #[structopt(about = "Allows withdrawing BTC from the internal Bitcoin wallet.")]
    WithdrawBtc {
        #[structopt(
            long = "amount",
            help = "Optionally specify the amount of Bitcoin to be withdrawn. If not specified the wallet will be drained."
        )]
        amount: Option<Amount>,
        #[structopt(long = "address", help = "The address to receive the Bitcoin.")]
        address: Address,
    },
    #[structopt(
        about = "Prints the Bitcoin and Monero balance. Requires the monero-wallet-rpc to be running."
    )]
    Balance,
    #[structopt(about = "Contains sub-commands for recovering a swap manually.")]
    ManualRecovery(ManualRecovery),
}

#[derive(structopt::StructOpt, Debug)]
pub enum ManualRecovery {
    #[structopt(
        about = "Publishes the Bitcoin cancel transaction. By default, the cancel timelock will be enforced. A confirmed cancel transaction enables refund and punish."
    )]
    Cancel {
        #[structopt(flatten)]
        cancel_params: RecoverCommandParams,
    },
    #[structopt(
        about = "Publishes the Monero refund transaction. By default, a swap-state where the cancel transaction was already published will be enforced. This command requires the counterparty Bitcoin refund transaction and will error if it was not published yet. "
    )]
    Refund {
        #[structopt(flatten)]
        refund_params: RecoverCommandParams,
    },
}

#[derive(structopt::StructOpt, Debug)]
pub struct RecoverCommandParams {
    #[structopt(
        long = "swap-id",
        help = "The swap id can be retrieved using the history subcommand"
    )]
    pub swap_id: Uuid,

    #[structopt(
        short,
        long,
        help = "Circumvents certain checks when recovering. It is recommended to run a recovery command without --force first to see what is returned."
    )]
    pub force: bool,
}

fn parse_btc(s: &str) -> Result<Amount, ParseAmountError> {
    Amount::from_str_in(s, Denomination::Bitcoin)
}
