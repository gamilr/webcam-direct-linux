use crate::error::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};

/// Type alias for a responder using oneshot channel.
pub type Responder<T> = oneshot::Sender<T>;

pub const MAX_BUFFER_LEN: usize = 5000; //max buffer length

pub type CommBuffer = Vec<u8>;

/// Represents a chunk of data with remaining length and buffer.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataChunk {
    /// Remaining length of the data.
    pub r: usize,
    /// Buffer containing the data.
    pub d: CommBuffer,
}

/// Request structure for a query.
#[derive(Debug)]
pub struct QueryReq {
    /// Type of the query.
    pub query_type: QueryApi,
    /// Maximum length of the buffer.
    pub resp_buffer_len: usize,
}

/// Type alias for a query response.
pub type QueryResp = Responder<Result<CommBuffer>>;

/// Request structure for a command.
#[derive(Debug)]
pub struct CommandReq {
    /// Type of the command.
    pub cmd_type: CmdApi,
    /// Payload of the command.
    pub payload: CommBuffer,
}

/// Type alias for a command response.
pub type CommandResp = Responder<Result<()>>;

/// Type alias for a PubSub publisher.
pub type PubSubPublisher = broadcast::Sender<DataChunk>;

/// Type alias for a PubSub subscriber.
pub type PubSubSubscriber = broadcast::Receiver<DataChunk>;

/// Request structure for a subscription.
pub struct SubReq {
    /// Topic to subscribe to.
    pub topic: PubSubTopic,
    /// Maximum length of the buffer.
    pub resp_buffer_len: usize,
}

/// Type alias for a subscription response.
pub type SubResp = Responder<Result<PubSubSubscriber>>;

/// Request structure for publishing data.
pub struct PubReq {
    /// Topic to publish to.
    pub topic: PubSubTopic,
    /// Payload to publish.
    pub payload: DataChunk,
}

/// Type alias for a publish response.
pub type PubResp = Responder<Result<()>>;

/// Enum representing different BLE API requests.
pub enum BleApi {
    /// Query request.
    Query(QueryReq, QueryResp),
    /// Command request.
    Command(CommandReq, CommandResp),
    /// Subscription request.
    Sub(SubReq, SubResp),
    /// Publish request.
    Pub(PubReq, PubResp),
}

/// Type alias for an address.
pub type Address = String;

/// Structure representing a BLE communication.
pub struct BleComm {
    /// Address of the BLE device.
    pub addr: Address,

    /// BLE API communication.
    pub comm_api: BleApi,
}

/// Enum representing different BLE command APIs.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum CmdApi {
    /// Mobile disconnected status.
    MobileDisconnected,
    /// Register mobile command.
    RegisterMobile,
    /// Mobile PNP ID command and sdp offer.
    SdpOffer,
}

/// Enum representing different BLE query APIs.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum QueryApi {
    /// Query to read host information.
    HostInfo,
    ///Query to read sdp offer.
    SdpAnswer,
}

/// Enum representing different PubSub topics.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PubSubTopic {
    /// Notify the mobile that the answer is ready for him.
    SdpAnswerReady,
}
