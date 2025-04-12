//! Serves a Bluetooth GATT application using the IO programming model.
use super::gatt_uuids::{CHAR_PROV_INFO_UUID, SERV_PROV_INFO_UUID};
use crate::ble::api::{CmdApi, QueryApi};
use crate::ble::requester::BleRequester;
use crate::error::Result;
use bluer::gatt::local::{characteristic_control, service_control, CharacteristicControlEvent};
use bluer::gatt::CharacteristicReader;
use bluer::{
    adv::Advertisement,
    gatt::local::{
        Application, Characteristic, CharacteristicRead, CharacteristicWrite,
        CharacteristicWriteMethod, Service,
    },
    Adapter,
};
use futures::{future, pin_mut, FutureExt, StreamExt};
use log::{error, info};
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot::{self, Receiver};

pub struct ProvisionerClient {
    _tx_drop: oneshot::Sender<()>,
}

impl ProvisionerClient {
    pub fn new(ble_adapter: Adapter, server_conn: BleRequester, host_name: String) -> Self {
        let (_tx_drop, _rx_drop) = oneshot::channel();

        tokio::spawn(async move {
            if let Err(e) = provisioner(ble_adapter, _rx_drop, server_conn, host_name).await {
                error!("Provisioner Client failed to start, error: {:?}", e);
            } else {
                info!("Provisioner Client stopped");
            }
        });

        Self { _tx_drop }
    }
}

pub async fn provisioner(
    adapter: Adapter, mut rx_drop: Receiver<()>, server_conn: BleRequester, host_name: String,
) -> Result<()> {
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

    let _adv_handle = adapter.advertise(le_advertisement).await?;

    info!("Serving Provisioner GATT service on Bluetooth adapter {}", adapter.name());

    let (_service_control, service_handle) = service_control();
    let (char_provisioner_control, char_provisioner_handle) = characteristic_control();

    let reader_server_requester = server_conn.clone();
    let app = Application {
        services: vec![Service {
            uuid: SERV_PROV_INFO_UUID,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: CHAR_PROV_INFO_UUID,
                read: Some(CharacteristicRead {
                    read: true,
                    fun: Box::new(move |req| {
                        let reader_server_requester = reader_server_requester.clone();
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
                    method: CharacteristicWriteMethod::Io,
                    ..Default::default()
                }),
                //write: Some(CharacteristicWrite {
                //    write: true,
                //    write_without_response: false,
                //    method: CharacteristicWriteMethod::Fun(Box::new(
                //        move |new_value, req| {
                //            let writer_server_requester =
                //                writer_server_requester.clone();
                //            async move {
                //                    match writer_server_requester
                //                        .cmd(
                //                            req.device_address.to_string(),
                //                            CmdApi::RegisterMobile,
                //                            new_value
                //                        )
                //                        .await
                //                        {
                //                            Ok(_) => {
                //                                info!("Mobile info registered");
                //                            }
                //                            Err(e) => {
                //                                error!(
                //                                    "Error registering mobile info, {:?}",
                //                                    e
                //                                );
                //                            }
                //                        }

                //                    Ok(()) //TODO do I need always to return OK?
                //                }
                //                .boxed()
                //        },
                //    )),
                //    ..Default::default()
                //}),
                control_handle: char_provisioner_handle,
                ..Default::default()
            }],
            control_handle: service_handle,
            ..Default::default()
        }],
        ..Default::default()
    };

    let _app_handle = adapter.serve_gatt_application(app).await?;

    let mut current_device_addr = String::new();

    let mut prov_read_buf = Vec::new();
    let mut prov_reader_opt: Option<CharacteristicReader> = None;

    pin_mut!(char_provisioner_control);

    loop {
        tokio::select! {
            evt = char_provisioner_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        info!("Accepting write event for provisioner with MTU {} from {}", req.mtu(), req.device_address());
                        prov_read_buf = vec![0; req.mtu()];
                        current_device_addr = req.device_address().to_string();
                        prov_reader_opt = Some(req.accept()?);
                    }
                    _ => {}
                }

            }
            _ = async {
                let read_res = match &mut prov_reader_opt {
                    Some(reader) => {
                        reader.read(&mut prov_read_buf).await
                    }
                    None => future::pending().await
                };

                match read_res {
                    Ok(0) => {
                        info!("Provisioner writing stream ended");
                        prov_reader_opt = None;
                    }
                    Ok(n) => {
                        if let Err(e) = server_conn.cmd(current_device_addr.clone(), CmdApi::RegisterMobile, prov_read_buf[..n].to_vec()).await {
                            error!("Error registering mobile info, {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error reading provisioner char, {:?}", e);
                        prov_reader_opt = None;
                    }
                }


            } => {}

            _ = &mut rx_drop => {
                break;
            }
        }
    }

    Ok(())
}
