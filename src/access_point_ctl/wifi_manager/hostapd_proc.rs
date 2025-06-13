//! This module contains the implementation to handle the hostapd process as a child process.
//! It provides structures and traits to manage the hostapd daemon, which is used to create a WiFi access point.

use super::super::process_hdl::ProcessHdlOps;
use super::file_hdl::FileHdlOps;
use crate::error::Result;
use log::{info, warn};
use std::process::Command;

#[cfg(test)]
use mockall::automock;
/// Structure to hold WiFi credentials
///
/// # Fields
///
/// * `ssid` - The SSID (name) of the WiFi network.
/// * `password` - The password for the WiFi network.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WifiCredentials {
    pub ssid: String,
    pub password: String,
}

/// Trait to control the Hostapd process
///
/// This trait defines the methods required to start and stop the Hostapd process.
#[cfg_attr(test, automock)]
pub trait HostapdProcCtl {
    /// Start the Hostapd process with the given credentials, interface name, and control directory.
    ///
    /// # Arguments
    ///
    /// * `creds` - WiFi credentials including SSID and password.
    /// * `iw_name` - The name of the network interface to use.
    /// * `control_dir` - The directory for control interface.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns Ok(()) if the process starts successfully, otherwise returns an error.
    fn start(
        &mut self, creds: &WifiCredentials, iw_name: &str, control_dir: &str,
    ) -> Result<()>;

    /// Stop the Hostapd process.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns Ok(()) if the process stops successfully, otherwise returns an error.
    fn stop(&mut self) -> Result<()>;
}

/// Structure to manage the Hostapd process
///
/// This structure holds the configuration file handler and the process handler.
pub struct HostapdProc<P, F>
where
    P: ProcessHdlOps,
    F: FileHdlOps,
{
    config_file: F,
    process: P,
}

impl<P: ProcessHdlOps, F: FileHdlOps> HostapdProc<P, F> {
    /// Create a new HostapdProc instance.
    ///
    /// # Arguments
    ///
    /// * `config_file` - The file handler for the configuration file.
    /// * `process` - The process handler for managing the Hostapd process.
    ///
    /// # Returns
    ///
    /// * `Self` - Returns a new instance of HostapdProc.
    pub fn new(config_file: F, process: P) -> Self {
        Self { config_file, process }
    }
}

impl<P: ProcessHdlOps, F: FileHdlOps> HostapdProcCtl for HostapdProc<P, F> {
    /// Start the Hostapd process.
    ///
    /// This method creates the Hostapd configuration file, writes the necessary configuration,
    /// and starts the Hostapd process.
    ///
    /// # Arguments
    ///
    /// * `creds` - WiFi credentials including SSID and password.
    /// * `iw_name` - The name of the network interface to use.
    /// * `control_dir` - The directory for control interface.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns Ok(()) if the process starts successfully, otherwise returns an error.
    fn start(
        &mut self, creds: &WifiCredentials, iw_name: &str, control_dir: &str,
    ) -> Result<()> {
        // Create the hostapd config file
        self.config_file.open()?;

        // Format the hostapd configuration
        let hostap_config = format!(
            r#"ctrl_interface={}
interface={}
driver=nl80211
ssid={}
hw_mode=g
channel=6
wpa=2
wpa_passphrase={}
wpa_key_mgmt=WPA-PSK
rsn_pairwise=CCMP
ieee80211n=1
wmm_enabled=1
"#,
            control_dir, iw_name, creds.ssid, creds.password
        );

        // Write the configuration to the file
        self.config_file.write_data(hostap_config.as_bytes())?;

        info!("Hostapd config file created");

        // Create the Unix socket upfront to avoid busy-waiting
        let mut cmd = Command::new("hostapd");
        cmd.arg(self.config_file.get_path());

        // In ubuntu the network manager will try to take control of the interface causing a
        // hostapd to fail to start. We need to disable the network manager for the interface
        if let Ok(_) = std::process::Command::new("nmcli")
            .arg("device")
            .arg("set")
            .arg(&iw_name)
            .arg("managed")
            .arg("no")
            .output()
        {
            info!("Network manager disabled for interface {}", iw_name);
        } else {
            warn!(
                "Failed to disable network manager for interface {}",
                iw_name
            );
        }

        // Spawn the hostapd process
        self.process.spawn(&mut cmd)?;

        info!("Hostapd process started");

        Ok(())
    }

