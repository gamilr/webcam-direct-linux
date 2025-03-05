//! This module provides a key-value database abstraction using the `sled` library.
//!
//! It defines traits for schema types and database operations, and implements these
//! traits for a disk-based key-value database. The database operations include adding,
//! reading, updating, and deleting items, with support for serialization and deserialization
//! using `bincode`.
//!
//! # Traits
//!
//! - `SchemaType`: Represents a schema type with a static keyspace name.
//! - `KvDbOps`: Defines operations for a key-value database.
//!
//! # Structs
//!
//! - `DiskBasedDb`: Represents a disk-based key-value database and implements `KvDbOps`.
//!
//! # Usage
//!
//! ```rust
//! use crate::app_data::kv_db::{DiskBasedDb, KvDbOps, SchemaType};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct MyData {
//!     field1: String,
//!     field2: i32,
//! }
//!
//! impl SchemaType for MyData {
//!     const KEYSPACE_NAME: &'static str = "my_data";
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = DiskBasedDb::open_from("my_db_path")?;
//!     let data = MyData {
//!         field1: "value".to_string(),
//!         field2: 42,
//!     };
//!     db.add("my_key", &data)?;
//!     if let Some(read_data) = db.read::<MyData>("my_key")? {
//!         println!("Read data: {:?}", read_data);
//!     }
//!     Ok(())
//! }
//! ```

use crate::error::Result;
use bincode;
use log::info;
use serde::{de::DeserializeOwned, Serialize};
use sled;
use std::path::Path;

#[cfg(test)]
use mockall::automock;

/// A trait representing a schema type with a static keyspace name.
/// extend to add more schema metadata
pub trait SchemaType {
    const KEYSPACE_NAME: &'static str;
}

/// A trait defining operations for a key-value database.
#[cfg_attr(test, automock)]
pub trait KvDbOps: Send + Sync + 'static {
    /// Adds an item to the database.
    ///
    /// # Arguments
    ///
    /// * `key` - A string slice that holds the key.
    /// * `data` - The data to be stored, which must implement `Serialize` and `SchemaType`.
    fn add<ItemType>(&self, key: &str, data: &ItemType) -> Result<()>
    where
        ItemType: Serialize + SchemaType + 'static;

    /// Reads an item from the database.
    ///
    /// # Arguments
    ///
    /// * `key` - A string slice that holds the key.
    ///
    /// # Returns
    ///
    /// An `Option` containing the item if found, or `None` if not found.
    fn read<ItemType>(&self, key: &str) -> Result<Option<ItemType>>
    where
        ItemType: DeserializeOwned + SchemaType + 'static;

    /// Updates an item in the database.
    ///
    /// # Arguments
    ///
    /// * `key` - A string slice that holds the key.
    /// * `data` - The data to be updated, which must implement `Serialize` and `SchemaType`.
    fn update<ItemType>(&self, key: &str, data: &ItemType) -> Result<()>
    where
        ItemType: Serialize + SchemaType + 'static;

    /// Deletes an item from the database.
    ///
    /// # Arguments
    ///
    /// * `key` - A string slice that holds the key.
    ///
    /// # Returns
    ///
    /// An `Option` containing the deleted item if found, or `None` if not found.
    fn delete<ItemType>(&self, key: &str) -> Result<Option<ItemType>>
    where
        ItemType: DeserializeOwned + SchemaType + 'static;
}

/// A struct representing a disk-based key-value database.
pub struct DiskBasedDb {
    db: sled::Db,
}

impl DiskBasedDb {
    /// Opens a disk-based database from the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - A reference to the path where the database is located.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `DiskBasedDb` instance if successful.
    pub fn open_from<P: AsRef<Path>>(path: P) -> Result<DiskBasedDb> {
        let db = sled::open(path)?;
        info!("Database opened");
        Ok(DiskBasedDb { db })
    }
}

impl KvDbOps for DiskBasedDb {
    fn add<ItemType>(&self, key: &str, data: &ItemType) -> Result<()>
    where
        ItemType: Serialize + SchemaType,
    {
        let tree = self.db.open_tree(ItemType::KEYSPACE_NAME)?;
        let serialized = bincode::serialize::<ItemType>(data)?;
        tree.insert(key, serialized)?;
        info!(
            "Added item with key: {} to keyspace: {}",
            key,
            ItemType::KEYSPACE_NAME
        );
        Ok(())
    }

    fn read<ItemType>(&self, key: &str) -> Result<Option<ItemType>>
    where
        ItemType: DeserializeOwned + SchemaType,
    {
        let tree = self.db.open_tree(ItemType::KEYSPACE_NAME)?;
        if let Some(data) = tree.get(key)? {
            let item: ItemType = bincode::deserialize::<ItemType>(&data)?;
            info!(
                "Read item with key: {} from keyspace: {}",
                key,
                ItemType::KEYSPACE_NAME
            );
            return Ok(Some(item));
        }
        info!(
            "Item with key: {} not found in keyspace: {}",
            key,
            ItemType::KEYSPACE_NAME
        );
        Ok(None)
    }

    fn update<ItemType>(&self, key: &str, data: &ItemType) -> Result<()>
    where
        ItemType: Serialize + SchemaType,
    {
        let tree = self.db.open_tree(ItemType::KEYSPACE_NAME)?;
        let serialized = bincode::serialize::<ItemType>(&data)?;
        tree.insert(key, serialized)?;
        info!(
            "Updated item with key: {} in keyspace: {}",
            key,
            ItemType::KEYSPACE_NAME
        );
        Ok(())
    }

    fn delete<ItemType>(&self, key: &str) -> Result<Option<ItemType>>
    where
        ItemType: DeserializeOwned + SchemaType,
    {
        let tree = self.db.open_tree(ItemType::KEYSPACE_NAME)?;
        if let Some(data) = tree.remove(key)? {
            let item: ItemType = bincode::deserialize::<ItemType>(&data)?;
            info!(
                "Deleted item with key: {} from keyspace: {}",
                key,
                ItemType::KEYSPACE_NAME
            );
            return Ok(Some(item));
        }
        info!(
            "Item with key: {} not found in keyspace: {}",
            key,
            ItemType::KEYSPACE_NAME
        );
        Ok(None)
    }
}
