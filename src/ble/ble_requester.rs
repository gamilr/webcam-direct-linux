use crate::{ble::ble_cmd_api::MAX_BUFFER_LEN, error::Result};
use anyhow::anyhow;
use log::info;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::ble_cmd_api::{
    BleApi, BleComm, CmdApi, CommandReq, DataChunk, PubReq, PubSubPublisher,
    PubSubSubscriber, PubSubTopic, QueryApi, QueryReq, SubReq,
};

#[derive(Clone)]
pub struct BleRequester {
    ble_tx: mpsc::Sender<BleComm>,
    chunk_len: usize,
}

impl BleRequester {
    pub fn new(ble_tx: mpsc::Sender<BleComm>) -> Self {
        //get the data structure length
        let chunk_len = match serde_json::to_vec(&DataChunk {
            r: MAX_BUFFER_LEN,
            d: "".to_string(),
        }) {
            Ok(chunk) => chunk.len(),
            Err(e) => {
                info!("Error to serialize data chunk {:?}", e);
                0
            }
        };

        info!("DataChunk length: {}", chunk_len);

        Self { ble_tx, chunk_len }
    }

    pub async fn query(
        &self, addr: String, query_type: QueryApi, max_buffer_len: usize,
    ) -> Result<Vec<u8>> {
        let query_req = QueryReq {
            query_type,
            resp_buffer_len: max_buffer_len - self.chunk_len,
        };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Query(query_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        match rx.await? {
            Ok(data_chunk) => serde_json::to_vec(&data_chunk)
                .map_err(|e| anyhow!("Error to serialize data chunk {:?}", e)),
            Err(e) => Err(anyhow!("Error to get data chunk {:?}", e)),
        }
    }

    pub async fn cmd(
        &self, addr: String, cmd_type: CmdApi, data: Vec<u8>,
    ) -> Result<()> {
        let data_str = String::from_utf8(data.clone())
            .map_err(|e| anyhow!("Error to convert data to string {:?}", e))?;
        info!("Command data: {}", data_str);
        let cmd_req = CommandReq {
            cmd_type,
            payload: if data.is_empty() {
                DataChunk::default()
            } else {
                serde_json::from_slice(&data)?
            },
        };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Command(cmd_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?
    }

    pub async fn subscribe(
        &self, addr: String, topic: PubSubTopic, max_buffer_len: usize,
    ) -> Result<BleSubscriber> {
        let sub_req =
            SubReq { topic, resp_buffer_len: max_buffer_len - self.chunk_len };

        let (tx, rx) = oneshot::channel();

        let ble_comm = BleComm { addr, comm_api: BleApi::Sub(sub_req, tx) };

        self.ble_tx.send(ble_comm).await?;

        rx.await?.map(|subscriber| BleSubscriber::new(subscriber))
    }

    #[allow(dead_code)]
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

#[derive(Clone, Debug)]
pub struct BlePublisher {
    publisher_tx: PubSubPublisher,
    max_buffer_len: usize,
}

impl BlePublisher {
    pub fn new(max_buffer_len: usize) -> Self {
        let (publisher_tx, _) = broadcast::channel(128);
        Self { publisher_tx, max_buffer_len }
    }

    pub async fn publish(&self, buffer: Vec<u8>) -> Result<()> {
        let mut remain_len = buffer.len();

        for chunk in buffer.chunks(self.max_buffer_len) {
            remain_len -= chunk.len();
            let data_chunk = DataChunk {
                r: remain_len,
                d: String::from_utf8(chunk.to_owned())?,
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
