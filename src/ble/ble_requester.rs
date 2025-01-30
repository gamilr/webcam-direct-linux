use crate::error::Result;
use anyhow::anyhow;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::ble_cmd_api::{
    BleApi, BleComm, CmdApi, CommandReq, DataChunk, PubReq, PubSubPublisher,
    PubSubSubscriber, PubSubTopic, QueryApi, QueryReq, SubReq,
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
        &self, addr: String, query_type: QueryApi, max_buffer_len: usize,
    ) -> Result<Vec<u8>> {
        let query_req = QueryReq { query_type, max_buffer_len };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Query(query_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        if let Ok(data_chunk) = rx.await? {
            return serde_json::to_vec(&data_chunk)
                .map_err(|e| anyhow!("Error to serialize data chunk {:?}", e));
        }

        Err(anyhow!("Error to get data chunk"))
    }

    pub async fn cmd(
        &self, addr: String, cmd_type: CmdApi, data: Vec<u8>,
    ) -> Result<()> {
        let cmd_req =
            CommandReq { cmd_type, payload: serde_json::from_slice(&data)? };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Command(cmd_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?
    }

    pub async fn subscribe(
        &self, addr: String, topic: PubSubTopic, max_buffer_len: usize,
    ) -> Result<BleSubscriber> {
        let sub_req = SubReq { topic, max_buffer_len };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Sub(sub_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?.map(|subscriber| BleSubscriber::new(subscriber))
    }

    pub async fn publish(
        &self, addr: String, topic: PubSubTopic, data: Vec<u8>,
    ) -> Result<()> {
        let pub_req = PubReq { topic, payload: serde_json::from_slice(&data)? };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Pub(pub_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?
    }
}

pub struct BlePublisher {
    publisher_tx: PubSubPublisher,
    max_buffer_len: usize,
}

impl BlePublisher {
    pub fn new(max_buffer_len: usize) -> Self {
        let (publisher_tx, _) = broadcast::channel(128);
        Self { publisher_tx, max_buffer_len }
    }

    pub async fn publish(&self, data: DataChunk) -> Result<()> {
        let DataChunk { remain_len, buffer } = data;

        for chunk in buffer.as_bytes().chunks(self.max_buffer_len) {
            let data_chunk = DataChunk {
                remain_len: remain_len - chunk.len(),
                buffer: String::from_utf8(chunk.to_vec())?,
            };

            self.publisher_tx.send(data_chunk)?;
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

    pub async fn get_data(&mut self) -> Result<Vec<u8>> {
        if let Ok(data_chunk) = self.subscriber_rx.recv().await {
            return serde_json::to_vec(&data_chunk)
                .map_err(|e| anyhow!("Error to serialize data chunk {:?}", e));
        }

        Err(anyhow!("Error to get data chunk"))
    }
}
