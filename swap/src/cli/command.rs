use crate::env::GetConfig;
use crate::fs::system_data_dir;
use crate::network::rendezvous::{XmrBtcNamespace, DEFAULT_RENDEZVOUS_ADDRESS};
use crate::{env, monero};
use anyhow::{Context, Result};
use libp2p::core::Multiaddr;
use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;
use structopt::{clap, StructOpt};
use url::Url;
use uuid::Uuid;

// See: https://moneroworld.com/
pub const DEFAULT_MONERO_DAEMON_ADDRESS: &str = "node.melo.tools:18081";
pub const DEFAULT_MONERO_DAEMON_ADDRESS_STAGENET: &str = "stagenet.melo.tools:38081";

// See: https://1209k.com/bitcoin-eye/ele.php?chain=btc
const DEFAULT_ELECTRUM_RPC_URL: &str = "ssl://electrum.blockstream.info:50002";
// See: https://1209k.com/bitcoin-eye/ele.php?chain=tbtc
pub const DEFAULT_ELECTRUM_RPC_URL_TESTNET: &str = "ssl://electrum.blockstream.info:60002";

const DEFAULT_BITCOIN_CONFIRMATION_TARGET: usize = 3;
const DEFAULT_BITCOIN_CONFIRMATION_TARGET_TESTNET: usize = 1;

const DEFAULT_TOR_SOCKS5_PORT: &str = "9050";

#[derive(Debug, PartialEq)]
pub struct Arguments {
    pub env_config: env::Config,
    pub debug: bool,
    pub json: bool,
    pub data_dir: PathBuf,
    pub cmd: Command,
}

/// Represents the result of parsing the command-line parameters.
#[derive(Debug, PartialEq)]
pub enum ParseResult {
    /// The arguments we were invoked in.
    Arguments(Arguments),
    /// A flag or command was given that does not need further processing other
    /// than printing the provided message.
    ///
    /// The caller should exit the program with exit code 0.
    PrintAndExitZero { message: String },
}

