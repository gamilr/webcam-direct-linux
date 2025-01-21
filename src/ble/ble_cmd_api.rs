use crate::error::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};

pub type Responder<T> = oneshot::Sender<T>;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataChunk {
    pub remain_len: usize,
    pub buffer: String,
}

//Query
#[derive(Debug)]
pub struct QueryReq {
    pub query_type: QueryApi,
    pub max_buffer_len: usize,
}
pub type QueryResp = Responder<Result<DataChunk>>;

//Command
#[derive(Debug)]
pub struct CommandReq {
    pub cmd_type: CmdApi,
    pub payload: DataChunk,
}
pub type CommandResp = Responder<Result<()>>;

//PubSub
pub type PubSubPublisher = broadcast::Sender<DataChunk>;
pub type PubSubSubscriber = broadcast::Receiver<DataChunk>;

//Sub
pub struct SubReq {
    pub topic: PubSubTopic,
    pub max_buffer_len: usize,
}
pub type SubResp = Responder<Result<PubSubSubscriber>>;

//Pub
pub struct PubReq {
    pub topic: PubSubTopic,
    pub payload: DataChunk,
}
pub type PubResp = Responder<Result<()>>;

//request BleApi
pub enum BleApi {
    Query(QueryReq, QueryResp),
    Command(CommandReq, CommandResp),
    Sub(SubReq, SubResp),
    Pub(PubReq, PubResp),
}

//Ble Request
pub type Address = String;

pub struct BleComm {
    pub addr: Address,
    pub comm_api: BleApi,
}

//Ble API Command
#[derive(Debug)]
pub enum CmdApi {
    //Mobile Connection status
    MobileDisconnected,

    //Register mobile
    RegisterMobile,

    //Mobile Pnp ID
    MobilePnpId,

    //Mobile SDP response
    MobileSdpResponse,
}

//Ble API Query
#[derive(Debug)]
pub enum QueryApi {
    //Read host info
    HostInfo,
}

//Ble PubSub Topic
#[derive(Debug, Eq, PartialEq, Hash)]
pub enum PubSubTopic {
    SdpCall, //SDP call pub/sub
}
