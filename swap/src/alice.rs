//! Run an XMR/BTC swap in the role of Alice.
//! Alice holds XMR and wishes receive BTC.
use anyhow::Result;
use libp2p::{
    core::{identity::Keypair, Multiaddr},
    request_response::ResponseChannel,
    NetworkBehaviour, PeerId,
};
use rand::rngs::OsRng;
use std::thread;
use tracing::{debug, info};

mod amounts;
mod message0;
mod message1;
mod message2;

use self::{amounts::*, message0::*, message1::*, message2::*};
use crate::{
    network::{
        peer_tracker::{self, PeerTracker},
        request_response::AliceToBob,
        transport, TokioExecutor,
    },
    SwapAmounts, PUNISH_TIMELOCK, REFUND_TIMELOCK,
};
use xmr_btc::{alice::State0, bob, monero};

pub type Swarm = libp2p::Swarm<Alice>;

// FIXME: This whole function is horrible, needs total re-write.
pub async fn swap(
    listen: Multiaddr,
    redeem_address: ::bitcoin::Address,
    punish_address: ::bitcoin::Address,
) -> Result<()> {
    let message0: bob::Message0;
    let mut last_amounts: Option<SwapAmounts> = None;

    let mut swarm = new_swarm(listen)?;

    loop {
        match swarm.next().await {
            OutEvent::ConnectionEstablished(id) => {
                info!("Connection established with: {}", id);
            }
            OutEvent::Request(amounts::OutEvent::Btc { btc, channel }) => {
                debug!("Got request from Bob to swap {}", btc);
                let amounts = calculate_amounts(btc);
                // TODO: We cache the last amounts returned, this needs improving along with
                // verification of message 0.
                last_amounts = Some(amounts);
                swarm.send_amounts(channel, amounts);
            }
            OutEvent::Message0(msg) => {
                // We don't want Bob to be able to crash us by sending an out of
                // order message. Keep looping if Bob has not requested amounts.
                if last_amounts.is_some() {
                    // TODO: We should verify the amounts and notify Bob if they have changed.
                    message0 = msg;
                    break;
                }
            }
            other => panic!("Unexpected event: {:?}", other),
        };
    }

    let (xmr, btc) = match last_amounts {
        Some(p) => (p.xmr, p.btc),
        None => unreachable!("should have amounts by here"),
    };

    // TODO: Pass this in using <R: RngCore + CryptoRng>
    let rng = &mut OsRng;
    let state0 = State0::new(
        rng,
        btc,
        xmr,
        REFUND_TIMELOCK,
        PUNISH_TIMELOCK,
        redeem_address,
        punish_address,
    );
    swarm.set_state0(state0.clone());

    let state1 = state0.receive(message0).expect("failed to receive msg 0");

    let (state2, channel) = match swarm.next().await {
        OutEvent::Message1 { msg, channel } => {
            let state2 = state1.receive(msg);
            (state2, channel)
        }
        other => panic!("Unexpected event: {:?}", other),
    };

    let msg = state2.next_message();
    swarm.send_message1(channel, msg);

    let _state3 = match swarm.next().await {
        OutEvent::Message2(msg) => state2.receive(msg)?,
        other => panic!("Unexpected event: {:?}", other),
    };

    info!("Handshake complete, we now have State3 for Alice.");

    thread::park();
    Ok(())
}

