use crate::error::Result;
use anyhow::anyhow;
use tokio::sync::{broadcast, mpsc, oneshot};


use super::{
    api::{
        BleApi, BleComm, CmdApi, CommBuffer, CommandReq, PubReq,
        PubSubPublisher, PubSubSubscriber, PubSubTopic, QueryApi, QueryReq,
        SubReq,
    },
    comm_types::DataChunk,
};

#[derive(Clone)]
pub struct BleRequester {
    ble_tx: mpsc::Sender<BleComm>,
}

impl BleRequester {
    pub fn new(ble_tx: mpsc::Sender<BleComm>) -> Self {
        Self { ble_tx }
    }

    pub async fn query(
        &self, addr: String, query_type: QueryApi, resp_buffer_len: usize,
    ) -> Result<CommBuffer> {
        let query_req = QueryReq { query_type, resp_buffer_len };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Query(query_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?
    }

    pub async fn cmd(
        &self, addr: String, cmd_type: CmdApi, data: CommBuffer,
    ) -> Result<()> {
        //create the command request
        let cmd_req = CommandReq { cmd_type, payload: data };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Command(cmd_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?
    }

    pub async fn subscribe(
        &self, addr: String, topic: PubSubTopic, resp_buffer_len: usize,
    ) -> Result<BleSubscriber> {
        let sub_req = SubReq { topic, resp_buffer_len };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Sub(sub_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?.map(|subscriber| BleSubscriber::new(subscriber))
    }

    #[allow(dead_code)]
    pub async fn publish(
        &self, addr: String, topic: PubSubTopic, data: Vec<u8>,
    ) -> Result<()> {
        let pub_req = PubReq { topic, payload: data };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Pub(pub_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?
    }
}

#[derive(Clone, Debug)]
pub struct BlePublisher {
    publisher_tx: PubSubPublisher,
    resp_buffer_len: usize,
}

impl BlePublisher {
    pub fn new(resp_buffer_len: usize) -> Self {
        let (publisher_tx, _) = broadcast::channel(128);

        Self { publisher_tx, resp_buffer_len }
    }

    pub async fn publish(&self, buffer: Vec<u8>) -> Result<()> {
        let mut remain_len = buffer.len();

        for chunk in buffer.chunks(self.resp_buffer_len) {
            remain_len -= chunk.len();
            let data_chunk = DataChunk { r: remain_len, d: chunk.to_owned() };

            self.publisher_tx.send(data_chunk.try_into()?)?;
        }

        Ok(())
    }

    pub async fn get_subscriber(&self) -> PubSubSubscriber {
        self.publisher_tx.subscribe()
    }
}

pub struct BleSubscriber {
    subscriber_rx: PubSubSubscriber,
}

impl BleSubscriber {
    pub fn new(subscriber_rx: PubSubSubscriber) -> Self {
        Self { subscriber_rx }
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>> {
        self.subscriber_rx
            .recv()
            .await
            .map_err(|_| anyhow!("Subscriber dropped"))
    }
}
