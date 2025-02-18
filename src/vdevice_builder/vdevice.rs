use crate::{ble::mobile_sdp_types::CameraSdp, error::Result};
use anyhow::anyhow;
use log::{error, info};
use tokio::task;
use v4l2loopback::{add_device, delete_device, DeviceConfig};

use super::webrtc_pipeline::WebrtcPipeline;

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
            min_width: res_width,
            max_width: 4000,
            min_height: res_height,
            max_height: 4000,
            max_buffers: 9,
            max_openers: 1,
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

        //create the pipeline in a blocking task
        let name_clone = name.clone();
        let sdp_offer = camera_offer.sdp.clone();
        let video_prop = camera_offer.format.clone();

        let webrtc_pipeline = task::spawn_blocking(move || {
            WebrtcPipeline::new(name_clone, sdp_offer, video_prop)
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
