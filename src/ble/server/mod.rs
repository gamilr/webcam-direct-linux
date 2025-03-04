pub mod mobile_buffer;
pub mod mobile_comm;

use std::collections::HashMap;

use mobile_buffer::MobileBufferMap;

use super::{
    api::{CommBuffer, MAX_BUFFER_LEN},
    comm_types::{CameraSdp, DataChunk, HostProvInfo, MobileSdpOffer},
};
use crate::{app_data::MobileSchema, ble::comm_types::CameraSdpList};
use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, error, info};
use tokio::sync::{mpsc, oneshot};

use crate::error::Result;

use super::{
    api::{
        Address, BleApi, BleComm, CmdApi, CommandReq, PubReq, PubSubSubscriber,
        PubSubTopic, QueryApi, QueryReq, SubReq,
    },
    requester::{BlePublisher, BleRequester},
};

#[cfg(test)]
use mockall::automock;

//trait
#[cfg_attr(test, automock)]
#[async_trait]
pub trait CommDataService: Send + Sync + 'static {
    //provisioning
    async fn register_mobile(
        &mut self, addr: String, mobile: MobileSchema,
    ) -> Result<()>;

    async fn get_host_info(&mut self, addr: String) -> Result<HostProvInfo>;

    //call establishment
    async fn set_mobile_sdp_offer(
        &mut self, addr: String, mobile_offer: MobileSdpOffer,
    ) -> Result<()>;

    async fn sub_to_ready_answer(
        &mut self, addr: String, publisher: BlePublisher,
    ) -> Result<()>;

    async fn get_sdp_answer(&mut self, addr: String) -> Result<Vec<CameraSdp>>;

    //disconnected device
    async fn mobile_disconnected(&mut self, addr: String) -> Result<()>;
}

pub struct BleServer {
    ble_req: BleRequester,
    _drop_tx: oneshot::Sender<()>,
}

impl BleServer {
    pub fn new(
        mut comm_handler: impl CommDataService, req_buffer_size: usize,
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

//data cache
struct ServerDataCache {
    host_info: Option<Vec<u8>>,
    sdp_answer: HashMap<Address, Option<Vec<u8>>>,
}

//Handle the communication
struct BleServerCommHandler {
    buffer_map: MobileBufferMap,
    server_data_cache: ServerDataCache,
    pubsub_topics_map: HashMap<PubSubTopic, BlePublisher>,
    chunk_len: usize,
}

impl BleServerCommHandler {
    pub fn new() -> Self {
        let chunk: Vec<u8> =
            match (DataChunk { r: MAX_BUFFER_LEN, d: vec![] }.try_into()) {
                Ok(chunk) => chunk,
                Err(e) => {
                    error!("Error creating chunk: {:?}", e);
                    vec![]
                }
            };

        let chunk_len = chunk.len();

        Self {
            buffer_map: MobileBufferMap::new(chunk_len),
            server_data_cache: ServerDataCache {
                host_info: None,
                sdp_answer: HashMap::new(),
            },
            pubsub_topics_map: HashMap::new(),
            chunk_len,
        }
    }

    //handle query
    async fn handle_query(
        &mut self, comm_handler: &mut impl CommDataService, addr: Address,
        query: QueryReq,
    ) -> Result<CommBuffer> {
        debug!("Query: {:?}", query.query_type);

        //get the data requested
        let data = match query.query_type {
            QueryApi::HostInfo => {
                if self.server_data_cache.host_info.is_none() {
                    let host_info: Vec<u8> = comm_handler
                        .get_host_info(addr.clone())
                        .await?
                        .try_into()?;

                    self.server_data_cache.host_info = Some(host_info.clone());
                }
                self.server_data_cache.host_info.as_ref().unwrap()
            }

            QueryApi::SdpAnswer => {
                if self.server_data_cache.sdp_answer.get(&addr).is_none() {
                    let sdp_answer: Vec<u8> = CameraSdpList(
                        comm_handler.get_sdp_answer(addr.clone()).await?,
                    )
                    .try_into()?;

                    self.server_data_cache
                        .sdp_answer
                        .insert(addr.clone(), Some(sdp_answer.clone()));
                }

                self.server_data_cache
                    .sdp_answer
                    .get(&addr)
                    .unwrap()
                    .as_ref()
                    .unwrap()
            }
        };

        info!("Query data: {:?}", data);
        info!("Query request: {:?}", query);

        //return the data
        self.buffer_map.get_next_data_chunk(&addr, &query, &data)
    }

    async fn handle_command(
        &mut self, comm_handler: &mut impl CommDataService, addr: Address,
        cmd: CommandReq,
    ) -> Result<()> {
        debug!("Command: {:?}", cmd.cmd_type);

        let Some(buffer) = self.buffer_map.get_complete_buffer(&addr, &cmd)?
        else {
            return Ok(());
        };

        match cmd.cmd_type {
            CmdApi::MobileDisconnected => {
                self.buffer_map.remove_mobile(&addr);
                self.server_data_cache.sdp_answer.remove(&addr);
                comm_handler.mobile_disconnected(addr).await
            }
            CmdApi::RegisterMobile => {
                let mobile = buffer.try_into()?;
                comm_handler.register_mobile(addr, mobile).await
            }
            CmdApi::SdpOffer => {
                let mobile_offer = buffer.try_into()?;
                debug!("Mobile offer: {:?}", mobile_offer);
                comm_handler.set_mobile_sdp_offer(addr, mobile_offer).await
            }
        }
    }

    async fn handle_sub(
        &mut self, comm_handler: &mut impl CommDataService, addr: Address,
        sub: SubReq,
    ) -> Result<PubSubSubscriber> {
        let SubReq { topic, resp_buffer_len } = sub;

        let publisher = self
            .pubsub_topics_map
            .entry(topic)
            .or_insert(BlePublisher::new(resp_buffer_len - self.chunk_len));

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
        &mut self, _comm_handler: &mut impl CommDataService, _addr: Address,
        pub_req: PubReq,
    ) -> Result<()> {
        let PubReq { topic, payload } = pub_req;

        let Some(publisher) = self.pubsub_topics_map.get(&topic) else {
            return Err(anyhow!("PubSub topic not found"));
        };

        match topic {
            PubSubTopic::SdpAnswerReady => {}
        };

        publisher.publish(payload).await
    }

    //This function does not return a Result since every request is successful
    //if internally any operation fails, it should handle it accordingly
    pub async fn handle_comm(
        &mut self, comm_handler: &mut impl CommDataService, comm: BleComm,
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
