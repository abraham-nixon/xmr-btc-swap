#![warn(
    unused_extern_crates,
    missing_debug_implementations,
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

//! # monero-harness
//!
//! A simple lib to start a monero container (incl. monerod and
//! monero-wallet-rpc). Provides initialisation methods to generate blocks,
//! create and fund accounts, and start a continuous mining task mining blocks
//! every BLOCK_TIME_SECS seconds.
//!
//! Also provides standalone JSON RPC clients for monerod and monero-wallet-rpc.

pub mod image;
pub mod rpc;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::time::Duration;
use testcontainers::{clients::Cli, core::Port, Container, Docker, RunArgs};
use tokio::time;

use crate::{
    image::{ALICE_WALLET_RPC_PORT, BOB_WALLET_RPC_PORT, MINER_WALLET_RPC_PORT, MONEROD_RPC_PORT},
    rpc::{
        monerod,
        wallet::{self, GetAddress, Transfer},
    },
};

/// How often we mine a block.
const BLOCK_TIME_SECS: u64 = 1;

/// Poll interval when checking if the wallet has synced with monerod.
const WAIT_WALLET_SYNC_MILLIS: u64 = 1000;

#[derive(Copy, Clone, Debug)]
pub struct Monero {
    monerod_rpc_port: u16,
}

impl<'c> Monero {
    /// Starts a new regtest monero container.
    pub fn new(cli: &'c Cli) -> Result<(Self, Container<'c, Cli, image::Monero>)> {
        let monerod_rpc_port: u16 =
            port_check::free_local_port().ok_or_else(|| anyhow!("Could not retrieve free port"))?;

        let image = image::Monero::default().with_mapped_port(Port {
            local: monerod_rpc_port,
            internal: MONEROD_RPC_PORT,
        });

        let run_args = RunArgs::default()
            .with_name("monerod")
            .with_network("monero");
        let docker = cli.run_with_args(image, run_args);

        Ok((Self { monerod_rpc_port }, docker))
    }

    pub fn monerod_rpc_client(&self) -> monerod::Client {
        monerod::Client::localhost(self.monerod_rpc_port)
    }

    /// Initialise by creating a wallet, generating some `blocks`, and starting
    /// a miner thread that mines to the primary account. Also create two
    /// sub-accounts, one for Alice and one for Bob. If alice/bob_funding is
    /// some, the value needs to be > 0.
    pub async fn init(&self, alice_funding: u64, bob_funding: u64) -> Result<()> {
        // let miner_wallet = self.miner_wallet_rpc_client();
        // let alice_wallet = self.alice_wallet_rpc_client();
        // let bob_wallet = self.bob_wallet_rpc_client();
        // let monerod = self.monerod_rpc_client();
        //
        // miner_wallet.create_wallet("miner_wallet").await?;
        // alice_wallet.create_wallet("alice_wallet").await?;
        // bob_wallet.create_wallet("bob_wallet").await?;
        //
        // let miner = self.get_address_miner().await?.address;
        // let alice = self.get_address_alice().await?.address;
        // let bob = self.get_address_bob().await?.address;
        //
        // let _ = monerod.generate_blocks(70, &miner).await?;
        // self.wait_for_miner_wallet_block_height().await?;
        //
        // if alice_funding > 0 {
        //     self.fund_account(&alice, &miner, alice_funding).await?;
        //     self.wait_for_alice_wallet_block_height().await?;
        //     let balance = self.get_balance_alice().await?;
        //     debug_assert!(balance == alice_funding);
        // }
        //
        // if bob_funding > 0 {
        //     self.fund_account(&bob, &miner, bob_funding).await?;
        //     self.wait_for_bob_wallet_block_height().await?;
        //     let balance = self.get_balance_bob().await?;
        //     debug_assert!(balance == bob_funding);
        // }
        //
        // let _ = tokio::spawn(mine(monerod.clone(), miner));

        Ok(())
    }
}

// We should be able to use monero-rs for this but it does not include all
// the fields.
#[derive(Clone, Debug, Deserialize)]
pub struct BlockHeader {
    pub block_size: u32,
    pub depth: u32,
    pub difficulty: u32,
    pub hash: String,
    pub height: u32,
    pub major_version: u32,
    pub minor_version: u32,
    pub nonce: u32,
    pub num_txes: u32,
    pub orphan_status: bool,
    pub prev_hash: String,
    pub reward: u64,
    pub timestamp: u32,
}