pub fn parse_args_and_apply_defaults<I, T>(raw_args: I) -> Result<ParseResult>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = match RawArguments::clap().get_matches_from_safe(raw_args) {
        Ok(matches) => RawArguments::from_clap(&matches),
        Err(clap::Error {
            message,
            kind: clap::ErrorKind::HelpDisplayed | clap::ErrorKind::VersionDisplayed,
            ..
        }) => return Ok(ParseResult::PrintAndExitZero { message }),
        Err(e) => anyhow::bail!(e),
    };

    let debug = args.debug;
    let json = args.json;
    let is_testnet = args.testnet;
    let data = args.data;

    let arguments = match args.cmd {
        RawCommand::BuyXmr {
            seller: Seller { seller },
            bitcoin:
                Bitcoin {
                    bitcoin_electrum_rpc_url,
                    bitcoin_target_block,
                },
            bitcoin_change_address,
            monero: Monero {
                monero_daemon_address,
            },
            monero_receive_address,
            tor: Tor { tor_socks5_port },
        } => Arguments {
            env_config: env_config_from(is_testnet),
            debug,
            json,
            data_dir: data::data_dir_from(data, is_testnet)?,
            cmd: Command::BuyXmr {
                seller,
                bitcoin_electrum_rpc_url: bitcoin_electrum_rpc_url_from(
                    bitcoin_electrum_rpc_url,
                    is_testnet,
                )?,
                bitcoin_target_block: bitcoin_target_block_from(bitcoin_target_block, is_testnet),
                bitcoin_change_address,
                monero_receive_address: validate_monero_address(
                    monero_receive_address,
                    is_testnet,
                )?,
                monero_daemon_address: monero_daemon_address_from(
                    monero_daemon_address,
                    is_testnet,
                ),
                tor_socks5_port,
            },
        },
        RawCommand::History => Arguments {
            env_config: env_config_from(is_testnet),
            debug,
            json,
            data_dir: data::data_dir_from(data, is_testnet)?,
            cmd: Command::History,
        },
        RawCommand::Resume {
            swap_id: SwapId { swap_id },
            bitcoin:
                Bitcoin {
                    bitcoin_electrum_rpc_url,
                    bitcoin_target_block,
                },
            monero: Monero {
                monero_daemon_address,
            },
            tor: Tor { tor_socks5_port },
        } => Arguments {
            env_config: env_config_from(is_testnet),
            debug,
            json,
            data_dir: data::data_dir_from(data, is_testnet)?,
            cmd: Command::Resume {
                swap_id,
                bitcoin_electrum_rpc_url: bitcoin_electrum_rpc_url_from(
                    bitcoin_electrum_rpc_url,
                    is_testnet,
                )?,
                bitcoin_target_block: bitcoin_target_block_from(bitcoin_target_block, is_testnet),
                monero_daemon_address: monero_daemon_address_from(
                    monero_daemon_address,
                    is_testnet,
                ),
                tor_socks5_port,
            },
        },
        RawCommand::Cancel {
            swap_id: SwapId { swap_id },
            force,
            bitcoin:
                Bitcoin {
                    bitcoin_electrum_rpc_url,
                    bitcoin_target_block,
                },
        } => Arguments {
            env_config: env_config_from(is_testnet),
            debug,
            json,
            data_dir: data::data_dir_from(data, is_testnet)?,
            cmd: Command::Cancel {
                swap_id,
                force,
                bitcoin_electrum_rpc_url: bitcoin_electrum_rpc_url_from(
                    bitcoin_electrum_rpc_url,
                    is_testnet,
                )?,
                bitcoin_target_block: bitcoin_target_block_from(bitcoin_target_block, is_testnet),
            },
        },
        RawCommand::Refund {
            swap_id: SwapId { swap_id },
            force,
            bitcoin:
                Bitcoin {
                    bitcoin_electrum_rpc_url,
                    bitcoin_target_block,
                },
        } => Arguments {
            env_config: env_config_from(is_testnet),
            debug,
            json,
            data_dir: data::data_dir_from(data, is_testnet)?,
            cmd: Command::Refund {
                swap_id,
                force,
                bitcoin_electrum_rpc_url: bitcoin_electrum_rpc_url_from(
                    bitcoin_electrum_rpc_url,
                    is_testnet,
                )?,
                bitcoin_target_block: bitcoin_target_block_from(bitcoin_target_block, is_testnet),
            },
        },
        RawCommand::ListSellers {
            rendezvous_point,
            tor: Tor { tor_socks5_port },
        } => Arguments {
            env_config: env_config_from(is_testnet),
            debug,
            json,
            data_dir: data::data_dir_from(data, is_testnet)?,
            cmd: Command::ListSellers {
                rendezvous_point,
                namespace: rendezvous_namespace_from(is_testnet),
                tor_socks5_port,
            },
        },
    };

    Ok(ParseResult::Arguments(arguments))
}

#[derive(Debug, PartialEq)]
pub enum Command {
    BuyXmr {
        seller: Multiaddr,
        bitcoin_electrum_rpc_url: Url,
        bitcoin_target_block: usize,
        bitcoin_change_address: bitcoin::Address,
        monero_receive_address: monero::Address,
        monero_daemon_address: String,
        tor_socks5_port: u16,
    },
    History,
    Resume {
        swap_id: Uuid,
        bitcoin_electrum_rpc_url: Url,
        bitcoin_target_block: usize,
        monero_daemon_address: String,
        tor_socks5_port: u16,
    },
    Cancel {
        swap_id: Uuid,
        force: bool,
        bitcoin_electrum_rpc_url: Url,
        bitcoin_target_block: usize,
    },
    Refund {
        swap_id: Uuid,
        force: bool,
        bitcoin_electrum_rpc_url: Url,
        bitcoin_target_block: usize,
    },
    ListSellers {
        rendezvous_point: Multiaddr,
        namespace: XmrBtcNamespace,
        tor_socks5_port: u16,
    },
}

