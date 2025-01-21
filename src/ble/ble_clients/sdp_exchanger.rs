use crate::ble::ble_cmd_api::{
    BleComm, BleCmd, BleSub, PubSubSubscriber, PubSubTopic,
};
use crate::ble::ble_server::ServerConn;
use crate::error::Result;
use crate::gatt_const::{
    SDP_EXCHANGE_CHAR_UUID, SDP_NOTIFY_CHAR_UUID, WEBCAM_PNP_WRITE_CHAR_UUID,
};
use anyhow::anyhow;
use bluer::adv::Advertisement;
use bluer::gatt::local::{
    characteristic_control, service_control, Application, Characteristic,
    CharacteristicControlEvent, CharacteristicNotify,
    CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
    CharacteristicWriteMethod, ReqError, Service,
};
use bluer::gatt::{CharacteristicReader, CharacteristicWriter};
use bluer::Adapter;
use bluer::Uuid;
use futures::{future, pin_mut, FutureExt, StreamExt};
use log::{error, info};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot::{self, Receiver};

pub struct SdpExchangerClient {
    _tx_drop: oneshot::Sender<()>,
}

impl SdpExchangerClient {
    pub fn new(
        ble_adapter: Adapter, server_conn: ServerConn, host_name: String,
        host_id: String,
    ) -> Self {
        info!("Starting SdpExchangerClient");

        let (_tx_drop, _rx_drop) = oneshot::channel();
        tokio::spawn(async move {
            if let Err(e) = sdp_exchanger(
                ble_adapter,
                _rx_drop,
                server_conn,
                host_name,
                host_id,
            )
            .await
            {
                error!("SdpExchangerClient failed, error: {:?}", e);
            } else {
                info!("SdpExchanger started");
            }
        });

        Self { _tx_drop }
    }
}

async fn send_mobile_pnp_id(
    server_conn: ServerConn, device_address: String, new_value: Vec<u8>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();

    let cmd = BleComm::MobilePnpId(BleCmd {
        addr: device_address,
        payload: new_value,
        resp: tx,
    });

    server_conn.send(cmd).await.map_err(|e| {
        error!("Error sending mobile pnp id request {:?}", e);
        anyhow!("Error sending mobile pnp id request")
    })?;

    let resp = rx.await.map_err(|e| {
        error!("Error receiving mobile pnp id response, {:?}", e);
        anyhow!("Error receiving mobile pnp id response")
    })?;

    resp.map_err(|e| {
        error!("Error mobile is not registered, {:?}", e);
        anyhow!("Error mobile is not registered")
    })?;

    Ok(())
}

async fn send_subscriber(
    server_conn: ServerConn, device_address: String, buffer_len: usize,
) -> Result<PubSubSubscriber> {
    let (tx, rx) = oneshot::channel();

    //crate subscription request
    let ble_sub =
        BleSub { addr: device_address, max_buffer_len: buffer_len, resp: tx };

    server_conn
        .send(BleComm::Subscribe(PubSubTopic::SdpCall, ble_sub))
        .await
        .map_err(|_| {
            error!("Error sending sdp call sub request");
            anyhow!("Error sending sdp call sub request")
        })?;

    let resp = rx.await.map_err(|_| {
        error!("Error receiving sdp call sub response");
        anyhow!("Error receiving sdp call sub response")
    })?;

    let sub_recv = resp.map_err(|_| {
        error!("Error sdp call sub failed");
        anyhow!("Error sdp call sub failed")
    })?;

    Ok(sub_recv)
}

