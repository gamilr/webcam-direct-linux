use crate::app_data::MobileSchema;
use crate::ble::{VDeviceBuilderOps, VDeviceMap};
use crate::error::Result;
use async_trait::async_trait;
use log::{error, info};
use std::path::PathBuf;
use system_utils::{load_kmodule, unload_kmodule};
mod system_utils;
mod vdevice;

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
    async fn create_from(&self, mobile: MobileSchema) -> Result<VDeviceMap> {
        let mut device_map = VDeviceMap::new();

        for camera in &mobile.cameras {
            if let Ok(vdevice) =
                VDevice::new(format!("{}-{}", &mobile.name, &camera.name)).await
            {
                let path =
                    PathBuf::from(format!("/dev/video{}", vdevice.device_num));

                info!(
                    "Created virtual device with name {} in path {:?}",
                    &vdevice.name, &path
                );

                device_map.insert(path, vdevice);
            }
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