#[derive(structopt::StructOpt, Debug)]
#[structopt(
    name = "swap",
    about = "CLI for swapping BTC for XMR",
    author,
    version = env!("VERGEN_GIT_SEMVER_LIGHTWEIGHT")
)]
struct RawArguments {
    // global is necessary to ensure that clap can match against testnet in subcommands
    #[structopt(
        long,
        help = "Swap on testnet and assume testnet defaults for data-dir and the blockchain related parameters",
        global = true
    )]
    testnet: bool,

    #[structopt(
        long = "--data-base-dir",
        help = "The base data directory to be used for mainnet / testnet specific data like database, wallets etc"
    )]
    data: Option<PathBuf>,

    #[structopt(long, help = "Activate debug logging")]
    debug: bool,

    #[structopt(
        short,
        long = "json",
        help = "Outputs all logs in JSON format instead of plain text"
    )]
    json: bool,

    #[structopt(subcommand)]
    cmd: RawCommand,
}

#[derive(structopt::StructOpt, Debug)]
enum RawCommand {
    /// Start a BTC for XMR swap
    BuyXmr {
        #[structopt(flatten)]
        seller: Seller,

        #[structopt(flatten)]
        bitcoin: Bitcoin,

        #[structopt(
            long = "change-address",
            help = "The bitcoin address where any form of change or excess funds should be sent to"
        )]
        bitcoin_change_address: bitcoin::Address,

        #[structopt(flatten)]
        monero: Monero,

        #[structopt(long = "receive-address",
            help = "The monero address where you would like to receive monero",
            parse(try_from_str = parse_monero_address)
        )]
        monero_receive_address: monero::Address,

        #[structopt(flatten)]
        tor: Tor,
    },
    /// Show a list of past, ongoing and completed swaps
    History,
    /// Resume a swap
    Resume {
        #[structopt(flatten)]
        swap_id: SwapId,

        #[structopt(flatten)]
        bitcoin: Bitcoin,

        #[structopt(flatten)]
        monero: Monero,

        #[structopt(flatten)]
        tor: Tor,
    },
    /// Try to cancel an ongoing swap (expert users only)
    Cancel {
        #[structopt(flatten)]
        swap_id: SwapId,

        #[structopt(short, long)]
        force: bool,

        #[structopt(flatten)]
        bitcoin: Bitcoin,
    },
    /// Try to cancel a swap and refund the BTC (expert users only)
    Refund {
        #[structopt(flatten)]
        swap_id: SwapId,

        #[structopt(short, long)]
        force: bool,

        #[structopt(flatten)]
        bitcoin: Bitcoin,
    },
    /// Discover and list sellers (i.e. ASB providers)
    ListSellers {
        #[structopt(
            long,
            help = "Address of the rendezvous point you want to use to discover ASBs",
            default_value = DEFAULT_RENDEZVOUS_ADDRESS
        )]
        rendezvous_point: Multiaddr,

        #[structopt(flatten)]
        tor: Tor,
    },
}

#[derive(structopt::StructOpt, Debug)]
struct Monero {
    #[structopt(
        long = "monero-daemon-address",
        help = "Specify to connect to a monero daemon of your choice: <host>:<port>"
    )]
    monero_daemon_address: Option<String>,
}

#[derive(structopt::StructOpt, Debug)]
struct Bitcoin {
    #[structopt(long = "electrum-rpc", help = "Provide the Bitcoin Electrum RPC URL")]
    bitcoin_electrum_rpc_url: Option<Url>,

    #[structopt(
        long = "bitcoin-target-block",
        help = "Estimate Bitcoin fees such that transactions are confirmed within the specified number of blocks"
    )]
    bitcoin_target_block: Option<usize>,
}

#[derive(structopt::StructOpt, Debug)]
struct Tor {
    #[structopt(
        long = "tor-socks5-port",
        help = "Your local Tor socks5 proxy port",
        default_value = DEFAULT_TOR_SOCKS5_PORT
    )]
    tor_socks5_port: u16,
}

