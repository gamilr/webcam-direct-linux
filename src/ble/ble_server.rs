use std::collections::HashMap;

use super::mobile_sdp_types::{MobileSdpAnswer, MobileSdpOffer};
use crate::app_data::MobileSchema;
use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::error::Result;

#[cfg(test)]
use mockall::automock;

use super::{
    ble_cmd_api::{
        Address, BleApi, BleComm, CmdApi, CommandReq, DataChunk, PubReq,
        PubSubSubscriber, PubSubTopic, QueryApi, QueryReq, SubReq,
    },
    ble_requester::{BlePublisher, BleRequester},
    mobile_buffer::MobileBufferMap,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HostProvInfo {
    pub id: String,
    pub name: String,
    pub connection_type: String,
}

//trait
#[cfg_attr(test, automock)]
#[async_trait]
pub trait MultiMobileCommService: Send + Sync + 'static {
    //provisioning
    async fn register_mobile(
        &mut self, addr: String, mobile: MobileSchema,
    ) -> Result<()>;

    async fn get_host_info<'a>(
        &'a mut self, addr: String,
    ) -> Result<&'a HostProvInfo>;

    //call establishment
    async fn set_mobile_sdp_offer(
        &mut self, addr: String, mobile_offer: MobileSdpOffer,
    ) -> Result<()>;

    async fn sub_to_ready_answer(
        &mut self, addr: String, publisher: BlePublisher,
    ) -> Result<()>;

    async fn get_sdp_answer<'a>(
        &'a mut self, addr: String,
    ) -> Result<&'a MobileSdpAnswer>;

    //disconnected device
    async fn mobile_disconnected(&mut self, addr: String) -> Result<()>;
}

pub struct BleServer {
    ble_req: BleRequester,
    _drop_tx: oneshot::Sender<()>,
}

impl BleServer {
    pub fn new(
        mut comm_handler: impl MultiMobileCommService, req_buffer_size: usize,
    ) -> Self {
        let (ble_tx, mut ble_rx) = mpsc::channel(req_buffer_size);
        let (_drop_tx, mut _drop_rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut ble_server_comm_handler = BleServerCommHandler::new();

            loop {
                tokio::select! {
                    _ = async {
                         if let Some(comm) = ble_rx.recv().await {
                            ble_server_comm_handler.handle_comm(&mut comm_handler, comm).await;
                         }
                    }  => {}

                    _ = &mut _drop_rx => {
                        info!("Ble Server task is stopping");
                        break;
                    }
                }
            }
        });

        Self { ble_req: BleRequester::new(ble_tx), _drop_tx }
    }

    pub fn get_requester(&self) -> BleRequester {
        self.ble_req.clone()
    }
}

//Handle the communication
struct BleServerCommHandler {
    buffer_map: MobileBufferMap,
    pubsub_topics_map: HashMap<PubSubTopic, BlePublisher>,
}

impl BleServerCommHandler {
    pub fn new() -> Self {
        Self {
            buffer_map: MobileBufferMap::new(5000),
            pubsub_topics_map: HashMap::new(),
        }
    }

    //handle query
    async fn handle_query(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, query: QueryReq,
    ) -> Result<DataChunk> {
        debug!("Query: {:?}", query.query_type);

        //get the data requested
        let data = match query.query_type {
            QueryApi::HostInfo => {
                let host_info =
                    comm_handler.get_host_info(addr.clone()).await?;
                serde_json::to_string(&host_info)?
            }

            QueryApi::SdpAnswer => {
                let sdp_offer =
                    comm_handler.get_sdp_answer(addr.clone()).await?;
                serde_json::to_string(&sdp_offer)?
            }
        };

        info!("Query data: {:?}", data);
        info!("Query request: {:?}", query);

        //return the data
        Ok(self.buffer_map.get_next_data_chunk(&addr, &query, &data))
    }

    async fn handle_command(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, cmd: CommandReq,
    ) -> Result<()> {
        debug!("Command: {:?}", cmd.cmd_type);

        let Some(buffer) = self.buffer_map.get_complete_buffer(&addr, &cmd)
        else {
            return Ok(());
        };

        match cmd.cmd_type {
            CmdApi::MobileDisconnected => {
                self.buffer_map.remove_mobile(&addr);
                comm_handler.mobile_disconnected(addr).await
            }
            CmdApi::RegisterMobile => {
                let mobile = serde_json::from_str(&buffer)?;
                comm_handler.register_mobile(addr, mobile).await
            }
            CmdApi::SdpOffer => {
                let mobile_offer = serde_json::from_str(&buffer)?;
                comm_handler.set_mobile_sdp_offer(addr, mobile_offer).await
            }
        }
    }

    async fn handle_sub(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, sub: SubReq,
    ) -> Result<PubSubSubscriber> {
        let SubReq { topic, max_buffer_len } = sub;

        let publisher = self
            .pubsub_topics_map
            .entry(topic)
            .or_insert(BlePublisher::new(max_buffer_len));

        match topic {
            PubSubTopic::SdpAnswerReady => {
                comm_handler
                    .sub_to_ready_answer(addr, publisher.clone())
                    .await?;
            }
        };

        //get the subscriber for this topic
        Ok(publisher.get_subscriber().await)
    }

    async fn handle_pub(
        &mut self, _comm_handler: &mut impl MultiMobileCommService,
        _addr: Address, pub_req: PubReq,
    ) -> Result<()> {
        let PubReq { topic, payload } = pub_req;

        let Some(publisher) = self.pubsub_topics_map.get(&topic) else {
            return Err(anyhow!("PubSub topic not found"));
        };

        match topic {
            PubSubTopic::SdpAnswerReady => {}
        };

        publisher.publish(payload.d.into()).await
    }

    //This function does not return a Result since every request is successful
    //if internally any operation fails, it should handle it accordingly
    pub async fn handle_comm(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        comm: BleComm,
    ) {
        //destructure the request
        let BleComm { addr, comm_api } = comm;

        match comm_api {
            BleApi::Query(req, resp) => {
                if let Err(e) =
                    resp.send(self.handle_query(comm_handler, addr, req).await)
                {
                    error!("Error sending query response: {:?}", e);
                }
            }
            BleApi::Command(req, resp) => {
                if let Err(e) = resp
                    .send(self.handle_command(comm_handler, addr, req).await)
                {
                    error!("Error sending command response: {:?}", e);
                }
            }
            BleApi::Sub(req, resp) => {
                if let Err(e) =
                    resp.send(self.handle_sub(comm_handler, addr, req).await)
                {
                    error!("Error sending sub response: {:?}", e);
                }
            }

            BleApi::Pub(req, resp) => {
                if let Err(e) =
                    resp.send(self.handle_pub(comm_handler, addr, req).await)
                {
                    error!("Error sending pub response: {:?}", e);
                }
            }
        }
    }
}
