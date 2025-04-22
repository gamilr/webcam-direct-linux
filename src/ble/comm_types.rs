use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

use crate::app_data::MobileSchema;

use anyhow::anyhow;
use rmp_serde::{Deserializer, Serializer};
use std::io::Cursor;

use anyhow::Result;

pub fn msgpack_ser<T: Serialize>(data: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut serializer = Serializer::new(&mut buf).with_struct_map();
    data.serialize(&mut serializer)?;
    Ok(buf)
}

pub fn msgpack_des<'a, T: Deserialize<'a>>(data: &'a [u8]) -> Result<T> {
    let mut de_data = Deserializer::new(Cursor::new(data));
    T::deserialize(&mut de_data).map_err(|e| anyhow!("Failed to deserialize data: {}", e))
}

/// Represents a chunk of data with remaining length and buffer.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataChunk {
    /// Remaining length of the data.
    pub r: usize,
    /// Buffer containing the data.
    pub d: Vec<u8>,
}

impl TryFrom<Vec<u8>> for DataChunk {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        msgpack_des(&bytes)
    }
}

impl TryFrom<DataChunk> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(data: DataChunk) -> Result<Self, Self::Error> {
        msgpack_ser(&data)
    }
}

// SDP Offer and Answer
/// Represents the properties of a video, including resolution and frames per second.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VideoProp {
    pub resolution: (u32, u32),
    pub fps: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CameraSdp {
    pub name: String,
    pub format: VideoProp,
    pub sdp: String,
}

/// Mobile Sdp Offer will be sent to the host to establish the connection
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MobileSdpOffer {
    pub mobile_id: String,
    pub camera_offer: Vec<CameraSdp>,
}

impl TryFrom<Vec<u8>> for MobileSdpOffer {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        msgpack_des(&bytes)
    }
}

impl TryFrom<MobileSdpOffer> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(data: MobileSdpOffer) -> Result<Self, Self::Error> {
        msgpack_ser(&data)
    }
}

/// Mobile Sdp Answer will be sent to the mobile to establish the connection
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MobileSdpAnswer {
    pub camera_answer: Vec<CameraSdp>,
}

impl TryFrom<Vec<u8>> for MobileSdpAnswer {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        msgpack_des(&bytes)
    }
}

impl TryFrom<MobileSdpAnswer> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(data: MobileSdpAnswer) -> Result<Self, Self::Error> {
        msgpack_ser(&data)
    }
}

/// Provisioning information of the host
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HostProvInfo {
    pub id: String,
    pub name: String,
    pub connection_type: String,
}

impl TryFrom<Vec<u8>> for HostProvInfo {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        msgpack_des(&bytes)
    }
}

impl TryFrom<HostProvInfo> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(data: HostProvInfo) -> Result<Self, Self::Error> {
        msgpack_ser(&data)
    }
}

/// Call notification to mobile that the answer is ready
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SdpAnswerReady {
    pub mobile_id: String,
}

impl TryFrom<&[u8]> for SdpAnswerReady {
    type Error = anyhow::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        msgpack_des(bytes)
    }
}

impl TryFrom<SdpAnswerReady> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(data: SdpAnswerReady) -> Result<Self, Self::Error> {
        msgpack_ser(&data)
    }
}

//MobileSchema
impl TryFrom<Vec<u8>> for MobileSchema {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        msgpack_des(&bytes)
    }
}

impl TryFrom<MobileSchema> for Vec<u8> {
    type Error = anyhow::Error;

    fn try_from(data: MobileSchema) -> Result<Self, Self::Error> {
        msgpack_ser(&data)
    }
}
