use crate::{ble::ble_requester::BlePublisher, error::Result};
use anyhow::anyhow;
use log::{error, info};
use v4l2loopback::{add_device, delete_device, DeviceConfig};

#[derive(Debug, Clone)]
pub struct VDevice {
    pub name: String,
    pub device_num: u32,
    pub caller: Option<BlePublisher>,
}

impl VDevice {
    pub async fn new(name: String) -> Result<Self> {
        let config = DeviceConfig {
            min_width: 100,
            max_width: 4000,
            min_height: 100,
            max_height: 4000,
            max_buffers: 9,
            max_openers: 1,
            label: name.clone(),
            ..Default::default()
        };

        info!("Adding virtual device with name {}", name);

        let device_num = match add_device(None, config)
            .map_err(|e| anyhow!("Failed to add device: {:?}", e))
        {
            Ok(num) => num,
            Err(e) => {
                // Handle the error from add_device
                error!("Error adding device: {:?}", e);
                return Err(e);
            }
        };

        Ok(Self { name, device_num, caller: None })
    }

    pub fn set_caller(&mut self, caller: BlePublisher) {
        self.caller = Some(caller);
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
