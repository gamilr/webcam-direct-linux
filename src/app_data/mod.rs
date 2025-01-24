//! This module defines the `AppDataStore` trait and the `AppData` struct which provides
//! methods to interact with the application's data store. It includes functionality to
//! get host information and add mobile devices to the store.

mod kv_db;
mod schemas;

use anyhow::anyhow;
pub use kv_db::DiskBasedDb;
pub use kv_db::KvDbOps;
use log::error;
use log::info;
pub use schemas::ConnectionType;
pub use schemas::HostSchema;
pub use schemas::MobileId;
pub use schemas::MobileSchema;
use uuid::Uuid;

use crate::ble::AppDataStore;
use crate::ble::HostProvInfo;
use crate::error::Result;

/// A struct that holds the application's data store.
pub struct AppData<Db> {
    data_db: Db,
}

/// A struct that holds information about the host.
#[derive(Debug, Clone)]
pub struct HostInfo {
    pub name: String,
    pub connection_type: ConnectionType,
}

impl<Db> AppData<Db>
where
    Db: KvDbOps,
{
    /// Creates a new `AppData` instance.
    ///
    /// If the host information is not present in the data store, it adds it.
    ///
    /// # Errors
    ///
    /// Returns an error if there is an issue reading from or writing to the data store.
    pub fn new(data_db: Db, host_info: HostInfo) -> Result<Self> {
        // If host_info is not present in the db, add it
        if let None = data_db.read::<HostSchema>("host_info")? {
            info!("Host info not found in the database. Adding new host info.");
            let host_info = HostSchema {
                id: Uuid::new_v4().to_string(),
                name: host_info.name,
                connection_type: host_info.connection_type,
                registered_mobiles: Vec::new(),
            };
            data_db.add("host_info", &host_info)?;
        } else {
            info!("Host info already exists in the database.");
        }

        Ok(AppData { data_db })
    }
}

impl<Db> AppDataStore for AppData<Db>
where
    Db: KvDbOps,
{
    fn get_host_prov_info(&self) -> Result<HostProvInfo> {
        if let Some(host) = self.data_db.read::<HostSchema>("host_info")? {
            info!("Host info retrieved successfully.");
            return Ok(HostProvInfo {
                id: host.id,
                name: host.name,
                connection_type: if host.connection_type == ConnectionType::WLAN
                {
                    "WLAN".to_string()
                } else {
                    "AP".to_string()
                },
            });
        }
        error!("Failed to retrieve host info: Host info not found.");
        Err(anyhow!("Host info not found"))
    }

    fn add_mobile(&mut self, mobile: &MobileSchema) -> Result<()> {
        if let Some(mut host) = self.data_db.read::<HostSchema>("host_info")? {
            // Update the host info with the new mobile id
            host.registered_mobiles.push(mobile.id.clone());
            self.data_db.update("host_info", &host)?;
            // Store the mobile info
            self.data_db.add(&mobile.id, mobile)?;
            info!("Mobile device added successfully.");
            return Ok(());
        }

        error!("Failed to add mobile device: Host info not found.");
        Err(anyhow!("Host info not found"))
    }

    fn get_mobile(&self, id: &str) -> Result<MobileSchema> {
        if let Some(mobile) = self.data_db.read::<MobileSchema>(id)? {
            info!("Mobile info retrieved successfully.");
            return Ok(mobile);
        }
        error!("Failed to retrieve mobile info: Mobile info not found.");
        Err(anyhow!("Mobile info not found"))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use kv_db::MockKvDbOps;
    use mockall::predicate::eq;

    fn init_logger() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_new_app_data() {
        init_logger();
        let mut mock_db = MockKvDbOps::new();

        let host_info = HostInfo {
            name: "TestHost".to_string(),
            connection_type: ConnectionType::WLAN,
        };

        mock_db
            .expect_read::<HostSchema>()
            .with(eq("host_info"))
            .returning(|_| Ok(None));

        mock_db
            .expect_add::<HostSchema>()
            .withf(|key, host| key == "host_info" && host.name == "TestHost")
            .returning(|_, _| Ok(()));

        let app_data = AppData::new(mock_db, host_info);
        assert!(app_data.is_ok());
    }

    #[test]
    fn test_add_mobile() {
        init_logger();
        let mut mock_db = MockKvDbOps::new();
        let host_schema = HostSchema {
            id: "123".to_string(),
            name: "TestHost".to_string(),
            connection_type: ConnectionType::WLAN,
            registered_mobiles: Vec::new(),
        };

        let mobile_schema = MobileSchema {
            id: "mobile_1".to_string(),
            name: "Mobile1".to_string(),
            ..Default::default()
        };

        mock_db
            .expect_read::<HostSchema>()
            .with(eq("host_info"))
            .returning(move |_| Ok(Some(host_schema.clone())));

        mock_db
            .expect_update::<HostSchema>()
            .withf(|key, host| {
                key == "host_info"
                    && host.registered_mobiles.contains(&"mobile_1".to_string())
            })
            .returning(|_, _| Ok(()));

        mock_db
            .expect_add::<MobileSchema>()
            .withf(|key, mobile| key == "mobile_1" && mobile.name == "Mobile1")
            .returning(|_, _| Ok(()));

        let mut app_data = AppData { data_db: mock_db };
        let result = app_data.add_mobile(&mobile_schema);
        assert!(result.is_ok());
    }
}
