//Under this module the comm layers are mixed
//It doesn't need to have different comm module for this simple application
//We can add a transaction Id , which is similar to TCP/UDP port to identify the transaction
//from the device and allow fully parallel communication from the same device even in the same api.
//
//The server Api is like a server port in this context.
//Similar to OSI model we need to implement the tansport layer independent of the application layer

pub mod ble_clients;
pub mod ble_cmd_api;
pub mod ble_requester;
pub mod ble_server;
pub mod buffer_process;
pub mod mobile_buffer;
pub mod mobile_comm;
pub mod mobile_sdp_types;

pub use mobile_comm::{
    AppDataStore, MobileComm, VDeviceBuilderOps, VDeviceMap,
};
