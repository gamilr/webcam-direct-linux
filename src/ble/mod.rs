pub mod ble_clients;
pub mod ble_cmd_api;
pub mod ble_requester;
pub mod ble_server;
pub mod mobile_buffer;
pub mod mobile_comm;

pub use mobile_comm::{
    AppDataStore, MobileComm, VDeviceBuilderOps, VDeviceMap,
};