#[derive(structopt::StructOpt, Debug)]
struct SwapId {
    #[structopt(
        long = "swap-id",
        help = "The swap id can be retrieved using the history subcommand"
    )]
    swap_id: Uuid,
}

#[derive(structopt::StructOpt, Debug)]
struct Seller {
    #[structopt(
        long,
        help = "The seller's address. Must include a peer ID part, i.e. `/p2p/`"
    )]
    seller: Multiaddr,
}

mod data {
    use super::*;

    pub fn data_dir_from(arg_dir: Option<PathBuf>, testnet: bool) -> Result<PathBuf> {
        let base_dir = match arg_dir {
            Some(custom_base_dir) => custom_base_dir,
            None => os_default()?,
        };

        let sub_directory = if testnet { "testnet" } else { "mainnet" };

        Ok(base_dir.join(sub_directory))
    }

    fn os_default() -> Result<PathBuf> {
        Ok(system_data_dir()?.join("cli"))
    }
}

fn bitcoin_electrum_rpc_url_from(url: Option<Url>, testnet: bool) -> Result<Url> {
    if let Some(url) = url {
        Ok(url)
    } else if testnet {
        Ok(Url::from_str(DEFAULT_ELECTRUM_RPC_URL_TESTNET)?)
    } else {
        Ok(Url::from_str(DEFAULT_ELECTRUM_RPC_URL)?)
    }
}

fn rendezvous_namespace_from(is_testnet: bool) -> XmrBtcNamespace {
    if is_testnet {
        XmrBtcNamespace::Testnet
    } else {
        XmrBtcNamespace::Mainnet
    }
}

fn bitcoin_target_block_from(target_block: Option<usize>, testnet: bool) -> usize {
    if let Some(target_block) = target_block {
        target_block
    } else if testnet {
        DEFAULT_BITCOIN_CONFIRMATION_TARGET_TESTNET
    } else {
        DEFAULT_BITCOIN_CONFIRMATION_TARGET
    }
}

fn monero_daemon_address_from(address: Option<String>, testnet: bool) -> String {
    if let Some(address) = address {
        address
    } else if testnet {
        DEFAULT_MONERO_DAEMON_ADDRESS_STAGENET.to_string()
    } else {
        DEFAULT_MONERO_DAEMON_ADDRESS.to_string()
    }
}

fn env_config_from(testnet: bool) -> env::Config {
    if testnet {
        env::Testnet::get_config()
    } else {
        env::Mainnet::get_config()
    }
}

fn validate_monero_address(
    address: monero::Address,
    testnet: bool,
) -> Result<monero::Address, MoneroAddressNetworkMismatch> {
    let expected_network = if testnet {
        monero::Network::Stagenet
    } else {
        monero::Network::Mainnet
    };

    if address.network != expected_network {
        return Err(MoneroAddressNetworkMismatch {
            expected: expected_network,
            actual: address.network,
        });
    }

    Ok(address)
}

fn parse_monero_address(s: &str) -> Result<monero::Address> {
    monero::Address::from_str(s).with_context(|| {
        format!(
            "Failed to parse {} as a monero address, please make sure it is a valid address",
            s
        )
    })
}

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq)]
#[error("Invalid monero address provided, expected address on network {expected:?}  but address provided is on {actual:?}")]
pub struct MoneroAddressNetworkMismatch {
    expected: monero::Network,
    actual: monero::Network,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tor::DEFAULT_SOCKS5_PORT;

    const BINARY_NAME: &str = "swap";

    const TESTNET: &str = "testnet";
    const MAINNET: &str = "mainnet";

