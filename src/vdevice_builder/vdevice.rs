use super::webrtc_pipeline::WebrtcPipeline;
use crate::{ble::mobile_sdp_types::CameraSdp, error::Result};
use anyhow::anyhow;
use log::{error, info};
use serde::{Deserialize, Serialize};
use tokio::task;
use v4l2loopback::{add_device, delete_device, DeviceConfig};

#[derive(Debug, Serialize, Deserialize)]
struct Sdp {
    #[serde(rename = "type")]
    type_: String,
    sdp: String,
}

#[derive(Debug)]
pub struct VDevice {
    pub name: String,
    pub device_num: u32,
    webrtc_pipeline: WebrtcPipeline,
}

impl VDevice {
    pub async fn new(name: String, camera_offer: CameraSdp) -> Result<Self> {
        //get he resolution from the camera offer
        let res_width = camera_offer.format.resolution.0;
        let res_height = camera_offer.format.resolution.1;

        let config = DeviceConfig {
            min_width: 100,
            max_width: 4000,
            min_height: 100,
            max_height: 4000,
            max_buffers: 2,
            max_openers: 9,
            label: name.clone(),
            ..Default::default()
        };

        info!("Adding virtual device with name {}", name);

        let name_clone = name.clone();

        //create the device in a blocking task
        let device_num = task::spawn_blocking(move || {
            add_device(None, config).map_err(|e| {
                error!(
                    "Failed to add virtual device with name {} error {:?}",
                    name_clone, e
                );
                anyhow!(
                    "Failed to add virtual device with name {} error {:?}",
                    name_clone,
                    e
                )
            })
        })
        .await??;

        info!(
            "Virtual device {} added with device number {}",
            name, device_num
        );

        //create the pipeline in a blocking task
        let device_path = format!("/dev/video{}", device_num);
        let sdp_offer: Sdp = serde_json::from_str(&camera_offer.sdp)?;
        //let sdp_offer = camera_offer.sdp.clone();
        let video_prop = camera_offer.format.clone();

        let webrtc_pipeline = task::spawn_blocking(move || {
            WebrtcPipeline::new(device_path, sdp_offer.sdp, video_prop)
        })
        .await??;

        Ok(Self { name, device_num, webrtc_pipeline })
    }

    pub fn get_sdp_answer(&self) -> String {
        self.webrtc_pipeline.get_sdp_answer()
    }
}

impl Drop for VDevice {
    fn drop(&mut self) {
        if let Err(e) = delete_device(self.device_num) {
            error!(
                "Failed to remove virtual device {} with error: {:?}",
                self.name, e
            );
        }
    }
}
