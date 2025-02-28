use std::path::PathBuf;

use super::webrtc_pipeline::WebrtcPipeline;
use crate::{ble::mobile_sdp_types::CameraSdp, error::Result};
use anyhow::anyhow;
use log::error;
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
struct V4l2Device {
    pub num: u32,
    pub path: PathBuf,
    pub name: String,
}

impl V4l2Device {
    async fn new(name: String) -> Result<Self> {
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

        //create the device in a blocking task
        let name_clone = name.clone();
        let num = task::spawn_blocking(move || {
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

        let path = format!("/dev/video{}", num);

        Ok(Self { name, num, path: PathBuf::from(path) })
    }
}

impl Drop for V4l2Device {
    fn drop(&mut self) {
        if let Err(e) = delete_device(self.num) {
            error!(
                "Failed to remove virtual device {} with error: {:?}",
                self.name, e
            );
        }
    }
}

#[derive(Debug)]
pub struct VDevice {
    //_v4l2_device: V4l2Device,
    webrtc_pipeline: WebrtcPipeline,
}

impl VDevice {
    pub async fn new(name: String, camera_offer: CameraSdp) -> Result<Self> {
        //get he resolution from the camera offer
        let res_width = camera_offer.format.resolution.0;
        let res_height = camera_offer.format.resolution.1;

        //        let v4l2_device = V4l2Device::new(name.clone()).await?;

        //create the pipeline in a blocking task
        let sdp_offer: Sdp = serde_json::from_str(&camera_offer.sdp)?;
        let video_prop = camera_offer.format.clone();

        //       let device_path_clone = v4l2_device.path.to_string_lossy().to_string();
        let device_path_clone = "/dev/video0".to_string();
        let webrtc_pipeline = task::spawn_blocking(move || {
            WebrtcPipeline::new(device_path_clone, sdp_offer.sdp, video_prop)
        })
        .await??;

        Ok(Self { /*_v4l2_device: v4l2_device,*/ webrtc_pipeline })
    }

    pub fn get_sdp_answer(&self) -> String {
        self.webrtc_pipeline.get_sdp_answer()
    }
}
