mod access_point_ctl;
mod app_data;
mod ble;
mod error;
mod vdevice_builder;

use tokio::signal;

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

use ble::{
    clients::{
        mobile_prop::MobilePropClient, provisioner::ProvisionerClient,
        sdp_exchanger::SdpExchangerClient,
    },
    server::BleServer,
};

use log::info;
use vdevice_builder::VDeviceBuilder;

use crate::ble::server::mobile_comm::{AppDataStore, MobileComm};

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

    let host_prov_info = app_data.get_host_prov_info()?;

    let mobile_comm = MobileComm::new(app_data, VDeviceBuilder::new().await?)?;

    let ble_server = BleServer::new(mobile_comm, 512);

    let _provisioner = ProvisionerClient::new(
        adapter.clone(),
        ble_server.get_requester(),
        host_prov_info.name.clone(),
    );

    let _mobile_prop_client =
        MobilePropClient::new(adapter.clone(), ble_server.get_requester());

    let _sdp_exchanger = SdpExchangerClient::new(
        adapter.clone(),
        ble_server.get_requester(),
        host_prov_info.name.clone(),
        host_prov_info.id,
    );

    info!("Press any key or Ctrl-C to stop the process");

    tokio::select! {
      _ = signal::ctrl_c() => {
        info!("Received Ctrl-C, shutting down.");
      }
    }

    info!("webcam direct process stopped");

    Ok(())
}
