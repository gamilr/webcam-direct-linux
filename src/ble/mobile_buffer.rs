// This module handles the chunk data transfer for BLE mobile buffers.

use super::ble_cmd_api::{Address, DataChunk};
use log::warn;
use std::collections::HashMap;

/// Represents the current state of a mobile buffer.
#[derive(Default)]
pub enum BufferCursor {
    /// Indicates the remaining length of data to be processed, used in queries.
    RemainLen(usize),
    /// Holds the current buffer content, used in commands.
    CurrentBuffer(String),
    /// Represents an idle state with no active buffer.
    #[default]
    Idle,
}

/// Manages the buffer states for multiple mobile devices.
pub struct MobileBufferMap {
    /// A map storing the buffer status for each mobile address.
    mobile_buffer_status: HashMap<Address, BufferCursor>,
}

impl MobileBufferMap {
    /// Creates a new instance of `MobileBufferMap`.
    ///
    /// # Examples
    ///
    /// ```
    /// let buffer_map = MobileBufferMap::new();
    /// ```
    pub fn new() -> Self {
        Self { mobile_buffer_status: HashMap::new() }
    }

    /// Adds a mobile device to the buffer map.
    ///
    /// If the device already exists, a warning is logged.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device as a `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// buffer_map.add_mobile("00:11:22:33:44:55".to_string());
    /// ```
    pub fn add_mobile(&mut self, addr: String) {
        self.mobile_buffer_status.insert(addr, BufferCursor::Idle);
    }

    /// Check if a mobile device exists in the buffer map.
    ///
    /// # Arguments
    /// * `addr` - The address of the mobile device to check.
    ///
    /// # returns
    /// A boolean indicating if the mobile device exists in the buffer map.
    ///
    /// # Examples
    ///
    /// ```
    /// if buffer_map.contains_mobile("00:11:22:33:44:55".to_string()) {
    ///    // Do something
    ///    }

    pub fn contains_mobile(&self, addr: &str) -> bool {
        self.mobile_buffer_status.contains_key(addr)
    }

    /// Removes a mobile device from the buffer map.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device to remove.
    ///
    /// # Examples
    ///
    /// ```
    /// buffer_map.remove_mobile("00:11:22:33:44:55".to_string());
    /// ```
    pub fn remove_mobile(&mut self, addr: String) {
        self.mobile_buffer_status.remove(&addr);
    }

    /// Retrieves a data chunk for a mobile device based on the current buffer state.
    ///
    /// If the buffer is idle, it initializes the remaining length.
    /// It then calculates the appropriate chunk of data to send.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device.
    /// * `max_buffer_len` - The maximum length of the buffer chunk.
    /// * `data` - The data to be chunked.
    ///
    /// # Returns
    ///
    /// An `Option<DataChunk>` containing the data chunk if available.
    ///
    /// # Examples
    ///
    /// ```
    /// let chunk = buffer_map.get_next_data_chunk(addr, 1024, data.to_string());
    /// ```
    pub fn get_next_data_chunk(
        &mut self, addr: String, max_buffer_len: usize, data: String,
    ) -> Option<DataChunk> {
        // Initialize remaining length if idle
        if let Some(BufferCursor::Idle) = self.mobile_buffer_status.get(&addr) {
            self.mobile_buffer_status
                .insert(addr.clone(), BufferCursor::RemainLen(data.len()));
        }

        if let Some(BufferCursor::RemainLen(remain_len)) =
            self.mobile_buffer_status.get_mut(&addr)
        {
            let chunk_start = data.len() - *remain_len;
            let mut chunk_end = chunk_start + max_buffer_len;

            // Cap the chunk end to the data length
            if chunk_end > data.len() {
                *remain_len = 0;
                chunk_end = data.len();
            } else {
                *remain_len -= max_buffer_len;
            }

            let data_chunk = DataChunk {
                remain_len: *remain_len,
                buffer: data[chunk_start..chunk_end].to_owned(),
            };

            if data_chunk.remain_len == 0 {
                // Reset to idle state when all data is sent
                self.mobile_buffer_status.insert(addr, BufferCursor::Idle);
            }

            return Some(data_chunk);
        } else {
            warn!(
                "Failed to get remain len, mobile with addr: {} was not ready to receive data",
                addr
            );
        }

        None
    }

    /// Retrieves the full buffer for a mobile device by accumulating data chunks.
    ///
    /// If the buffer is idle, it initializes the current buffer.
    /// It appends the received data chunk to the current buffer.
    /// Once all data is received, it returns the complete buffer.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device.
    /// * `data_chunk` - The data chunk to append.
    ///
    /// # Returns
    ///
    /// An `Option<String>` containing the full buffer if all data has been received.
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(full_buffer) = buffer_map.get_complete_buffer(addr, data_chunk) {
    ///     // Process the full buffer
    /// }
    /// ```
    pub fn get_complete_buffer(
        &mut self, addr: String, data_chunk: DataChunk,
    ) -> Option<String> {
        // Initialize current buffer if idle
        if let Some(BufferCursor::Idle) = self.mobile_buffer_status.get(&addr) {
            self.mobile_buffer_status.insert(
                addr.clone(),
                BufferCursor::CurrentBuffer(String::new()),
            );
        }

        if let Some(BufferCursor::CurrentBuffer(curr_buffer)) =
            self.mobile_buffer_status.get_mut(&addr)
        {
            curr_buffer.push_str(&data_chunk.buffer);

            if data_chunk.remain_len == 0 {
                // Finalize and reset to idle state
                let buffer = curr_buffer.to_owned();
                self.mobile_buffer_status
                    .insert(addr.clone(), BufferCursor::Idle);
                return Some(buffer);
            }
        } else {
            warn!(
                "Failed to get current buffer, mobile with addr: {} was not ready to send commands",
                addr
            );
        }

        None
    }
}
