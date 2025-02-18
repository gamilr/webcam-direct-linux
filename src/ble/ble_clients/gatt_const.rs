use bluer::Uuid;

// Constants for the BLE Provisioning Service
pub const SERV_PROV_INFO_UUID: Uuid =
    Uuid::from_u128(0x124ddac5b10746a0ade04ae8b2b700f5); //service
pub const CHAR_PROV_INFO_UUID: Uuid =
    Uuid::from_u128(0x124ddac6b10746a0ade04ae8b2b700f5); //characteristic to read host info

//Webrtc SDP offer and answer
// The service for this characteristic will be the same host Id
// that way I can filter out for only that host from the mobiles
pub const CHAR_PNP_EXCHANGE_SDP_UUID: Uuid =
    Uuid::from_u128(0x124ddacab10746a0ade04ae8b2b700f5);
