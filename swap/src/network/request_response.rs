use async_trait::async_trait;
use futures::prelude::*;
use libp2p::{
    core::upgrade,
    request_response::{ProtocolName, RequestResponseCodec},
};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, io};

use crate::SwapAmounts;
use xmr_btc::{alice, bob, monero};

/// Time to wait for a response back once we send a request.
pub const TIMEOUT: u64 = 3600; // One hour.

// TODO: Think about whether there is a better way to do this, e.g., separate
// Codec for each Message and a macro that implements them.

/// Messages Bob sends to Alice.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum BobToAlice {
    #[serde(with = "::bitcoin::util::amount::serde::as_sat")]
    AmountsFromBtc(::bitcoin::Amount),
    AmountsFromXmr(monero::Amount),
    Message0(bob::Message0),
    Message1(bob::Message1),
    Message2(bob::Message2),
    Message3(bob::Message3),
}

/// Messages Alice sends to Bob.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum AliceToBob {
    Amounts(SwapAmounts),
    Message0(alice::Message0),
    Message1(alice::Message1),
    Message2(alice::Message2),
    Message3, // empty response
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Protocol;

impl ProtocolName for Protocol {
    fn protocol_name(&self) -> &[u8] {
        b"/xmr/btc/1.0.0"
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Codec;

#[async_trait]
impl RequestResponseCodec for Codec {
    type Protocol = Protocol;
    type Request = BobToAlice;
    type Response = AliceToBob;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let message = upgrade::read_one(io, 1024)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut de = serde_json::Deserializer::from_slice(&message);
        let msg = BobToAlice::deserialize(&mut de)?;

        Ok(msg)
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let message = upgrade::read_one(io, 1024)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut de = serde_json::Deserializer::from_slice(&message);
        let msg = AliceToBob::deserialize(&mut de)?;

        Ok(msg)
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = serde_json::to_vec(&req)?;
        upgrade::write_one(io, &bytes).await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = serde_json::to_vec(&res)?;
        upgrade::write_one(io, &bytes).await?;

        Ok(())
    }
}
