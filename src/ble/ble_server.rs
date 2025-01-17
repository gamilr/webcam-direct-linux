use std::collections::HashMap;

use async_trait::async_trait;
use log::{error, info};
use tokio::sync::{mpsc, oneshot};

use crate::error::Result;

#[cfg(test)]
use mockall::automock;

use super::ble_cmd_api::{Address, BleApi, DataChunk, PubSubSubscriber, PubSubTopic};


//trait
#[cfg_attr(test, automock)]
#[async_trait]
pub trait MultiMobileCommService: Send + Sync + 'static {
    async fn set_register_mobile(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()>;

    async fn read_host_info(
        &mut self, addr: String, max_size: usize,
    ) -> Result<DataChunk>;

    async fn device_disconnected(&mut self, addr: String) -> Result<()>;

    async fn set_mobile_pnp_id(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()>;

    async fn subscribe_to_sdp_req(
        &mut self, addr: String, max_size: usize,
    ) -> Result<PubSubSubscriber>;

    async fn set_mobile_sdp_resp(
        &mut self, addr: String, data: DataChunk,
    ) -> Result<()>;
}

//Chunk data handler
enum DataBufferCursor{
    RemainLen(usize),      //used in queries
    CurrentBuffer(String), //used in commands
}

type ClientBufferCursor = HashMap<Address, DataBufferCursor>;

pub type ServerConn = mpsc::Sender<BleApi>;
pub struct BleServer {
    ble_tx: ServerConn,
    _drop_tx: oneshot::Sender<()>,
}

impl BleServer {
    pub fn new(
        mut comm_handler: impl MultiMobileCommService, req_buffer_size: usize,
    ) -> Self {
        let (ble_tx, mut ble_rx) = mpsc::channel(req_buffer_size);

        let (_drop_tx, mut _drop_rx) = oneshot::channel();

        tokio::spawn(async move {

            let mut client_buffer_cursor = ClientBufferCursor::new();

            loop {
                tokio::select! {
                    _ = async {
                         if let Some(req) = ble_rx.recv().await {
                            handle_request(&mut comm_handler, req).await;
                         }
                    }  => {}

                    _ = &mut _drop_rx => {
                        info!("Ble Server task is stopping");
                        break;
                    }
                }
            }
        });

        Self { ble_tx, _drop_tx }
    }

    pub fn connection(&self) -> ServerConn {
        self.ble_tx.clone()
    }
}

//This function does not return a Result since every request is successful
//if internally any operation fails, it should handle it accordingly
async fn handle_request(
    comm_handler: &mut impl MultiMobileCommService, req: BleApi,
) {
    match req {
        BleApi::HostInfo(query) => {
            if let Err(e) = query.resp.send(
                comm_handler
                    .read_host_info(query.addr, query.max_buffer_len)
                    .await,
            ) {
                error!("Error sending host info: {:?}", e);
            }
        }

        BleApi::MobileDisconnected(cmd) => {
            info!("Mobile disconnected: {:?}", cmd.addr);
            if let Err(e) =
                cmd.resp.send(comm_handler.device_disconnected(cmd.addr).await)
            {
                error!("Error disconnecting mobile: {:?}", e);
            }
        }

        BleApi::RegisterMobile(cmd) => {
            if let Err(e) = cmd.resp.send(
                comm_handler.set_register_mobile(cmd.addr, cmd.payload).await,
            ) {
                error!(
                    "Error sending mobile registration response error: {:?}",
                    e
                );
            }
        }

        BleApi::MobilePnpId(cmd) => {
            if let Err(e) = cmd.resp.send(
                comm_handler.set_mobile_pnp_id(cmd.addr, cmd.payload).await,
            ) {
                error!("Error setting mobile Pnp Id error: {:?}", e);
            }
        }

        BleApi::MobileSdpResponse(cmd) => {
            if let Err(e) = cmd.resp.send(
                comm_handler.set_mobile_sdp_resp(cmd.addr, cmd.payload).await,
            ) {
                error!("Error setting mobile sdp response error: {:?}", e);
            }
        }

        BleApi::Subscribe(topic, sub) => {
            //initialize the topic with the first subscriber
            match topic {
                PubSubTopic::SdpCall => {
                    //process subscribe
                    if let Err(e) = sub.resp.send(
                        comm_handler
                            .subscribe_to_sdp_req(sub.addr, sub.max_buffer_len)
                            .await,
                    ) {
                        error!(
                            "Error sending sdp call sub response, error: {:?}",
                            e
                        );
                    }
                }
            }
        }
        _ => {
            error!("Not handle request: {:?}", req);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble::ble_cmd_api::{
        BleApi, BleCmd, BleQuery, PubSubSubscriber, PubSubTopic,
    };
    use mockall::mock;
    use mockall::predicate::eq;

    mock! {
        CommHandler {}
        #[async_trait]
        impl MultiMobileCommService for CommHandler {
            async fn read_host_info(&mut self, addr: String, max_size: usize) -> Result<DataChunk>;
            async fn set_register_mobile(&mut self, addr: String, data: DataChunk) -> Result<()>;
            async fn device_disconnected(&mut self, addr: String) -> Result<()>;
            async fn set_mobile_pnp_id(&mut self, addr: String, data: DataChunk) -> Result<()>;
            async fn subscribe_to_sdp_req(&mut self, addr: String, max_size: usize) -> Result<PubSubSubscriber>;
            async fn set_mobile_sdp_resp(&mut self, addr: String, data: DataChunk) -> Result<()>;
        }
    }

    fn init_logger() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[tokio::test]
    async fn test_ble_server_host_info() {
        init_logger();
        let mut mock_handler = MockCommHandler::new();
        mock_handler
            .expect_read_host_info()
            .with(eq("test_addr".to_string()), eq(1024))
            .returning(|_, _| Ok(vec![1, 2, 3]));

        let server = BleServer::new(mock_handler, 10);
        let conn = server.connection();

        let (resp_tx, resp_rx) = oneshot::channel();
        let query = BleQuery {
            addr: "test_addr".to_string(),
            max_buffer_len: 1024,
            resp: resp_tx,
        };

        conn.send(BleApi::HostInfo(query)).await.unwrap();
        let result = resp_rx.await.unwrap();
        assert_eq!(result.unwrap(), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_ble_server_register_mobile() {
        init_logger();
        let mut mock_handler = MockCommHandler::new();
        mock_handler
            .expect_set_register_mobile()
            .with(eq("test_addr".to_string()), eq(vec![1, 2, 3]))
            .returning(|_, _| Ok(()));

        let server = BleServer::new(mock_handler, 10);
        let conn = server.connection();

        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = BleCmd {
            addr: "test_addr".to_string(),
            payload: vec![1, 2, 3],
            resp: resp_tx,
        };

        conn.send(BleApi::RegisterMobile(cmd)).await.unwrap();
        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ble_server_device_disconnected() {
        init_logger();
        let mut mock_handler = MockCommHandler::new();
        mock_handler
            .expect_device_disconnected()
            .with(eq("test_addr".to_string()))
            .returning(|_| Ok(()));

        let server = BleServer::new(mock_handler, 10);
        let conn = server.connection();

        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = BleCmd {
            addr: "test_addr".to_string(),
            payload: vec![],
            resp: resp_tx,
        };

        conn.send(BleApi::MobileDisconnected(cmd)).await.unwrap();
        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ble_server_mobile_pnp_id() {
        init_logger();
        let mut mock_handler = MockCommHandler::new();
        mock_handler
            .expect_set_mobile_pnp_id()
            .with(eq("test_addr".to_string()), eq(vec![1, 2, 3]))
            .returning(|_, _| Ok(()));

        let server = BleServer::new(mock_handler, 10);
        let conn = server.connection();

        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = BleCmd {
            addr: "test_addr".to_string(),
            payload: vec![1, 2, 3],
            resp: resp_tx,
        };

        conn.send(BleApi::MobilePnpId(cmd)).await.unwrap();
        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ble_server_mobile_sdp_response() {
        init_logger();
        let mut mock_handler = MockCommHandler::new();
        mock_handler
            .expect_set_mobile_sdp_resp()
            .with(eq("test_addr".to_string()), eq(vec![1, 2, 3]))
            .returning(|_, _| Ok(()));

        let server = BleServer::new(mock_handler, 10);
        let conn = server.connection();

        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = BleCmd {
            addr: "test_addr".to_string(),
            payload: vec![1, 2, 3],
            resp: resp_tx,
        };

        conn.send(BleApi::MobileSdpResponse(cmd)).await.unwrap();
        let result = resp_rx.await.unwrap();
        assert!(result.is_ok());
    }
    /*
        #[tokio::test]
        async fn test_ble_server_subscribe_to_sdp_req() {
            init_logger();
            let mut mock_handler = MockCommHandler::new();
            mock_handler
                .expect_subscribe_to_sdp_req()
                .with(eq("test_addr".to_string()), eq(1024))
                .returning(|_, _| Ok(PubSubSubscriber {}));

            let server = BleServer::new(mock_handler, 10);
            let conn = server.connection();

            let (resp_tx, resp_rx) = oneshot::channel();
            let sub = BleCmd {
                addr: "test_addr".to_string(),
                max_buffer_len: 1024,
                resp: resp_tx,
            };

            conn.send(BleApi::Subscribe(PubSubTopic::SdpCall, sub)).await.unwrap();
            let result = resp_rx.await.unwrap();
            assert!(result.is_ok());
        }
    */
}
