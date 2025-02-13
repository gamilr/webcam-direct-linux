use crate::app_data::{MobileId, MobileSchema};
use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use log::{debug, trace};

use anyhow::anyhow;

use super::{
    ble_cmd_api::Address,
    ble_requester::BlePublisher,
    ble_server::{HostProvInfo, MultiMobileCommService},
    mobile_sdp_types::{CameraSdp, MobileSdpOffer},
};
use crate::error::Result;
use crate::vdevice_builder::VDevice;

#[cfg(test)]
use mockall::automock;

/// A trait that defines the operations for interacting with the application's data store.
#[cfg_attr(test, automock)]
pub trait AppDataStore: Send + Sync + 'static {
    /// Retrieves the host provisioning info  from the data store.
    ///
    /// # Errors
    ///
    /// Returns an error if the host information is not found in the data store.
    fn get_host_prov_info(&self) -> Result<HostProvInfo>;

    /// Adds a mobile device to the data store.
    ///
    /// # Errors
    ///
    /// Returns an error if the host information is not found in the data store.
    fn add_mobile(&mut self, mobile: &MobileSchema) -> Result<()>;

    fn get_mobile(&self, id: &str) -> Result<MobileSchema>;
}

pub type VDeviceMap = HashMap<PathBuf, VDevice>;

#[async_trait]
pub trait VDeviceBuilderOps: Send + Sync + 'static {
    async fn create_from(
        &self, mobile_name: String, camera_offer: Vec<CameraSdp>,
    ) -> Result<VDeviceMap>;
}

//caller to send SDP data as a publisher
//to all mobiles subscribed
pub struct MobileComm<Db, VDevBuilder> {
    db: Db,

    //virtual devices
    mobiles_connected: HashMap<Address, VDeviceMap>,

    //virtual device builder
    vdev_builder: VDevBuilder,
}

impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps>
    MobileComm<Db, VDevBuilder>
{
    pub fn new(db: Db, vdev_builder: VDevBuilder) -> Result<Self> {
        Ok(Self { db, mobiles_connected: HashMap::new(), vdev_builder })
    }
}

#[async_trait]
impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps> MultiMobileCommService
    for MobileComm<Db, VDevBuilder>
{
    //provisioning
    async fn get_host_info(&mut self, addr: Address) -> Result<HostProvInfo> {
        trace!("Host info requested by: {:?}", addr);

        //get the host info
        self.db.get_host_prov_info()
    }

    async fn register_mobile(
        &mut self, addr: Address, mobile: MobileSchema,
    ) -> Result<()> {
        trace!("Registering mobile: {:?}", addr);

        //add the mobile to the db
        self.db.add_mobile(&mobile)
    }

    //call establishment
    async fn set_mobile_sdp_offer(
        &mut self, addr: Address, mobile_offer: MobileSdpOffer,
    ) -> Result<()> {
        trace!("Mobile Pnp ID: {:?}", addr);

        let MobileSdpOffer { mobile_id, camera_offer } = mobile_offer;

        //check if the mobile is registered
        let mobile = self.db.get_mobile(&mobile_id)?;

        //create the virtual device
        self.mobiles_connected.insert(
            addr.clone(),
            self.vdev_builder.create_from(mobile.name, camera_offer).await?,
        );

        Ok(())
    }

    async fn sub_to_ready_answer(
        &mut self, addr: Address, publisher: BlePublisher,
    ) -> Result<()> {
        trace!("Subscribing to SDP call: {:?}", addr);
        Ok(())
    }

    async fn get_sdp_answer(&mut self, addr: Address) -> Result<String> {
        trace!("SDP offer requested by: {:?}", addr);

        //get the sdp offer
        Ok("SDP Offer".to_string())
    }

    //disconnect the mobile device
    async fn mobile_disconnected(&mut self, addr: Address) -> Result<()> {
        if let Some(_) = self.mobiles_connected.remove(&addr) {
            debug!(
                "Mobile: {:?} disconnected and removed from connected devices",
                addr
            );

            return Ok(());
        }

        Err(anyhow!("Mobile not found in connected devices"))
    }
}