async fn sdp_exchanger(
    ble_adapter: Adapter, mut rx_drop: Receiver<()>, server_conn: ServerConn,
    host_name: String, host_id: String,
) -> Result<()> {
    info!(
        "Advertising Sdp Exchanger on Bluetooth adapter {} with address {}",
        ble_adapter.name(),
        ble_adapter.address().await?
    );
    let host_id = Uuid::parse_str(&host_id)?;
    let le_advertisement = Advertisement {
        service_uuids: vec![host_id].into_iter().collect(),
        discoverable: Some(true),
        local_name: Some(host_name),
        ..Default::default()
    };

    let _adv_handle = ble_adapter.advertise(le_advertisement).await?;

    info!("Serving GATT service on Bluetooth adapter {}", ble_adapter.name());

    let (_service_control, service_handle) = service_control();
    let (char_webcam_pnp_control, char_webcam_pnp_handle) =
        characteristic_control();
    let (char_sdp_exchange_control, char_sdp_exchange_handle) =
        characteristic_control();

    let app = Application {
        services: vec![Service {
            uuid: host_id,
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: SDP_EXCHANGE_CHAR_UUID,
                    write: Some(CharacteristicWrite {
                        write: true,
                        method: CharacteristicWriteMethod::Io,
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: char_sdp_exchange_handle,
                    ..Default::default()
                },
                Characteristic {
                    uuid: WEBCAM_PNP_WRITE_CHAR_UUID,
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: false,
                        method: CharacteristicWriteMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: char_webcam_pnp_handle,
                    ..Default::default()
                },
            ],
            control_handle: service_handle,
            ..Default::default()
        }],
        ..Default::default()
    };

    let _app_handle = ble_adapter.serve_gatt_application(app).await?;

    //current device address
    let mut current_device_addr = String::new();

    // Webcam pnp id write event
    let mut pnp_read_buf = Vec::new();
    let mut pnp_reader_opt: Option<CharacteristicReader> = None;

    //Webcam sdp exchange notify
    let mut notifier_opt: Option<CharacteristicWriter> = None;
    let mut sub_recv_opt: Option<PubSubSubscriber> = None;

    let mut sdp_read_buf = Vec::new();
    let mut sdp_reader_opt: Option<CharacteristicReader> = None;

    pin_mut!(char_webcam_pnp_control);
    pin_mut!(char_sdp_exchange_control);

    loop {
        tokio::select! {
            //webcam pnp id write event
            evt = char_webcam_pnp_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        info!("Accepting write event for PnP with MTU {} from {}", req.mtu(), req.device_address());
                        pnp_read_buf = vec![0; req.mtu()];
                        current_device_addr = req.device_address().to_string();
                        pnp_reader_opt = Some(req.accept()?);
                    },
                    _ => {
                        error!("Error accepting write event");
                    },
                }


            }

            _ = async {
                let read_res = match &mut pnp_reader_opt {
                    Some(reader) => reader.read(&mut pnp_read_buf).await,
                    None => future::pending().await,
                };

                match read_res {
                    Ok(0) => {
                        info!("Write stream ended");
                        pnp_reader_opt = None;
                    }
                    Ok(n) => {
                        if let Err(e) = send_mobile_pnp_id(
                            server_conn.clone(),
                            current_device_addr.clone(),
                            pnp_read_buf[0..n].to_vec(),
                        ).await {
                            error!("Failed to send mobile pnp id: {:?}", e);
                        }
                    }
                    Err(err) => {
                        info!("Write stream error: {}", &err);
                        pnp_reader_opt = None;
                    }
                }
            } => {
            }

            //sdp exchange write event
            evt = char_sdp_exchange_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        info!("Accepting write event for SDP Exchanger with MTU {} from {}", req.mtu(), req.device_address());
                        sdp_read_buf = vec![0; req.mtu()];
                        current_device_addr = req.device_address().to_string();
                        sdp_reader_opt = Some(req.accept()?);
                    },

                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        info!("Accepting notify request event with MTU {} from {}", notifier.mtu(), notifier.device_address());

                        match send_subscriber(
                            server_conn.clone(),
                            notifier.device_address().to_string(),
                            notifier.mtu(),
                        ).await {
                            Ok(sub_recv) => {
                                sub_recv_opt = Some(sub_recv);
                                notifier_opt = Some(notifier);
                            },
                            Err(e) => {
                                error!("Failed to send sdp notify: {:?}", e);
                            }
                        }
                    },
                    _ => {
                        error!("Error accepting notify event");
                    },
                }
            }

            _ = async {
                let read_res = match &mut sdp_reader_opt {
                    Some(reader) => reader.read(&mut sdp_read_buf).await,
                    None => future::pending().await,
                };

                match read_res {
                    Ok(0) => {
                        info!("Write stream ended");
                        sdp_reader_opt = None;
                    }
                    Ok(n) => {
                        //todo
                        info!("Received SDP data: {:?}", &sdp_read_buf[0..n]);
                    }
                    Err(err) => {
                        info!("Write stream error: {}", &err);
                        sdp_reader_opt = None;
                    }
                }
            } => {
            }

            //receive data from server
            pub_data = async {
                match &mut sub_recv_opt {
                    Some(pub_recv) => pub_recv.recv().await,
                    None => future::pending().await,
                }
            } => {
                match pub_data {
                    Ok(data) => {
                        info!("Received data from server: {:?}", data);

                        if let Some(notifier) = notifier_opt.as_mut() {
                            if let Err(e) = notifier.write(&data).await {
                                error!("Failed to write notify: {:?}", e);
                                notifier_opt = None;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving data from server: {:?}", e);
                    }
                }
            }

            _ = &mut rx_drop => {
                info!("SdpExchangerClient stopped");
                break;
            }

        }
    }

    Ok(())
}