    /// Stop the Hostapd process.
    ///
    /// This method stops the Hostapd process by killing it.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Returns Ok(()) if the process stops successfully, otherwise returns an error.
    fn stop(&mut self) -> Result<()> {
        // Kill the hostapd process
        self.process.kill()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_point_ctl::{
        process_hdl::MockProcessHdlOps, wifi_manager::file_hdl::MockFileHdlOps,
    };
    use anyhow::anyhow;

    fn init_logger() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_hostapd_proc_start() {
        init_logger();
        let mut mock_file_hdl = MockFileHdlOps::new();
        let mut mock_process_hdl = MockProcessHdlOps::new();

        // Set expectations
        mock_file_hdl.expect_open().times(1).returning(|| Ok(()));
        mock_file_hdl
            .expect_write_data()
            .withf(|data| {
                let config_str = String::from_utf8_lossy(data);
                config_str.contains("ssid=test_ssid")
                    && config_str.contains("wpa_passphrase=test_password")
            })
            .times(1)
            .returning(|_| Ok(()));
        mock_file_hdl
            .expect_get_path()
            .times(1)
            .return_const("/tmp/hostapd.conf".into());
        mock_process_hdl
            .expect_spawn()
            .withf(|cmd| cmd.get_program() == "hostapd")
            .times(1)
            .returning(|_| Ok(()));

        let mut hostapd_proc =
            HostapdProc::new(mock_file_hdl, mock_process_hdl);

        let creds = WifiCredentials {
            ssid: "test_ssid".to_string(),
            password: "test_password".to_string(),
        };

        // Call the start method
        let result = hostapd_proc.start(&creds, "wlan0", "/var/run/hostapd");

        // Assert that the method returns Ok(())
        assert!(result.is_ok());
    }

    #[test]
    fn test_hostapd_proc_start_fail_open() {
        init_logger();
        let mut mock_file_hdl = MockFileHdlOps::new();
        let mock_process_hdl = MockProcessHdlOps::new();

        // Set expectations
        mock_file_hdl
            .expect_open()
            .times(1)
            .returning(|| Err(anyhow!("Failed to open file")));

        let mut hostapd_proc =
            HostapdProc::new(mock_file_hdl, mock_process_hdl);
        let creds = WifiCredentials {
            ssid: "test_ssid".to_string(),
            password: "test_password".to_string(),
        };

        // Call the start method
        let result = hostapd_proc.start(&creds, "wlan0", "/var/run/hostapd");

        // Assert that the method returns an error
        assert!(result.is_err());
    }

    #[test]
    fn test_hostapd_proc_start_fail_write() {
        init_logger();
        let mut mock_file_hdl = MockFileHdlOps::new();
        let mock_process_hdl = MockProcessHdlOps::new();

        // Set expectations
        mock_file_hdl.expect_open().times(1).returning(|| Ok(()));
        mock_file_hdl
            .expect_write_data()
            .times(1)
            .returning(|_| Err(anyhow!("Failed to write data")));

        let mut hostapd_proc =
            HostapdProc::new(mock_file_hdl, mock_process_hdl);
        let creds = WifiCredentials {
            ssid: "test_ssid".to_string(),
            password: "test_password".to_string(),
        };

        // Call the start method
        let result = hostapd_proc.start(&creds, "wlan0", "/var/run/hostapd");

        // Assert that the method returns an error
        assert!(result.is_err());
    }

    #[test]
    fn test_hostapd_proc_start_fail_spawn() {
        init_logger();
        let mut mock_file_hdl = MockFileHdlOps::new();
        let mut mock_process_hdl = MockProcessHdlOps::new();

        // Set expectations
        mock_file_hdl.expect_open().times(1).returning(|| Ok(()));
        mock_file_hdl
            .expect_write_data()
            .withf(|data| {
                let config_str = String::from_utf8_lossy(data);
                config_str.contains("ssid=test_ssid")
                    && config_str.contains("wpa_passphrase=test_password")
            })
            .times(1)
            .returning(|_| Ok(()));
        mock_file_hdl
            .expect_get_path()
            .times(1)
            .return_const("/tmp/hostapd.conf".into());
        mock_process_hdl
            .expect_spawn()
            .withf(|cmd| cmd.get_program() == "hostapd")
            .times(1)
            .returning(|_| Err(anyhow!("Failed to spawn process")));

        let mut hostapd_proc =
            HostapdProc::new(mock_file_hdl, mock_process_hdl);
        let creds = WifiCredentials {
            ssid: "test_ssid".to_string(),
            password: "test_password".to_string(),
        };

        // Call the start method
        let result = hostapd_proc.start(&creds, "wlan0", "/var/run/hostapd");

        // Assert that the method returns an error
        assert!(result.is_err());
    }

    #[test]
    fn test_hostapd_proc_stop() {
        init_logger();
        let mut mock_process_hdl = MockProcessHdlOps::new();

        // Set expectations
        mock_process_hdl.expect_kill().times(1).returning(|| Ok(()));

        let mock_file_hdl = MockFileHdlOps::new();
        let mut hostapd_proc =
            HostapdProc::new(mock_file_hdl, mock_process_hdl);

        // Call the stop method
        let result = hostapd_proc.stop();

        // Assert that the method returns Ok(())
        assert!(result.is_ok());
    }
}
