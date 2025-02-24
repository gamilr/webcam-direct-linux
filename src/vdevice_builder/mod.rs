use crate::ble::mobile_sdp_types::CameraSdp;
use crate::ble::{VDeviceBuilderOps, VDeviceMap};
use crate::error::Result;
use async_trait::async_trait;
use log::{error, info};
use std::path::PathBuf;
use system_utils::{load_kmodule, unload_kmodule, update_dir_permissions};
mod system_utils;
mod vdevice;
mod webrtc_pipeline;

pub use vdevice::VDevice;

use system_utils::is_kmodule_loaded;

pub struct VDeviceBuilder {
    //flags to set up the system at beginning and tear down at the end
    is_v4l2loopback_loaded: bool,
    is_videodev_loaded: bool,
}

impl VDeviceBuilder {
    pub async fn new() -> Result<Self> {
        let mut is_v4l2loopback_loaded = false;
        let mut is_videodev_loaded = false;
        //check for videodev module
        if !is_kmodule_loaded("/proc/modules", "videodev").await? {
            is_v4l2loopback_loaded = true;
            load_kmodule("videodev", None).await?;
            //update_dir_permissions("/dev/v4l2loopback", "o+r").await?;
        }

        //check for v4l2loopback module
        if !is_kmodule_loaded("/proc/modules", "v4l2loopback").await? {
            is_videodev_loaded = true;
            load_kmodule("v4l2loopback", Some(&["exclusive_caps=1"])).await?;
        }

        Ok(Self { is_v4l2loopback_loaded, is_videodev_loaded })
    }
}

#[async_trait]
impl VDeviceBuilderOps for VDeviceBuilder {
    async fn create_from(
        &self, mobile_name: String, camera_offer_list: Vec<CameraSdp>,
    ) -> Result<VDeviceMap> {
        let mut device_map = VDeviceMap::new();

        for camera_offer in camera_offer_list {
            let vdevice_name =
                format!("{}: {}", &mobile_name, &camera_offer.name);
            let camera_name = camera_offer.name.clone();
            let vdevice = match VDevice::new(vdevice_name, camera_offer).await {
                Ok(vdevice) => vdevice,
                Err(e) => {
                    error!("Failed to create virtual device for camera {} error: {:?}", &camera_name, e);
                    continue;
                }
            };
            let path =
                PathBuf::from(format!("/dev/video{}", vdevice.device_num));

            info!(
                "Created virtual device with name {} in path {:?}",
                &vdevice.name, &path
            );

            device_map.insert(camera_name, vdevice);
        }

        Ok(device_map)
    }
}

impl Drop for VDeviceBuilder {
    fn drop(&mut self) {
        //unload the modules
        if self.is_v4l2loopback_loaded
            && unload_kmodule("v4l2loopback").is_err()
        {
            error!("Failed to unload v4l2loopback module");
        }

        if self.is_videodev_loaded && unload_kmodule("videodev").is_err() {
            error!("Failed to unload videodev module");
        }
    }
}
