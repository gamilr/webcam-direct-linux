use crate::error::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};

pub type Address = String;
pub type Responder<T> = oneshot::Sender<T>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataChunk {
    pub remain_len: usize,
    pub payload: String,
}

//Query
#[derive(Debug)]
pub struct Query<RespType> {
    pub addr: Address,
    pub max_buffer_len: usize,
    pub resp: Responder<Result<RespType>>,
}

//Cmd
#[derive(Debug)]
pub struct Cmd<RespType> {
    pub addr: Address,
    pub payload: DataChunk,
    pub resp: Responder<Result<RespType>>,
}

pub type BleQuery = Query<DataChunk>;
pub type BleCmd = Cmd<()>;

//PubSub
pub type PubSubPublisher = broadcast::Sender<DataChunk>;
pub type PubSubSubscriber = broadcast::Receiver<DataChunk>;
pub type BleSub = Query<PubSubSubscriber>;
pub type BlePub = Cmd<()>;

#[derive(Debug, Eq, PartialEq, Hash)]
pub enum PubSubTopic {
    SdpCall, //SDP call pub/sub
}

//Ble API
#[derive(Debug)]
pub enum BleApi {
    //Mobile Connection status
    MobileDisconnected(BleCmd),

    //Register mobile
    RegisterMobile(BleCmd),

    //Read host info
    HostInfo(BleQuery),

    //Mobile Pnp ID
    MobilePnpId(BleCmd),

    //Mobile SDP response
    MobileSdpResponse(BleCmd),

    //Publish/Subscribe API
    Subscribe(PubSubTopic, BleSub),
    Publish(PubSubTopic, BlePub),
}
