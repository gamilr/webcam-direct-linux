use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use log::{error, info, trace};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::{
    ble_cmd_api::{Address, DataChunk, PubSubPublisher, PubSubSubscriber},
    ble_server::MultiMobileCommService,
};
use crate::vdevice_builder::VDevice;
use crate::{app_data::MobileSchema, error::Result};

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BufferComm {
    pub remain_len: usize,
    pub payload: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HostProvInfo {
    pub id: String,
    pub name: String,
    pub connection_type: String,
}

/*
 * This represent the json
 * {
 *  "id": "host_id",
 *  "name": "host_name",
 *  "connection_type": "WLAN"
 * }
 * */

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
    async fn create_from(&self, mobile: MobileSchema) -> Result<VDeviceMap>;
}
//States:
//Provisioning:  Connected -> ReadHostInfo->WriteMobileInfo->WriteMobileId->ReadyToStream
//Identification: Connected -> WriteMobileId->ReadyToStream
//
#[derive(Debug)]
enum MobileDataState {
    MobileConnected,

    ReadingHostInfo,

    WriteMobileInfo,

    WriteMobileId,

    SaveMobileData { mobile: MobileSchema },

    ReadyToStream { virtual_devices: VDeviceMap },
}

//State for the communication buffer
enum CommBufferStatus {
    RemainLen(usize),      //used in queries
    CurrentBuffer(String), //used in commands
}

struct ConnectedMobileData {
    pub mobile_state: MobileDataState,
    pub buffer_status: Option<CommBufferStatus>,
}

//caller to send SDP data as a publisher
//to all mobiles subscribed
struct MobileSdpCaller {
    pub max_buffer_len: usize,
    pub publisher: PubSubPublisher,
}

pub struct MobileComm<Db, VDevBuilder> {
    db: Db,
    mobiles_connected: HashMap<Address, ConnectedMobileData>,
    //index to get the mobile address from virtual device path
    vdevice_index: HashMap<PathBuf, Address>,

    host_info: String,
    sdp_caller: MobileSdpCaller,
    vdev_builder: VDevBuilder,
}

impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps>
    MobileComm<Db, VDevBuilder>
{
    pub fn new(db: Db, vdev_builder: VDevBuilder) -> Result<Self> {
        let host = db.get_host_prov_info()?;
        let host_info = serde_json::to_string(&host)?;

        let (sender, _) = broadcast::channel(16);
        let sdp_caller = MobileSdpCaller {
            max_buffer_len: 19, //default mtu size 23 - 4 bytes for header
            publisher: sender,
        };

        Ok(Self {
            db,
            mobiles_connected: HashMap::new(),
            vdevice_index: HashMap::new(),
            host_info,
            sdp_caller,
            vdev_builder,
        })
    }
}