fn new_swarm(listen: Multiaddr) -> Result<Swarm> {
    use anyhow::Context as _;

    let behaviour = Alice::default();

    let local_key_pair = behaviour.identity();
    let local_peer_id = behaviour.peer_id();

    let transport = transport::build(local_key_pair)?;

    let mut swarm = libp2p::swarm::SwarmBuilder::new(transport, behaviour, local_peer_id.clone())
        .executor(Box::new(TokioExecutor {
            handle: tokio::runtime::Handle::current(),
        }))
        .build();

    Swarm::listen_on(&mut swarm, listen.clone())
        .with_context(|| format!("Address is not supported: {:#}", listen))?;

    info!("Initialized swarm: {}", local_peer_id);

    Ok(swarm)
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum OutEvent {
    ConnectionEstablished(PeerId),
    Request(amounts::OutEvent), // Not-uniform with Bob on purpose, ready for adding Xmr event.
    Message0(bob::Message0),
    Message1 {
        msg: bob::Message1,
        channel: ResponseChannel<AliceToBob>,
    },
    Message2(bob::Message2),
}

impl From<peer_tracker::OutEvent> for OutEvent {
    fn from(event: peer_tracker::OutEvent) -> Self {
        match event {
            peer_tracker::OutEvent::ConnectionEstablished(id) => {
                OutEvent::ConnectionEstablished(id)
            }
        }
    }
}

impl From<amounts::OutEvent> for OutEvent {
    fn from(event: amounts::OutEvent) -> Self {
        OutEvent::Request(event)
    }
}

impl From<message0::OutEvent> for OutEvent {
    fn from(event: message0::OutEvent) -> Self {
        match event {
            message0::OutEvent::Msg(msg) => OutEvent::Message0(msg),
        }
    }
}

impl From<message1::OutEvent> for OutEvent {
    fn from(event: message1::OutEvent) -> Self {
        match event {
            message1::OutEvent::Msg { msg, channel } => OutEvent::Message1 { msg, channel },
        }
    }
}

impl From<message2::OutEvent> for OutEvent {
    fn from(event: message2::OutEvent) -> Self {
        match event {
            message2::OutEvent::Msg(msg) => OutEvent::Message2(msg),
        }
    }
}

/// A `NetworkBehaviour` that represents an XMR/BTC swap node as Alice.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent", event_process = false)]
#[allow(missing_debug_implementations)]
pub struct Alice {
    pt: PeerTracker,
    amounts: Amounts,
    message0: Message0,
    message1: Message1,
    message2: Message2,
    #[behaviour(ignore)]
    identity: Keypair,
}

impl Alice {
    pub fn identity(&self) -> Keypair {
        self.identity.clone()
    }

    pub fn peer_id(&self) -> PeerId {
        PeerId::from(self.identity.public())
    }

    /// Alice always sends her messages as a response to a request from Bob.
    pub fn send_amounts(&mut self, channel: ResponseChannel<AliceToBob>, amounts: SwapAmounts) {
        let msg = AliceToBob::Amounts(amounts);
        self.amounts.send(channel, msg);
    }

    /// Message0 gets sent within the network layer using this state0.
    pub fn set_state0(&mut self, state: State0) {
        let _ = self.message0.set_state(state);
    }

    /// Send Message1 to Bob in response to receiving his Message1.
    pub fn send_message1(
        &mut self,
        channel: ResponseChannel<AliceToBob>,
        msg: xmr_btc::alice::Message1,
    ) {
        self.message1.send(channel, msg)
    }
}

impl Default for Alice {
    fn default() -> Self {
        let identity = Keypair::generate_ed25519();

        Self {
            pt: PeerTracker::default(),
            amounts: Amounts::default(),
            message0: Message0::default(),
            message1: Message1::default(),
            message2: Message2::default(),
            identity,
        }
    }
}

fn calculate_amounts(btc: ::bitcoin::Amount) -> SwapAmounts {
    const XMR_PER_BTC: u64 = 100; // TODO: Get this from an exchange.

    // TODO: Check that this is correct.
    // XMR uses 12 zerose BTC uses 8.
    let picos = (btc.as_sat() * 10000) * XMR_PER_BTC;
    let xmr = monero::Amount::from_piconero(picos);

    SwapAmounts { btc, xmr }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_BTC: u64 = 100_000_000;
    const HUNDRED_XMR: u64 = 100_000_000_000_000;

    #[test]
    fn one_bitcoin_equals_a_hundred_moneroj() {
        let btc = ::bitcoin::Amount::from_sat(ONE_BTC);
        let want = monero::Amount::from_piconero(HUNDRED_XMR);

        let SwapAmounts { xmr: got, .. } = calculate_amounts(btc);
        assert_eq!(got, want);
    }
}
