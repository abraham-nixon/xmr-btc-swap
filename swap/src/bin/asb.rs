#![warn(
    unused_extern_crates,
    missing_copy_implementations,
    rust_2018_idioms,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::fallible_impl_from,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::dbg_macro
)]
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

use anyhow::{Context, Result};
use libp2p::core::multiaddr::Protocol;
use libp2p::core::Multiaddr;
use libp2p::Swarm;
use prettytable::{row, Table};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use structopt::StructOpt;
use swap::asb::command::{Arguments, Command};
use swap::asb::config::{
    default_config_path, initial_setup, query_user_for_initial_testnet_config, read_config, Config,
    ConfigNotInitialized,
};
use swap::database::Database;
use swap::env::GetConfig;
use swap::monero::Amount;
use swap::network::swarm;
use swap::protocol::alice::event_loop::KrakenRate;
use swap::protocol::alice::{run, EventLoop};
use swap::seed::Seed;
use swap::tor::AuthenticatedClient;
use swap::{asb, bitcoin, env, kraken, monero, tor};
use tracing::{info, warn};
use tracing_subscriber::filter::LevelFilter;

#[macro_use]
extern crate prettytable;

const DEFAULT_WALLET_NAME: &str = "asb-wallet";

#[tokio::main]
async fn main() -> Result<()> {
    asb::tracing::init(LevelFilter::DEBUG).expect("initialize tracing");

    let opt = Arguments::from_args();

    let config_path = if let Some(config_path) = opt.config {
        config_path
    } else {
        default_config_path()?
    };

    let config = match read_config(config_path.clone())? {
        Ok(config) => config,
        Err(ConfigNotInitialized {}) => {
            initial_setup(config_path.clone(), query_user_for_initial_testnet_config)?;
            read_config(config_path)?.expect("after initial setup config can be read")
        }
    };

    info!(
        "Database and Seed will be stored in directory: {}",
        config.data.dir.display()
    );

    let db_path = config.data.dir.join("database");

    let db = Database::open(config.data.dir.join(db_path).as_path())
        .context("Could not open database")?;

    let seed =
        Seed::from_file_or_generate(&config.data.dir).expect("Could not retrieve/initialize seed");

    let env_config = env::Testnet::get_config();

    match opt.cmd {
        Command::Start {
            max_buy,
            ask_spread,
        } => {
            let bitcoin_wallet = init_bitcoin_wallet(&config, &seed, env_config).await?;
            let monero_wallet = init_monero_wallet(&config, env_config).await?;

            let bitcoin_balance = bitcoin_wallet.balance().await?;
            info!("Bitcoin balance: {}", bitcoin_balance);

            let monero_balance = monero_wallet.get_balance().await?;
            if monero_balance == Amount::ZERO {
                let deposit_address = monero_wallet.get_main_address();
                warn!(
                    "The Monero balance is 0, make sure to deposit funds at: {}",
                    deposit_address
                )
            } else {
                info!("Monero balance: {}", monero_balance);
            }

            let kraken_price_updates = kraken::connect()?;

            // setup Tor hidden services
            let tor_client =
                tor::Client::new(config.tor.socks5_port).with_control_port(config.tor.control_port);
            let _ac = match tor_client.assert_tor_running().await {
                Ok(_) => {
                    tracing::info!("Tor found. Setting up hidden service. ");
                    let ac =
                        register_tor_services(config.network.clone().listen, tor_client, &seed)
                            .await?;
                    Some(ac)
                }
                Err(_) => {
                    tracing::warn!("Tor not found. Running on clear net. ");
                    None
                }
            };

            let mut swarm = swarm::alice(&seed)?;

            for listen in config.network.listen {
                Swarm::listen_on(&mut swarm, listen.clone())
                    .with_context(|| format!("Failed to listen on network interface {}", listen))?;
            }

            let (event_loop, mut swap_receiver) = EventLoop::new(
                swarm,
                env_config,
                Arc::new(bitcoin_wallet),
                Arc::new(monero_wallet),
                Arc::new(db),
                KrakenRate::new(ask_spread, kraken_price_updates),
                max_buy,
            )
            .unwrap();

            tokio::spawn(async move {
                while let Some(swap) = swap_receiver.recv().await {
                    tokio::spawn(async move {
                        let swap_id = swap.swap_id;
                        match run(swap).await {
                            Ok(state) => {
                                tracing::debug!(%swap_id, "Swap finished with state {}", state)
                            }
                            Err(e) => {
                                tracing::error!(%swap_id, "Swap failed with {:#}", e)
                            }
                        }
                    });
                }
            });

            info!("Our peer id is {}", event_loop.peer_id());

            event_loop.run().await;
        }
        Command::History => {
            let mut table = Table::new();

            table.add_row(row!["SWAP ID", "STATE"]);

            for (swap_id, state) in db.all_alice()? {
                table.add_row(row![swap_id, state]);
            }

            // Print the table to stdout
            table.printstd();
        }
        Command::WithdrawBtc { amount, address } => {
            let bitcoin_wallet = init_bitcoin_wallet(&config, &seed, env_config).await?;

            let amount = match amount {
                Some(amount) => amount,
                None => {
                    bitcoin_wallet
                        .max_giveable(address.script_pubkey().len())
                        .await?
                }
            };

            let psbt = bitcoin_wallet.send_to_address(address, amount).await?;
            let signed_tx = bitcoin_wallet.sign_and_finalize(psbt).await?;

            bitcoin_wallet.broadcast(signed_tx, "withdraw").await?;
        }
        Command::Balance => {
            let bitcoin_wallet = init_bitcoin_wallet(&config, &seed, env_config).await?;
            let monero_wallet = init_monero_wallet(&config, env_config).await?;

            let bitcoin_balance = bitcoin_wallet.balance().await?;
            let monero_balance = monero_wallet.get_balance().await?;

            tracing::info!("Current balance: {}, {}", bitcoin_balance, monero_balance);
        }
    };

    Ok(())
}