//TODO: split the data chunk handling and the Mobile state machine logic
#[async_trait]
impl<Db: AppDataStore, VDevBuilder: VDeviceBuilderOps> MultiMobileCommService
    for MobileComm<Db, VDevBuilder>
{
    async fn mobile_disconnected(&mut self, addr: Address) -> Result<()> {
        if let Some(connected_data) = self.mobiles_connected.remove(&addr) {
            if let MobileDataState::ReadyToStream { virtual_devices } =
                connected_data.mobile_state
            {
                //remove the virtual devices
                for (path, _) in virtual_devices {
                    info!("Removing index with path {:?}", path);
                    if self.vdevice_index.remove(&path).is_none() {
                        error!("Device not found in vdevice index {:?}", path);
                    }
                }
            }

            info!(
                "Mobile: {:?} disconnected and removed from connected devices",
                addr
            );
        } else {
            error!("Mobile not found in connected devices");
            return Err(anyhow!("Mobile not found"));
        }
        Ok(())
    }

    async fn get_host_info(
        &mut self, addr: Address, max_buffer_len: usize,
    ) -> Result<DataChunk> {
        info!("Host info requested by: {:?}", addr);

        let total_len = self.host_info.len();

        //if mobile is not connected, add it with the state ReadHostInfo
        //start condition
        if !self.mobiles_connected.contains_key(&addr) {
            self.mobiles_connected.insert(
                addr.clone(),
                ConnectedMobileData {
                    mobile_state: MobileDataState::ReadingHostInfo,
                    buffer_status: Some(CommBufferStatus::RemainLen(total_len)),
                },
            );
        }

        if let ConnectedMobileData {
            mobile_state: MobileDataState::ReadingHostInfo,
            buffer_status: Some(CommBufferStatus::RemainLen(remain)),
        } = self
            .mobiles_connected
            .get_mut(&addr)
            .ok_or_else(|| anyhow!("Mobile not found in connected devices"))?
        {
            let initial_len = total_len - *remain;

            let ble_comm = if max_buffer_len >= *remain {
                *remain = total_len;
                //move to next state
                self.mobiles_connected.insert(
                    addr.clone(),
                    ConnectedMobileData {
                        mobile_state: MobileDataState::WriteMobileInfo,
                        buffer_status: Some(CommBufferStatus::CurrentBuffer(
                            "".to_string(),
                        )),
                    },
                );
                info!("Mobile: {:?} in state WriteMobileInfo", addr);

                BufferComm {
                    remain_len: 0,
                    payload: self.host_info[initial_len..total_len].to_owned(),
                }
            } else {
                *remain -= max_buffer_len;
                BufferComm {
                    remain_len: *remain,
                    payload: self.host_info
                        [initial_len..initial_len + max_buffer_len]
                        .to_owned(),
                }
            };

            info!("Sending host info: {:?}", ble_comm);

            return Ok(serde_json::to_vec(&ble_comm)?);
        }

        error!("Mobile is not reading host info");
        Err(anyhow!("Mobile is not reading host info"))
    }

    async fn set_register_mobile(
        &mut self, addr: Address, data: DataChunk,
    ) -> Result<()> {
        info!("Registering mobile: {:?}", addr);

        if let ConnectedMobileData {
            mobile_state: MobileDataState::WriteMobileInfo,
            buffer_status: Some(CommBufferStatus::CurrentBuffer(current_buffer)),
        } = self
            .mobiles_connected
            .get_mut(&addr)
            .ok_or_else(|| anyhow!("Mobile not found in connected devices"))?
        {
            let buff_comm = serde_json::from_slice::<BufferComm>(&data)?;

            info!("buff_comm {:?}", buff_comm);

            current_buffer.push_str(&buff_comm.payload);

            info!("current_buffer {:?}", buff_comm);

            if buff_comm.remain_len == 0 {
                let mobile = serde_json::from_str(&current_buffer)?;
                self.db.add_mobile(&mobile)?;
                info!("Mobile registered: {:?}", mobile);
                //move to next state
                self.mobiles_connected.insert(
                    addr.clone(),
                    ConnectedMobileData {
                        mobile_state: MobileDataState::WriteMobileId,
                        buffer_status: Some(CommBufferStatus::CurrentBuffer(
                            "".to_string(),
                        )),
                    },
                );
                info!("Mobile: {:#?} in state WriteMobileId", mobile);
            }
        } else {
            error!("Mobile is not writing mobile info");
            return Err(anyhow!("Mobile is not writing mobile info"));
        }

        Ok(())
    }

    async fn set_mobile_pnp_id(
        &mut self, addr: Address, data: DataChunk,
    ) -> Result<()> {
        info!("Mobile Pnp ID: {:?}", addr);

        if !self.mobiles_connected.contains_key(&addr) {
            //new connection, already registered
            self.mobiles_connected.insert(
                addr.clone(),
                ConnectedMobileData {
                    mobile_state: MobileDataState::WriteMobileId,
                    buffer_status: Some(CommBufferStatus::CurrentBuffer(
                        "".to_string(),
                    )),
                },
            );
        }

        if let ConnectedMobileData {
            mobile_state: MobileDataState::WriteMobileId,
            buffer_status: Some(CommBufferStatus::CurrentBuffer(current_buffer)),
        } = self
            .mobiles_connected
            .get_mut(&addr)
            .ok_or_else(|| anyhow!("Mobile not found in connected devices"))?
        {
            let buff_comm = serde_json::from_slice::<BufferComm>(&data)?;

            info!("buff_comm {:?}", buff_comm);

            current_buffer.push_str(&buff_comm.payload);

            info!("current_buffer {:?}", buff_comm);

            if buff_comm.remain_len == 0 {
                let mobile_id = current_buffer.clone();
                if let Ok(mobile) = self.db.get_mobile(&mobile_id) {
                    info!("Mobile: {:#?} found", mobile);
                    //move to next State
                    self.mobiles_connected.insert(
                        addr.clone(),
                        ConnectedMobileData {
                            mobile_state: MobileDataState::SaveMobileData {
                                mobile,
                            },
                            buffer_status: None,
                        },
                    );
                } else {
                    error!("Mobile with id: {current_buffer} not found");
                    return Err(anyhow!("Mobile not found"));
                }
            }
        } else {
            error!("Mobile is not writing mobile id");
            return Err(anyhow!("Mobile is not writing mobile id"));
        }

        Ok(())
    }

    async fn subscribe_to_sdp_req(
        &mut self, addr: String, max_size: usize,
    ) -> Result<PubSubSubscriber> {
        info!("Subscribe to SDP call: {:?}", addr);
        //get the virtual device
        let vdev_map = if let Some(ConnectedMobileData {
            mobile_state: MobileDataState::SaveMobileData { mobile },
            buffer_status: None,
        }) = self.mobiles_connected.get(&addr)
        {
            self.vdev_builder.create_from(mobile.clone()).await?
        } else {
            error!("Mobile not found in connected devices or in wrong state");
            return Err(anyhow!(
                "Mobile not found in connected devices or in wrong state"
            ));
        };

        if let Some(ConnectedMobileData {
            mobile_state: MobileDataState::SaveMobileData { mobile },
            buffer_status: None,
        }) = self.mobiles_connected.remove(&addr)
        {
            info!("Mobile: {:#?} is subscribe to SDP call", mobile);

            //update the index
            for (path, _) in &vdev_map {
                self.vdevice_index.insert(path.clone(), addr.clone());
            }

            //move to next state
            self.mobiles_connected.insert(
                addr.clone(),
                ConnectedMobileData {
                    mobile_state: MobileDataState::ReadyToStream {
                        virtual_devices: vdev_map,
                    },
                    buffer_status: Some(CommBufferStatus::CurrentBuffer(
                        "".to_string(),
                    )),
                },
            );

            //update the max buffer len
            self.sdp_caller.max_buffer_len = max_size;
        } else {
            return Err(anyhow!(
                "Mobile not ready is not ready to start streaming"
            ));
        }

        Ok(self.sdp_caller.publisher.subscribe())
    }

    async fn set_mobile_sdp_resp(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()> {
        if let ConnectedMobileData {
            mobile_state: MobileDataState::ReadyToStream { .. },
            buffer_status: Some(CommBufferStatus::CurrentBuffer(current_buffer)),
        } = self
            .mobiles_connected
            .get_mut(&addr)
            .ok_or_else(|| anyhow!("Mobile not found in connected devices"))?
        {
            let buff_comm = serde_json::from_slice::<BufferComm>(&data)?;

            info!("buff_comm {:?}", buff_comm);

            current_buffer.push_str(&buff_comm.payload);

            info!("current_buffer {:?}", buff_comm);

            if buff_comm.remain_len == 0 {
                info!("SDP data: {:?}", current_buffer);

                //parse the sdp data and use it some how
                //ex: virtual_device.send_sdp_data(&data)?;
            }
        } else {
            return Err(anyhow!("Mobile is not ready to stream"));
        }

        Ok(())
    }
}
