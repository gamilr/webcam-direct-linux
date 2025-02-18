use super::gatt_const::CHAR_PNP_EXCHANGE_SDP_UUID;
use crate::ble::ble_cmd_api::{CmdApi, PubSubTopic, QueryApi};
use crate::ble::ble_requester::{BleRequester, BleSubscriber};
use crate::error::Result;
use bluer::adv::Advertisement;
use bluer::gatt::local::{
    characteristic_control, service_control, Application, Characteristic,
    CharacteristicControlEvent, CharacteristicNotify,
    CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
    CharacteristicWriteMethod, Service,
};

use bluer::gatt::{CharacteristicReader, CharacteristicWriter};
use bluer::Adapter;
use bluer::Uuid;
use futures::FutureExt;
use futures::{future, pin_mut, StreamExt};
use log::{debug, error, info};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot::{self, Receiver};

pub struct SdpExchangerClient {
    _tx_drop: oneshot::Sender<()>,
}

impl SdpExchangerClient {
    pub fn new(
        ble_adapter: Adapter, server_conn: BleRequester, host_name: String,
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

async fn sdp_exchanger(
    ble_adapter: Adapter, mut rx_drop: Receiver<()>, server_conn: BleRequester,
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
    let (char_pnp_exchange_control, char_pnp_exchange_handle) =
        characteristic_control();

    let reader_server_requester = server_conn.clone();

    let app = Application {
        services: vec![Service {
            uuid: host_id,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: CHAR_PNP_EXCHANGE_SDP_UUID,
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
                read: Some(CharacteristicRead {
                    read: true,
                    fun: Box::new(move |req| {
                        let reader_server_requester =
                            reader_server_requester.clone();
                        async move {
                            match reader_server_requester
                                .query(
                                    req.device_address.to_string(),
                                    QueryApi::SdpAnswer,
                                    req.mtu as usize,
                                )
                                .await
                            {
                                Ok(data) => {
                                    return Ok(data);
                                }
                                Err(e) => {
                                    error!("Error reading sdp answer, {:?}", e);
                                }
                            }

                            Ok(vec![])
                        }
                        .boxed()
                    }),
                    ..Default::default()
                }),
                control_handle: char_pnp_exchange_handle,
                ..Default::default()
            }],
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
    let mut sub_recv_opt: Option<BleSubscriber> = None;

    pin_mut!(char_pnp_exchange_control);

    loop {
        tokio::select! {
            evt = char_pnp_exchange_control.next() => {
                match evt {
                    //write sdp offer
                    Some(CharacteristicControlEvent::Write(req)) => {
                        info!("Accepting write event for pnp with MTU {} from {}", req.mtu(), req.device_address());
                        pnp_read_buf = vec![0; req.mtu()];
                        current_device_addr = req.device_address().to_string();
                        pnp_reader_opt = Some(req.accept()?);
                    },

                    //notify sdp answer
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        info!("Accepting notify request event with MTU {} from {}", notifier.mtu(), notifier.device_address());

                        match server_conn.subscribe(
                            notifier.device_address().to_string(),
                            PubSubTopic::SdpAnswerReady,
                            notifier.mtu(),
                        ).await {
                            Ok(subscriber) => {
                                if notifier_opt.is_some() && sub_recv_opt.is_some() {
                                    debug!("Already have a notifier");
                                    continue;
                                }

                                notifier_opt = Some(notifier);
                                sub_recv_opt = Some(subscriber);
                            },
                            Err(e) => {
                                error!("Failed to subscribe to sdp call: {:?}", e);
                            }
                        }
                    },
                    _ => {
                        error!("Error accepting notify event");
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
                        if let Err(e) = server_conn.cmd(
                            current_device_addr.clone(),
                            CmdApi::SdpOffer,
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
            } => {}

            //receive data from server
            _ = async {
                let sub_data = match &mut sub_recv_opt {
                    Some(pub_recv) => pub_recv.get_data().await,
                    None => future::pending().await,
                };

                match sub_data {
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

            } => {
            }
            _ = &mut rx_drop => {
                info!("SdpExchangerClient stopped");
                break;
            }

        }
    }

    Ok(())
}
