use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use crate::error::Result;

pub fn serialize_data<T: Serialize>(data: &T) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    data.serialize(&mut rmp_serde::Serializer::new(&mut buf))?;
    Ok(buf)
}

pub fn deserialize_data<'a, T: Deserialize<'a>>(data: &'a [u8]) -> Result<T> {
    let mut de_data = rmp_serde::Deserializer::new(Cursor::new(data));
    T::deserialize(&mut de_data)
        .map_err(|e| anyhow!("Failed to deserialize data: {}", e))
}
