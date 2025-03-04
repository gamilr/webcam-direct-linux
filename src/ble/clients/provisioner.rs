//! Serves a Bluetooth GATT application using the IO programming model.
use super::gatt_uuids::{CHAR_PROV_INFO_UUID, SERV_PROV_INFO_UUID};
use crate::ble::api::{CmdApi, QueryApi};
use crate::ble::requester::BleRequester;
use crate::error::Result;
use bluer::{
    adv::{Advertisement, AdvertisementHandle},
    gatt::local::{
        Application, ApplicationHandle, Characteristic, CharacteristicRead,
        CharacteristicWrite, CharacteristicWriteMethod, Service,
    },
    Adapter,
};
use futures::FutureExt;
use log::{error, info};
use tokio::sync::oneshot;

pub struct ProvisionerClient {
    _tx_drop: oneshot::Sender<()>,
}

impl ProvisionerClient {
    pub fn new(
        ble_adapter: Adapter, server_conn: BleRequester, host_name: String,
    ) -> Self {
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            if let Ok((_adv_handle, _app_handle)) =
                provisioner(ble_adapter, server_conn, host_name).await
            {
                info!("Provisioner started");

                let _ = rx.await;

                info!("Provisioner stopped");
            } else {
                error!("Provisioner failed to start");
            }
        });

        Self { _tx_drop: tx }
    }
}

pub async fn provisioner(
    adapter: Adapter, server_conn: BleRequester, host_name: String,
) -> Result<(AdvertisementHandle, ApplicationHandle)> {
    info!(
        "Advertising Provisioner on Bluetooth adapter {} with address {}",
        adapter.name(),
        adapter.address().await?
    );
    let le_advertisement = Advertisement {
        service_uuids: vec![SERV_PROV_INFO_UUID].into_iter().collect(),
        discoverable: Some(true),
        local_name: Some(host_name),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(le_advertisement).await?;

    info!("Serving GATT service on Bluetooth adapter {}", adapter.name());

    let reader_server_requester = server_conn.clone();
    let writer_server_requester = server_conn.clone();
    let app = Application {
        services: vec![Service {
            uuid: SERV_PROV_INFO_UUID,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: CHAR_PROV_INFO_UUID,
                read: Some(CharacteristicRead {
                    read: true,
                    fun: Box::new(move |req| {
                        let reader_server_requester =
                            reader_server_requester.clone();
                        async move {
                            match reader_server_requester
                                .query(
                                    req.device_address.to_string(),
                                    QueryApi::HostInfo,
                                    req.mtu as usize,
                                )
                                .await
                            {
                                Ok(data) => {
                                    return Ok(data);
                                }
                                Err(e) => {
                                    error!("Error reading host info, {:?}", e);
                                }
                            }

                            Ok(vec![])
                        }
                        .boxed()
                    }),
                    ..Default::default()
                }),
                write: Some(CharacteristicWrite {
                    write: true,
                    write_without_response: false,
                    method: CharacteristicWriteMethod::Fun(Box::new(
                        move |new_value, req| {
                            let writer_server_requester =
                                writer_server_requester.clone();
                            async move {
                                    match writer_server_requester
                                        .cmd(
                                            req.device_address.to_string(),
                                            CmdApi::RegisterMobile,
                                            new_value
                                        )
                                        .await
                                        {
                                            Ok(_) => {
                                                info!("Mobile info registered");
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Error registering mobile info, {:?}",
                                                    e
                                                );
                                            }
                                        }


                                    Ok(()) //TODO do I need always to return OK?
                                }
                                .boxed()
                        },
                    )),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;

    Ok((adv_handle, app_handle))
}
