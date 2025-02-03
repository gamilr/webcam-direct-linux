use crate::error::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};

/// Type alias for a responder using oneshot channel.
pub type Responder<T> = oneshot::Sender<T>;

/// Represents a chunk of data with remaining length and buffer.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataChunk {
    /// Remaining length of the data.
    pub remain_len: usize,
    /// Buffer containing the data.
    pub buffer: String,
}

/// Request structure for a query.
#[derive(Debug)]
pub struct QueryReq {
    /// Type of the query.
    pub query_type: QueryApi,
    /// Maximum length of the buffer.
    pub max_buffer_len: usize,
}

/// Type alias for a query response.
pub type QueryResp = Responder<Result<DataChunk>>;

/// Request structure for a command.
#[derive(Debug)]
pub struct CommandReq {
    /// Type of the command.
    pub cmd_type: CmdApi,
    /// Payload of the command.
    pub payload: DataChunk,
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
    pub max_buffer_len: usize,
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
#[derive(Debug)]
pub enum CmdApi {
    /// Mobile disconnected status.
    MobileDisconnected,
    /// Register mobile command.
    RegisterMobile,
    /// Mobile PNP ID command.
    MobilePnpId,
    /// Mobile SDP response command.
    MobileSdpResponse,
}

/// Enum representing different BLE query APIs.
#[derive(Debug)]
pub enum QueryApi {
    /// Query to read host information.
    HostInfo,
}

/// Enum representing different PubSub topics.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PubSubTopic {
    /// SDP call PubSub topic.
    SdpCall,
}
