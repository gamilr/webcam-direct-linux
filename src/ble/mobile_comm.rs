use crate::app_data::MobileSchema;
use base64::prelude::*;
use std::collections::HashMap;

use async_trait::async_trait;
use log::{debug, error, info, trace};

use anyhow::anyhow;

use super::{
    ble_cmd_api::Address,
    ble_requester::BlePublisher,
    ble_server::{HostProvInfo, MultiMobileCommService},
    mobile_sdp_types::{CameraSdp, MobileSdpAnswer, MobileSdpOffer, VideoProp},
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

pub type VDeviceMap = HashMap<String, VDevice>;

#[derive(Default)]
pub struct VDeviceInfo {
    publisher: Option<BlePublisher>,
    vdevices: VDeviceMap,
    sdp_answer_cache: Option<String>,
}

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
    mobiles_connected: HashMap<Address, VDeviceInfo>,

    //virtual device builder
    vdev_builder: VDevBuilder,

    //host cache
    host_prov_info_cache: Option<String>,
}

impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps>
    MobileComm<Db, VDevBuilder>
{
    pub fn new(db: Db, vdev_builder: VDevBuilder) -> Result<Self> {
        Ok(Self {
            db,
            mobiles_connected: HashMap::new(),
            vdev_builder,
            host_prov_info_cache: None,
        })
    }
}

#[async_trait]
impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps> MultiMobileCommService
    for MobileComm<Db, VDevBuilder>
{
    //provisioning
    async fn get_host_info<'a>(
        &'a mut self, addr: Address,
    ) -> Result<&'a String> {
        debug!("Host info requested by: {:?}", addr);

        //check if the host info is already cached
        if let None = self.host_prov_info_cache {
            let host_info = self.db.get_host_prov_info()?;

            let host_info_b64 =
                BASE64_STANDARD.encode(serde_json::to_vec(&host_info)?);

            self.host_prov_info_cache = Some(host_info_b64);
        }

        //get the host info
        Ok(self.host_prov_info_cache.as_ref().ok_or_else(|| {
            anyhow!("Host info not found in connected devices")
        })?)
    }

    async fn register_mobile(
        &mut self, addr: Address, mobile: MobileSchema,
    ) -> Result<()> {
        debug!("Registering mobile: {:?}", addr);

        //add the mobile to the db
        self.db.add_mobile(&mobile)
    }

    //call establishment
    async fn sub_to_ready_answer(
        &mut self, addr: Address, publisher: BlePublisher,
    ) -> Result<()> {
        debug!("Subscribing to SDP call: {:?}", addr);

        //add the publisher to for this mobile
        self.mobiles_connected.insert(
            addr,
            VDeviceInfo {
                publisher: Some(publisher),
                vdevices: HashMap::new(),
                sdp_answer_cache: None,
            },
        );

        Ok(())
    }

    //set the SDP offer from the mobile
    async fn set_mobile_sdp_offer(
        &mut self, addr: Address, mobile_offer: MobileSdpOffer,
    ) -> Result<()> {
        debug!("Mobile Pnp ID: {:?}", addr);

        let MobileSdpOffer { mobile_id, camera_offer } = mobile_offer;

        //check if the mobile is registered
        let mobile = self.db.get_mobile(&mobile_id)?;

        if let Some(vdevice_info) = self.mobiles_connected.get_mut(&addr) {
            if let Some(publisher) = &vdevice_info.publisher {
                //create the virtual devices
                vdevice_info.vdevices = self
                    .vdev_builder
                    .create_from(mobile.name, camera_offer)
                    .await?;

                //notify the mobile the SDP answer are ready
                publisher.publish(addr.to_string().into()).await?;
            } else {
                return Err(anyhow!("Publisher not found for mobile"));
            }
        } else {
            return Err(anyhow!("Mobile not found in connected devices"));
        }

        Ok(())
    }

    async fn get_sdp_answer<'a>(
        &'a mut self, addr: Address,
    ) -> Result<&'a String> {
        debug!("SDP answer requested by: {:?}", addr);

        let vdevice_info = self
            .mobiles_connected
            .get_mut(&addr)
            .ok_or_else(|| anyhow!("Mobile not found in connected devices"))?;

        //check if the SDP answer is already cached
        if let None = vdevice_info.sdp_answer_cache {
            let sdp_answer = vdevice_info
                .vdevices
                .iter()
                .map(|(name, vdevice)| CameraSdp {
                    name: name.clone(),
                    format: VideoProp::default(),
                    sdp: vdevice.get_sdp_answer().clone(),
                })
                .collect::<Vec<CameraSdp>>();

            let sdp_answer_b64 =
                BASE64_STANDARD.encode(serde_json::to_vec(&sdp_answer)?);

            debug!(
                "encoded SDP answer: {} with len {}",
                sdp_answer_b64,
                sdp_answer_b64.len(),
            );

            vdevice_info.sdp_answer_cache = Some(sdp_answer_b64);

            debug!("SDP answer cached for mobile: {:?}", addr);
        }

        let sdp_answer =
            vdevice_info.sdp_answer_cache.as_ref().ok_or_else(|| {
                anyhow!("SDP answer not found in connected devices")
            })?;

        info!("SDP answer {:?} for mobile {:?} is ready", sdp_answer, addr);

        Ok(sdp_answer)
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
