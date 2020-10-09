use anyhow::{anyhow, Result};

use async_trait::async_trait;
use tokio::{
    stream::StreamExt,
    sync::mpsc::{Receiver, Sender},
};
use xmr_btc::{alice, bob, transport::SendReceive};

#[derive(Debug)]
pub struct Transport<SendMsg, RecvMsg> {
    pub sender: Sender<SendMsg>,
    pub receiver: Receiver<RecvMsg>,
}

#[async_trait]
impl SendReceive<alice::Message, bob::Message> for Transport<alice::Message, bob::Message> {
    async fn send_message(&mut self, message: alice::Message) -> Result<()> {
        let _ = self
            .sender
            .send(message)
            .await
            .map_err(|_| anyhow!("failed to send message"))?;
        Ok(())
    }

    async fn receive_message(&mut self) -> Result<bob::Message> {
        let message = self
            .receiver
            .next()
            .await
            .ok_or_else(|| anyhow!("failed to receive message"))?;
        Ok(message)
    }
}

#[async_trait]
impl SendReceive<bob::Message, alice::Message> for Transport<bob::Message, alice::Message> {
    async fn send_message(&mut self, message: bob::Message) -> Result<()> {
        let _ = self
            .sender
            .send(message)
            .await
            .map_err(|_| anyhow!("failed to send message"))?;
        Ok(())
    }

    async fn receive_message(&mut self) -> Result<alice::Message> {
        let message = self
            .receiver
            .next()
            .await
            .ok_or_else(|| anyhow!("failed to receive message"))?;
        Ok(message)
    }
}
