mod access_point_ctl;
mod app_data;
mod app_data_store;
mod ble;
mod error;
mod gatt_const;
mod provisioner;
mod sdp_exchanger;


use access_point_ctl::{
    dhcp_server::{DhcpIpRange, DnsmasqProc},
    iw_link::{wdev_drv, IwLink},
    process_hdl::ProcessHdl,
    wifi_manager::{
        FileHdl, HostapdProc, WifiCredentials, WifiManager, WpaCtl,
    },
    AccessPointCtl, ApController,
};
use app_data::{AppData, ConnectionType, DiskBasedDb, HostInfo};
use error::Result;

use ble::{ble_clients::provisioner::ProvisionerClient, ble_server::BleServer, AppDataStore, MobileComm};
use tokio::io::AsyncBufReadExt;

use log::info;
use sdp_exchanger::SdpExchanger;

fn setup_access_point() -> Result<impl AccessPointCtl> {
    let if_name = "wcdirect0";

    //init the wireless interface handler---------
    let link = IwLink::new(wdev_drv::Nl80211Driver, if_name)?;

    //init the dhcp server---------
    let dhcp_server_proc = DnsmasqProc::new(ProcessHdl::handler());

    //wifi manager process
    let hostapd_proc = HostapdProc::new(
        FileHdl::from_path("/tmp/hostapd.conf"),
        ProcessHdl::handler(),
    );

    let wpactrl = WpaCtl::new("/tmp/hostapd", if_name);

    let creds = WifiCredentials {
        ssid: "WebcamDirect".to_string(),
        password: "12345678".to_string(),
    };

    let wifi_manager = WifiManager::new(&creds, hostapd_proc, wpactrl)?;

    let mut ap = ApController::new(link, dhcp_server_proc, wifi_manager);

    ap.start_dhcp_server(DhcpIpRange::new("193.168.3.5", "193.168.3.150")?)?;

    ap.start_wifi()?;

    //init Access Point manager------
    Ok(ap)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    info!("Starting webcam direct");

    //get host name
    let mut host_info = HostInfo {
        name: "MyPC".to_string(),
        connection_type: ConnectionType::WLAN,
    };

    if let Ok(host_name) = hostname::get()?.into_string() {
        host_info.name = host_name;
    }

    let ap_controller_rc = setup_access_point();
    if ap_controller_rc.is_ok() {
        host_info.connection_type = ConnectionType::AP;
    }

    let session = bluer::Session::new().await?;

    let adapter = session.default_adapter().await?;

    adapter.set_powered(true).await?;

    //init the in disk database
    let config_path = "/tmp";

    let disk_db = DiskBasedDb::open_from(config_path)?;

    let app_data = AppData::new(disk_db, host_info.clone())?;

    let mobile_comm = MobileComm::new(app_data)?;

    let ble_server = BleServer::new(mobile_comm, 512);

    let conn = ble_server.connection();
    let _provisioner = ProvisionerClient::new(adapter.clone(), conn, host_info.name); 

    //    let app_store = AppStore::new("webcam-direct-config.json").await;

    //    info!("Webcam direct started");
    // let mut sdp_exchanger =
    //     SdpExchanger::new(adapter.clone(), app_store.clone());

    //    let mut provisioner = Provisioner::new(adapter.clone(), app_store.clone());

    //    provisioner.start_provisioning().await?;

    //sdp_exchanger.start().await?;

    //    device_props(adapter.clone()).await?;
    //

    info!("Service ready. Press enter to quit.");
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let _ = lines.next_line().await;

    //provisioner.stop_provisioning();
    //sdp_exchanger.stop().await?;

    info!("webcam direct stopped stopped");

    Ok(())
}