async fn init_bitcoin_wallet(
    config: &Config,
    seed: &Seed,
    env_config: swap::env::Config,
) -> Result<bitcoin::Wallet> {
    let wallet_dir = config.data.dir.join("wallet");

    let wallet = bitcoin::Wallet::new(
        config.bitcoin.electrum_rpc_url.clone(),
        &wallet_dir,
        seed.derive_extended_private_key(env_config.bitcoin_network)?,
        env_config,
        6, // TODO move this into config
    )
    .await
    .context("Failed to initialize Bitcoin wallet")?;

    wallet.sync().await?;

    Ok(wallet)
}

async fn init_monero_wallet(
    config: &Config,
    env_config: swap::env::Config,
) -> Result<monero::Wallet> {
    let wallet = monero::Wallet::open_or_create(
        config.monero.wallet_rpc_url.clone(),
        DEFAULT_WALLET_NAME.to_string(),
        env_config,
    )
    .await?;

    Ok(wallet)
}

/// Registers a hidden service for each network.
/// Note: Once ac goes out of scope, the services will be de-registered.
async fn register_tor_services(
    networks: Vec<Multiaddr>,
    tor_client: tor::Client,
    seed: &Seed,
) -> Result<AuthenticatedClient> {
    let mut ac = tor_client.into_authenticated_client().await?;

    let hidden_services_details = networks
        .iter()
        .flat_map(|network| {
            network.iter().map(|protocol| match protocol {
                Protocol::Tcp(port) => Some((
                    port,
                    SocketAddr::new(IpAddr::from(Ipv4Addr::new(127, 0, 0, 1)), port),
                )),
                _ => {
                    // We only care for Tcp for now.
                    None
                }
            })
        })
        .filter_map(|details| details)
        .collect::<Vec<_>>();

    let key = seed.derive_torv3_key();

    ac.add_services(&hidden_services_details, &key).await?;

    let onion_address = key
        .public()
        .get_onion_address()
        .get_address_without_dot_onion();

    hidden_services_details.iter().for_each(|(port, _)| {
        tracing::info!("/onion3/{}:{}", onion_address, port);
    });

    Ok(ac)
}
