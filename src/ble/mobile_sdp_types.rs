use serde::{Deserialize, Serialize};

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
///Mobile Sdp Offer will be sent to the host to establish the connection
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MobileSdpOffer {
    pub mobile_id: String,
    pub camera_offer: Vec<CameraSdp>,
}

///Mobile Sdp Answer will be sent to the mobile to establish the connection
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MobileSdpAnswer {
    pub camera_answer: Vec<CameraSdp>,
}
