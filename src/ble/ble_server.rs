use std::collections::HashMap;

use crate::app_data::{MobileId, MobileSchema};
use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

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
    async fn register_mobile(
        &mut self, addr: String, mobile: MobileSchema,
    ) -> Result<()>;

    async fn get_host_info(&mut self, addr: String) -> Result<HostProvInfo>;

    async fn mobile_disconnected(&mut self, addr: String) -> Result<()>;

    async fn set_mobile_pnp_id(
        &mut self, addr: String, mobile_id: MobileId,
    ) -> Result<()>;

    async fn subscribe_to_sdp_req(&mut self, addr: String) -> Result<()>;

    async fn set_mobile_sdp_resp(
        &mut self, addr: String, sdp: String,
    ) -> Result<()>;
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

struct BleServerCommHandler {
    pub buffer_map: MobileBufferMap,
    pub pubsub_topics_map: HashMap<PubSubTopic, BlePublisher>,
}

impl BleServerCommHandler {
    pub fn new() -> Self {
        Self {
            buffer_map: MobileBufferMap::new(),
            pubsub_topics_map: HashMap::new(),
        }
    }

    //handle query
    async fn handle_query(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, query: QueryReq,
    ) -> Result<DataChunk> {
        let QueryReq { query_type, max_buffer_len } = query;

        debug!("Query: {:?}", query_type);

        //get the data requested
        let data = match query_type {
            QueryApi::HostInfo => {
                let host_info =
                    comm_handler.get_host_info(addr.clone()).await?;
                serde_json::to_string(&host_info)?
            }
        };

        //return the data
        self.buffer_map
            .get_next_data_chunk(addr, max_buffer_len, data)
            .ok_or(anyhow!("No data chunk available"))
    }

    async fn handle_command(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, cmd: CommandReq,
    ) -> Result<()> {
        let CommandReq { cmd_type, payload } = cmd;

        debug!("Command: {:?}", cmd_type);

        let Some(buffer) =
            self.buffer_map.get_complete_buffer(addr.clone(), payload)
        else {
            return Ok(());
        };

        match cmd_type {
            CmdApi::MobileDisconnected => {
                self.buffer_map.remove_mobile(addr.clone());
                comm_handler.mobile_disconnected(addr).await
            }
            CmdApi::RegisterMobile => {
                let mobile = serde_json::from_str(&buffer)?;
                comm_handler.register_mobile(addr, mobile).await
            }
            CmdApi::MobilePnpId => {
                comm_handler.set_mobile_pnp_id(addr, buffer).await
            }
            CmdApi::MobileSdpResponse => {
                comm_handler.set_mobile_sdp_resp(addr, buffer).await
            }
        }
    }

    async fn handle_sub(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, sub: SubReq,
    ) -> Result<PubSubSubscriber> {
        let SubReq { topic, max_buffer_len } = sub;

        //create the topic if it does not exist
        if !self.pubsub_topics_map.contains_key(&topic) {
            self.pubsub_topics_map
                .insert(topic.clone(), BlePublisher::new(max_buffer_len));
        }

        //process the subscription
        match topic {
            PubSubTopic::SdpCall => {
                comm_handler.subscribe_to_sdp_req(addr).await?
            }
        };

        //return the subscriber
        let Some(publisher) = self.pubsub_topics_map.get(&topic) else {
            return Err(anyhow!("PubSub topic not found"));
        };

        return Ok(publisher.get_subscriber().await);
    }

    async fn handle_pub(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        addr: Address, pub_req: PubReq,
    ) -> Result<()> {
        let PubReq { topic, payload } = pub_req;

        let Some(publisher) = self.pubsub_topics_map.get(&topic) else {
            return Err(anyhow!("PubSub topic not found"));
        };

        match topic {
            PubSubTopic::SdpCall => {
                let payload = serde_json::to_string(&payload)?;
                comm_handler.set_mobile_sdp_resp(addr, payload).await?;
            }
        };

        publisher.publish(payload).await
    }

    //This function does not return a Result since every request is successful
    //if internally any operation fails, it should handle it accordingly
    pub async fn handle_comm(
        &mut self, comm_handler: &mut impl MultiMobileCommService,
        comm: BleComm,
    ) {
        //destructure the request
        let BleComm { addr, comm_api } = comm;

        //add the mobile to the buffer map if it does not exist
        //this will indeicate current connection
        if !self.buffer_map.contains_mobile(&addr) {
            self.buffer_map.add_mobile(addr.clone());
        }

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
