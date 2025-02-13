use serde::{Deserialize, Serialize};

/// Represents the properties of a video, including resolution and frames per second.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VideoProp {
    resolution: (u32, u32),
    fps: u32,
}

/// Represents information about a camera, including its name and supported video formats.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CameraInfo {
    pub name: String,
    pub format: VideoProp,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CameraSdp {
    pub camera: CameraInfo,
    pub sdp: String,
}
///Mobile Sdp Offer will be sent to the host to establish the connection
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MobileSdpOffer {
    pub mobile_id: String,
    pub camera_offer: Vec<CameraSdp>,
}

///Mobile Sdp Answer will be sent to the mobile to establish the connection
pub struct MobileSdpAnswer {
    pub mobile_id: String,
    pub camera_answer: Vec<CameraSdp>,
}
