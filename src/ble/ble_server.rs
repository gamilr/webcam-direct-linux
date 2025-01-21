use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, error, info, trace};
use tokio::sync::{mpsc, oneshot};

use crate::error::Result;

#[cfg(test)]
use mockall::automock;

use super::{
    ble_cmd_api::{
        Address, BleApi, BleComm, CmdApi, CommandReq, DataChunk,
        PubSubSubscriber, PubSubTopic, QueryApi, QueryReq,
    },
    mobile_buffer::MobileBufferMap,
    HostProvInfo,
};

//trait
#[cfg_attr(test, automock)]
#[async_trait]
pub trait MultiMobileCommService: Send + Sync + 'static {
    async fn set_register_mobile(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()>;

    async fn get_host_info(&mut self, addr: String) -> Result<DataChunk>;

    async fn mobile_disconnected(&mut self, addr: String) -> Result<()>;

    async fn set_mobile_pnp_id(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()>;

    async fn subscribe_to_sdp_req(
        &mut self, addr: String, max_size: usize,
    ) -> Result<PubSubSubscriber>;

    async fn set_mobile_sdp_resp(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()>;
}

pub type ServerConn = mpsc::Sender<BleComm>;

pub struct BleServer {
    ble_tx: ServerConn,
    _drop_tx: oneshot::Sender<()>,
}

impl BleServer {
    pub fn new(
        mut comm_handler: impl MultiMobileCommService, req_buffer_size: usize,
    ) -> Self {
        let (ble_tx, mut ble_rx) = mpsc::channel(req_buffer_size);
        let (_drop_tx, mut _drop_rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut mobiles_buffer_map = MobileBufferMap::new();

            loop {
                tokio::select! {
                    _ = async {
                         if let Some(comm) = ble_rx.recv().await {
                            handle_comm(&mut mobiles_buffer_map, &mut comm_handler, comm).await;
                         }
                    }  => {}

                    _ = &mut _drop_rx => {
                        info!("Ble Server task is stopping");
                        break;
                    }
                }
            }
        });

        Self { ble_tx, _drop_tx }
    }

    pub fn connection(&self) -> ServerConn {
        self.ble_tx.clone()
    }
}

//handle query
async fn handle_query(
    buffer_map: &mut MobileBufferMap,
    comm_handler: &mut impl MultiMobileCommService, addr: Address,
    query: QueryReq,
) -> Result<DataChunk> {
    let QueryReq { query_type, max_buffer_len } = query;

    debug!("Query: {:?}", query_type);

    //get the data requested
    let data_chunk = match query_type {
        QueryApi::HostInfo => {
            let host_info = comm_handler.get_host_info(addr.clone()).await?;
            let host_info_str = serde_json::to_string(&host_info)
                .map_err(|e| anyhow!("Error to serialize host info {:?}", e))?;
            buffer_map
                .get_next_data_chunk(addr, max_buffer_len, host_info_str)
                .ok_or(anyhow!("No data chunk available"))?
        }
    };

    Ok(data_chunk)
}

async fn handle_command(
    buffer_map: &mut MobileBufferMap,
    comm_handler: &mut impl MultiMobileCommService, addr: Address,
    cmd: CommandReq,
) -> Result<()> {
    let CommandReq { cmd_type, payload } = cmd;

    debug!("Command: {:?}", cmd_type);

    let Some(buffer) = buffer_map.get_complete_buffer(addr.clone(), payload)
    else {
        return Ok(());
    };

    match cmd_type {
        CmdApi::MobileDisconnected => {}
        CmdApi::RegisterMobile => {
            let mobile = serde_json::from_str(&buffer)?;
            comm_handler.set_register_mobile(addr, mobile).await?;
        }
        CmdApi::MobilePnpId => {}
        CmdApi::MobileSdpResponse => {}
    }

    Ok(())
}

async fn handle_pubsub(
    client_buffer_cursor: &mut MobileBufferMap,
    comm_handler: &mut impl MultiMobileCommService, pubsub: PubSubTopic,
) {
    match pubsub {
        PubSubTopic::SdpCall => {}
    }
}

//This function does not return a Result since every request is successful
//if internally any operation fails, it should handle it accordingly
async fn handle_comm(
    mobile_buffer_map: &mut MobileBufferMap,
    comm_handler: &mut impl MultiMobileCommService, comm: BleComm,
) {
    //destructure the request
    let BleComm { addr, comm_api } = comm;

    //add the mobile to the buffer map if it does not exist
    //this will indeicate current connection
    if !mobile_buffer_map.contains_mobile(&addr) {
        mobile_buffer_map.add_mobile(addr.clone());
    }

    match comm_api {
        BleApi::Query(req, resp) => {
            if let Err(e) = resp.send(
                handle_query(mobile_buffer_map, comm_handler, addr, req).await,
            ) {
                error!("Error sending query response: {:?}", e);
            }
        }
        BleApi::Command(req, resp) => {
            if let Err(e) = resp.send(
                handle_command(mobile_buffer_map, comm_handler, addr, req)
                    .await,
            ) {
                error!("Error sending command response: {:?}", e);
            }
        }
        BleApi::Sub(req, resp) => {}
        BleApi::Pub(req, resp) => {}
    }
    /*
        match req {
            BleReq::HostInfo(query) => {
                if let Err(e) = query.resp.send(
                    comm_handler
                        .get_host_info(query.addr, query.max_buffer_len)
                        .await,
                ) {
                    error!("Error sending host info: {:?}", e);
                }
            }

            BleReq::MobileConnected(cmd) => {
                trace!("Mobile: {}, connected: {}", cmd.addr, cmd.payload.buffer);

                if cmd.payload.buffer == "true" {
                    debug!("Mobile connected: {:?}", cmd.addr);
                    if let Err(e) = comm_handler.mobile_connected(cmd.addr).await {
                        error!("Error connecting mobile: {:?}", e);
                    } else {
                        client_buffer_cursor
                            .insert(cmd.addr.clone(), BufferCursor::RemainLen(0));
                    }
                } else if let Err(e) =
                    cmd.resp.send(comm_handler.mobile_disconnected(cmd.addr).await)
                {
                    error!("Error disconnecting mobile: {:?}", e);
                } else {
                    client_buffer_cursor.remove(&cmd.addr);
                }
            }

            BleReq::RegisterMobile(cmd) => {
                if let Err(e) = cmd.resp.send(
                    comm_handler.set_register_mobile(cmd.addr, cmd.payload).await,
                ) {
                    error!(
                        "Error sending mobile registration response error: {:?}",
                        e
                    );
                }
            }

            BleReq::MobilePnpId(cmd) => {
                if let Err(e) = cmd.resp.send(
                    comm_handler.set_mobile_pnp_id(cmd.addr, cmd.payload).await,
                ) {
                    error!("Error setting mobile Pnp Id error: {:?}", e);
                }
            }

            BleReq::MobileSdpResponse(cmd) => {
                if let Err(e) = cmd.resp.send(
                    comm_handler.set_mobile_sdp_resp(cmd.addr, cmd.payload).await,
                ) {
                    error!("Error setting mobile sdp response error: {:?}", e);
                }
            }

            BleReq::Subscribe(topic, sub) => {
                //initialize the topic with the first subscriber
                match topic {
                    PubSubTopic::SdpCall => {
                        //process subscribe
                        if let Err(e) = sub.resp.send(
                            comm_handler
                                .subscribe_to_sdp_req(sub.addr, sub.max_buffer_len)
                                .await,
                        ) {
                            error!(
                                "Error sending sdp call sub response, error: {:?}",
                                e
                            );
                        }
                    }
                }
            }
            _ => {
                error!("Not handle request: {:?}", req);
            }
        };
    */
}
