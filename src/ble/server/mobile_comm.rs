use crate::{
    app_data::MobileSchema,
    ble::comm_types::{MobileSdpAnswer, SdpAnswerReady},
};
use std::collections::HashMap;

use async_trait::async_trait;
use log::debug;

use anyhow::anyhow;

use crate::ble::{
    api::Address,
    comm_types::{CameraSdp, HostProvInfo, MobileSdpOffer, VideoProp},
    requester::BlePublisher,
    server::CommDataService,
};
use crate::error::Result;
use crate::vdevice_builder::VDevice;

#[cfg(test)]
use mockall::automock;

/// A trait that defines the operations for interacting with the application's data store.
#[cfg_attr(test, automock)]
pub trait AppDataStore: Send + Sync + 'static {
    fn get_host_prov_info(&self) -> Result<HostProvInfo>;

    fn add_mobile(&mut self, mobile: &MobileSchema) -> Result<()>;

    fn get_mobile(&self, id: &str) -> Result<MobileSchema>;
}

pub type VDeviceMap = HashMap<String, VDevice>;

#[derive(Default)]
pub struct DeviceInfo {
    publisher: Option<BlePublisher>,
    vdevices: VDeviceMap,
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
    mobiles_connected: HashMap<Address, DeviceInfo>,

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
impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps> CommDataService
    for MobileComm<Db, VDevBuilder>
{
    //provisioning
    async fn get_host_info(&mut self, addr: Address) -> Result<HostProvInfo> {
        debug!("Host info requested by: {:?}", addr);

        self.db.get_host_prov_info()
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
            DeviceInfo { publisher: Some(publisher), vdevices: HashMap::new() },
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
                publisher
                    .publish(SdpAnswerReady { mobile_id }.try_into()?)
                    .await?;
            } else {
                return Err(anyhow!("Publisher not found for mobile"));
            }
        } else {
            return Err(anyhow!("Mobile not found in connected devices"));
        }

        Ok(())
    }

    async fn get_sdp_answer(
        &mut self, addr: Address,
    ) -> Result<MobileSdpAnswer> {
        debug!("SDP answer requested by: {:?}", addr);

        let vdevice_info = self
            .mobiles_connected
            .get_mut(&addr)
            .ok_or_else(|| anyhow!("Mobile not found in connected devices"))?;

        let camera_answer = vdevice_info
            .vdevices
            .iter()
            .map(|(name, vdevice)| CameraSdp {
                name: name.clone(),
                format: VideoProp::default(),
                sdp: vdevice.get_sdp_answer().clone(),
            })
            .collect::<Vec<CameraSdp>>();

        Ok(MobileSdpAnswer { camera_answer })
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
