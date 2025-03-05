//! This module provides functionality to manage WPA control operations.
//!
//! The `WpaCtl` struct and the `WpaCtlClientOps` trait define methods to connect to,
//! disconnect from, enable, disable, and set the SSID for a WPA client. The module
//! uses the `wpactrl` crate to interact with the WPA control interface and provides
//! error handling and logging for these operations.

use crate::error::Result;
use anyhow::anyhow;
use log::{error, info, warn};
use std::{
    fs,
    path::{Path, PathBuf},
};
use wpactrl::Client;

#[cfg(test)]
use mockall::automock;

/// Trait defining operations for WPA control client.
#[cfg_attr(test, automock)]
pub trait WpaCtlClientOps {
    /// Connects to the WPA control interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails.
    fn connect(&mut self) -> Result<()>;

    /// Disconnects from the WPA control interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the disconnection fails.
    fn disconnect(&mut self) -> Result<()>;

    /// Enables the WPA client.
    ///
    /// # Errors
    ///
    /// Returns an error if enabling the client fails.
    fn enable(&mut self) -> Result<()>;

    /// Disables the WPA client.
    ///
    /// # Errors
    ///
    /// Returns an error if disabling the client fails.
    fn disable(&mut self) -> Result<()>;

    /// Sets the SSID for the WPA client.
    ///
    /// # Errors
    ///
    /// Returns an error if setting the SSID fails.
    fn set_ssid(&mut self, ssid: &str) -> Result<()>;

    /// Sets the password for the Wi-Fi access point.
    ///
    /// # Arguments
    ///
    /// * `password` - A string slice that holds the password to be set.
    ///
    /// # Returns
    ///
    /// * `Result<String>` - A result containing a success message or an error.
    fn set_password(&mut self, password: &str) -> Result<()>;

    /// Reloads the Wi-Fi configuration.
    ///
    /// This function attempts to reload the Wi-Fi configuration and returns the result as a `String`.
    ///
    /// # Errors
    ///
    /// This function will return an error if the reload operation fails.
    fn reload(&mut self) -> Result<()>;

    /// Retrieves the interface name for the Wi-Fi device.
    ///
    /// # Returns
    ///
    /// * `&str` - A string slice that holds the interface name.
    fn get_iw_name(&self) -> &str;

    /// Retrieves the control directory path for the Wi-Fi device.
    ///
    /// # Returns
    ///
    /// * `&Path` - A reference to a Path that holds the control directory path.
    fn get_control_dir(&self) -> &Path;
}

/// Struct representing the WPA control client.
pub struct WpaCtl {
    client: Option<Client>,
    control_dir: PathBuf,
    iw_name: String,
}

impl WpaCtl {
    /// Creates a new `WpaCtl` instance.
    ///
    /// # Arguments
    ///
    /// * `control_dir` - Path to the control directory.
    /// * `iw_name` - Interface name.
    pub fn new<P: AsRef<Path>>(control_dir: P, iw_name: &str) -> Self {
        Self {
            client: None,
            control_dir: control_dir.as_ref().to_path_buf(),
            iw_name: iw_name.to_string(),
        }
    }

    /// Helper method to get a mutable reference to the connected client.
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not connected.
    fn get_client(&mut self) -> Result<&mut Client> {
        self.client.as_mut().ok_or_else(|| anyhow!("WPA client not connected"))
    }

    /// Handles a Wi-Fi control request.
    ///
    /// This function processes a given Wi-Fi control request and returns the result as a `String`.
    ///
    /// # Arguments
    ///
    /// * `request` - A string slice that holds the request to be processed.
    ///
    /// # Errors
    ///
    /// This function will return an error if the request handling fails.
    fn handle_request(&mut self, request: &str) -> Result<String> {
        let client = self.get_client()?;
        let resp = client.request(request)?;
        if resp == "FAIL" {
            return Err(anyhow!("Failed to handle request"));
        }
        Ok(resp)
    }
}

impl WpaCtlClientOps for WpaCtl {
    fn connect(&mut self) -> Result<()> {
        // No connection needed if already connected
        if self.client.is_some() {
            warn!("WPA client already connected");
            return Ok(());
        }

        info!("Connecting to WPA control socket");
        let soc_path = self.control_dir.join(&self.iw_name);
        self.client = Some(Client::builder().ctrl_path(&soc_path).open()?);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from WPA control socket");
        std::mem::drop(self.client.take());
        Ok(())
    }

    fn enable(&mut self) -> Result<()> {
        self.handle_request("ENABLE").map(|_| ())
    }

    fn disable(&mut self) -> Result<()> {
        self.handle_request("DISABLE")?;
        Ok(())
    }

    fn set_ssid(&mut self, ssid: &str) -> Result<()> {
        self.handle_request(&format!("SET ssid {}", ssid)).map(|_| ())
    }

    fn set_password(&mut self, password: &str) -> Result<()> {
        self.handle_request(&format!("SET wpa_passphrase {}", password))
            .map(|_| ())
    }

    fn reload(&mut self) -> Result<()> {
        self.handle_request("RELOAD").map(|_| ())
    }

    fn get_iw_name(&self) -> &str {
        &self.iw_name
    }

    fn get_control_dir(&self) -> &Path {
        &self.control_dir
    }
}