    const MONERO_STAGENET_ADDRESS: &str = "53gEuGZUhP9JMEBZoGaFNzhwEgiG7hwQdMCqFxiyiTeFPmkbt1mAoNybEUvYBKHcnrSgxnVWgZsTvRBaHBNXPa8tHiCU51a";
    const BITCOIN_TESTNET_ADDRESS: &str = "tb1qr3em6k3gfnyl8r7q0v7t4tlnyxzgxma3lressv";
    const MONERO_MAINNET_ADDRESS: &str = "44Ato7HveWidJYUAVw5QffEcEtSH1DwzSP3FPPkHxNAS4LX9CqgucphTisH978FLHE34YNEx7FcbBfQLQUU8m3NUC4VqsRa";
    const BITCOIN_MAINNET_ADDRESS: &str = "bc1qe4epnfklcaa0mun26yz5g8k24em5u9f92hy325";
    const MULTI_ADDRESS: &str =
        "/ip4/127.0.0.1/tcp/9939/p2p/12D3KooWCdMKjesXMJz1SiZ7HgotrxuqhQJbP5sgBm2BwP1cqThi";
    const SWAP_ID: &str = "ea030832-3be9-454f-bb98-5ea9a788406b";

    #[test]
    fn given_buy_xmr_on_mainnet_then_defaults_to_mainnet() {
        let raw_ars = vec![
            BINARY_NAME,
            "buy-xmr",
            "--receive-address",
            MONERO_MAINNET_ADDRESS,
            "--change-address",
            BITCOIN_MAINNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let expected_args = ParseResult::Arguments(Arguments::buy_xmr_mainnet_defaults());
        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(expected_args, args);
    }

    #[test]
    fn given_buy_xmr_on_testnet_then_defaults_to_testnet() {
        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "buy-xmr",
            "--receive-address",
            MONERO_STAGENET_ADDRESS,
            "--change-address",
            BITCOIN_TESTNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::buy_xmr_testnet_defaults())
        );
    }

    #[test]
    fn given_buy_xmr_on_mainnet_with_testnet_address_then_fails() {
        let raw_ars = vec![
            BINARY_NAME,
            "buy-xmr",
            "--receive-address",
            MONERO_STAGENET_ADDRESS,
            "--change-address",
            BITCOIN_TESTNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let err = parse_args_and_apply_defaults(raw_ars).unwrap_err();

        assert_eq!(
            err.downcast_ref::<MoneroAddressNetworkMismatch>().unwrap(),
            &MoneroAddressNetworkMismatch {
                expected: monero::Network::Mainnet,
                actual: monero::Network::Stagenet
            }
        );
    }

    #[test]
    fn given_buy_xmr_on_testnet_with_mainnet_address_then_fails() {
        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "buy-xmr",
            "--receive-address",
            MONERO_MAINNET_ADDRESS,
            "--change-address",
            BITCOIN_MAINNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let err = parse_args_and_apply_defaults(raw_ars).unwrap_err();

        assert_eq!(
            err.downcast_ref::<MoneroAddressNetworkMismatch>().unwrap(),
            &MoneroAddressNetworkMismatch {
                expected: monero::Network::Stagenet,
                actual: monero::Network::Mainnet
            }
        );
    }

    #[test]
    fn given_resume_on_mainnet_then_defaults_to_mainnet() {
        let raw_ars = vec![BINARY_NAME, "resume", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::resume_mainnet_defaults())
        );
    }

    #[test]
    fn given_resume_on_testnet_then_defaults_to_testnet() {
        let raw_ars = vec![BINARY_NAME, "--testnet", "resume", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::resume_testnet_defaults())
        );
    }

    #[test]
    fn given_cancel_on_mainnet_then_defaults_to_mainnet() {
        let raw_ars = vec![BINARY_NAME, "cancel", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::cancel_mainnet_defaults())
        );
    }

    #[test]
    fn given_cancel_on_testnet_then_defaults_to_testnet() {
        let raw_ars = vec![BINARY_NAME, "--testnet", "cancel", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::cancel_testnet_defaults())
        );
    }

    #[test]
    fn given_refund_on_mainnet_then_defaults_to_mainnet() {
        let raw_ars = vec![BINARY_NAME, "refund", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::refund_mainnet_defaults())
        );
    }

    #[test]
    fn given_refund_on_testnet_then_defaults_to_testnet() {
        let raw_ars = vec![BINARY_NAME, "--testnet", "refund", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::refund_testnet_defaults())
        );
    }

    #[test]
    fn given_with_data_dir_then_data_dir_set() {
        let data_dir = "/some/path/to/dir";

        let raw_ars = vec![
            BINARY_NAME,
            "--data-base-dir",
            data_dir,
            "buy-xmr",
            "--change-address",
            BITCOIN_MAINNET_ADDRESS,
            "--receive-address",
            MONERO_MAINNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(
                Arguments::buy_xmr_mainnet_defaults()
                    .with_data_dir(PathBuf::from_str(data_dir).unwrap().join("mainnet"))
            )
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "--data-base-dir",
            data_dir,
            "buy-xmr",
            "--change-address",
            BITCOIN_TESTNET_ADDRESS,
            "--receive-address",
            MONERO_STAGENET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(
                Arguments::buy_xmr_testnet_defaults()
                    .with_data_dir(PathBuf::from_str(data_dir).unwrap().join("testnet"))
            )
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--data-base-dir",
            data_dir,
            "resume",
            "--swap-id",
            SWAP_ID,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(
                Arguments::resume_mainnet_defaults()
                    .with_data_dir(PathBuf::from_str(data_dir).unwrap().join("mainnet"))
            )
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "--data-base-dir",
            data_dir,
            "resume",
            "--swap-id",
            SWAP_ID,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();

        assert_eq!(
            args,
            ParseResult::Arguments(
                Arguments::resume_testnet_defaults()
                    .with_data_dir(PathBuf::from_str(data_dir).unwrap().join("testnet"))
            )
        );
    }

    #[test]
    fn given_with_debug_then_debug_set() {
        let raw_ars = vec![
            BINARY_NAME,
            "--debug",
            "buy-xmr",
            "--change-address",
            BITCOIN_MAINNET_ADDRESS,
            "--receive-address",
            MONERO_MAINNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::buy_xmr_mainnet_defaults().with_debug())
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "--debug",
            "buy-xmr",
            "--change-address",
            BITCOIN_TESTNET_ADDRESS,
            "--receive-address",
            MONERO_STAGENET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::buy_xmr_testnet_defaults().with_debug())
        );

        let raw_ars = vec![BINARY_NAME, "--debug", "resume", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::resume_mainnet_defaults().with_debug())
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "--debug",
            "resume",
            "--swap-id",
            SWAP_ID,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::resume_testnet_defaults().with_debug())
        );
    }

    #[test]
    fn given_with_json_then_json_set() {
        let raw_ars = vec![
            BINARY_NAME,
            "--json",
            "buy-xmr",
            "--change-address",
            BITCOIN_MAINNET_ADDRESS,
            "--receive-address",
            MONERO_MAINNET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::buy_xmr_mainnet_defaults().with_json())
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "--json",
            "buy-xmr",
            "--change-address",
            BITCOIN_TESTNET_ADDRESS,
            "--receive-address",
            MONERO_STAGENET_ADDRESS,
            "--seller",
            MULTI_ADDRESS,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::buy_xmr_testnet_defaults().with_json())
        );

        let raw_ars = vec![BINARY_NAME, "--json", "resume", "--swap-id", SWAP_ID];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::resume_mainnet_defaults().with_json())
        );

        let raw_ars = vec![
            BINARY_NAME,
            "--testnet",
            "--json",
            "resume",
            "--swap-id",
            SWAP_ID,
        ];

        let args = parse_args_and_apply_defaults(raw_ars).unwrap();
        assert_eq!(
            args,
            ParseResult::Arguments(Arguments::resume_testnet_defaults().with_json())
        );
    }

    impl Arguments {
        pub fn buy_xmr_testnet_defaults() -> Self {
            Self {
                env_config: env::Testnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(TESTNET),
                cmd: Command::BuyXmr {
                    seller: Multiaddr::from_str(MULTI_ADDRESS).unwrap(),
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL_TESTNET)
                        .unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET_TESTNET,
                    bitcoin_change_address: BITCOIN_TESTNET_ADDRESS.parse().unwrap(),
                    monero_receive_address: monero::Address::from_str(MONERO_STAGENET_ADDRESS)
                        .unwrap(),
                    monero_daemon_address: DEFAULT_MONERO_DAEMON_ADDRESS_STAGENET.to_string(),
                    tor_socks5_port: DEFAULT_SOCKS5_PORT,
                },
            }
        }

        pub fn buy_xmr_mainnet_defaults() -> Self {
            Self {
                env_config: env::Mainnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(MAINNET),
                cmd: Command::BuyXmr {
                    seller: Multiaddr::from_str(MULTI_ADDRESS).unwrap(),
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL).unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET,
                    bitcoin_change_address: BITCOIN_MAINNET_ADDRESS.parse().unwrap(),
                    monero_receive_address: monero::Address::from_str(MONERO_MAINNET_ADDRESS)
                        .unwrap(),
                    monero_daemon_address: DEFAULT_MONERO_DAEMON_ADDRESS.to_string(),
                    tor_socks5_port: DEFAULT_SOCKS5_PORT,
                },
            }
        }

        pub fn resume_testnet_defaults() -> Self {
            Self {
                env_config: env::Testnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(TESTNET),
                cmd: Command::Resume {
                    swap_id: Uuid::from_str(SWAP_ID).unwrap(),
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL_TESTNET)
                        .unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET_TESTNET,
                    monero_daemon_address: DEFAULT_MONERO_DAEMON_ADDRESS_STAGENET.to_string(),
                    tor_socks5_port: DEFAULT_SOCKS5_PORT,
                },
            }
        }

        pub fn resume_mainnet_defaults() -> Self {
            Self {
                env_config: env::Mainnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(MAINNET),
                cmd: Command::Resume {
                    swap_id: Uuid::from_str(SWAP_ID).unwrap(),
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL).unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET,
                    monero_daemon_address: DEFAULT_MONERO_DAEMON_ADDRESS.to_string(),
                    tor_socks5_port: DEFAULT_SOCKS5_PORT,
                },
            }
        }

        pub fn cancel_testnet_defaults() -> Self {
            Self {
                env_config: env::Testnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(TESTNET),
                cmd: Command::Cancel {
                    swap_id: Uuid::from_str(SWAP_ID).unwrap(),
                    force: false,
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL_TESTNET)
                        .unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET_TESTNET,
                },
            }
        }

        pub fn cancel_mainnet_defaults() -> Self {
            Self {
                env_config: env::Mainnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(MAINNET),
                cmd: Command::Cancel {
                    swap_id: Uuid::from_str(SWAP_ID).unwrap(),
                    force: false,
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL).unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET,
                },
            }
        }

        pub fn refund_testnet_defaults() -> Self {
            Self {
                env_config: env::Testnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(TESTNET),
                cmd: Command::Refund {
                    swap_id: Uuid::from_str(SWAP_ID).unwrap(),
                    force: false,
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL_TESTNET)
                        .unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET_TESTNET,
                },
            }
        }

        pub fn refund_mainnet_defaults() -> Self {
            Self {
                env_config: env::Mainnet::get_config(),
                debug: false,
                json: false,
                data_dir: data_dir_path_cli().join(MAINNET),
                cmd: Command::Refund {
                    swap_id: Uuid::from_str(SWAP_ID).unwrap(),
                    force: false,
                    bitcoin_electrum_rpc_url: Url::from_str(DEFAULT_ELECTRUM_RPC_URL).unwrap(),
                    bitcoin_target_block: DEFAULT_BITCOIN_CONFIRMATION_TARGET,
                },
            }
        }

        pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
            self.data_dir = data_dir;
            self
        }

        pub fn with_debug(mut self) -> Self {
            self.debug = true;
            self
        }

        pub fn with_json(mut self) -> Self {
            self.json = true;
            self
        }
    }

    fn data_dir_path_cli() -> PathBuf {
        system_data_dir().unwrap().join("cli")
    }
}
