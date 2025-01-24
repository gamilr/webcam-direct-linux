use crate::error::Result;
use anyhow::anyhow;
use tokio::sync::{mpsc, oneshot};

use super::ble_cmd_api::{
    BleApi, BleComm, CmdApi, CommandReq, DataChunk, QueryApi, QueryReq,
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
}
