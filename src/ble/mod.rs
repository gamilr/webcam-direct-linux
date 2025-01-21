pub mod ble_clients;
mod ble_cmd_api;
pub mod ble_server;
mod mobile_buffer;
mod mobile_comm;

pub use mobile_comm::{
    AppDataStore, HostProvInfo, MobileComm, VDeviceBuilderOps, VDeviceMap,
};
