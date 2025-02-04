// This module handles the chunk data transfer for BLE mobile buffers.
// Since the BLE communication is limited to mtu negotiated size, the data
// has to be chunked and sent in multiple packets.
//

use super::ble_cmd_api::{
    Address, CmdApi, CommandReq, DataChunk, QueryApi, QueryReq,
};
use log::warn;
use std::collections::HashMap;

/// Represents the current state of a mobile buffer.
#[derive(Default)]
pub struct BufferCursor {
    writer: HashMap<CmdApi, String>,
    reader: HashMap<QueryApi, usize>,
}

/// Manages the buffer states for multiple mobile devices.
pub struct MobileBufferMap {
    /// A map storing the buffer status for each mobile address.
    mobile_buffer_status: HashMap<Address, BufferCursor>,

    /// Buffer size limit for each mobile device in bytes
    /// hard coded to 5000 bytes
    buffer_size_limit: usize,
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
        Self { mobile_buffer_status: HashMap::new(), buffer_size_limit: 5000 }
    }

    /// Adds a mobile device to the buffer map.
    ///
    /// If the device already exists, a warning is logged.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device as a `&str`.
    ///
    /// # Examples
    ///
    /// ```
    /// buffer_map.add_mobile("00:11:22:33:44:55");
    /// ```
    pub fn add_mobile(&mut self, addr: &str) {
        if let Some(_) = self
            .mobile_buffer_status
            .insert(addr.to_string(), Default::default())
        {
            warn!(
                "Mobile with addr: {} already exists in the buffer map",
                addr
            );
        }
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
    /// if buffer_map.contains_mobile("00:11:22:33:44:55") {
    ///    // Do something
    /// }
    ///
    pub fn contains_mobile(&self, addr: &str) -> bool {
        self.mobile_buffer_status.contains_key(addr)
    }

    /// Removes a mobile device from the buffer map.
    ///
    /// If the device does not exist, a warning is logged.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device to remove.
    ///
    /// # Examples
    ///
    /// ```
    /// buffer_map.remove_mobile("00:11:22:33:44:55");
    /// ```
    pub fn remove_mobile(&mut self, addr: &str) {
        if let None = self.mobile_buffer_status.remove(addr) {
            warn!(
                "Mobile with addr: {} does not exist in the buffer map",
                addr
            );
        }
    }

    fn get_cursors(&mut self, addr: &str) -> &mut BufferCursor {
        //add the mobile to the map if not present
        if let None = self.mobile_buffer_status.get_mut(addr) {
            self.mobile_buffer_status.insert(
                addr.to_string(),
                BufferCursor { writer: HashMap::new(), reader: HashMap::new() },
            );
        }

        self.mobile_buffer_status.get_mut(addr).unwrap()
    }

    /// Retrieves a data chunk for a mobile device based on the current buffer state.
    ///
    /// If the buffer is idle, it initializes the remaining length.
    /// It then calculates the appropriate chunk of data to send.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the mobile device.
    /// * `query` - The query request containing the query type and max buffer length.
    /// * `data` - The data to be chunked.
    ///
    /// # Returns
    ///
    /// An `Option<DataChunk>` containing the data chunk if available.
    ///
    /// # Examples
    ///
    /// ```
    /// let chunk_opt = buffer_map.get_next_data_chunk("00:11:22:33:44:55", query, &data);
    /// ```
    pub fn get_next_data_chunk(
        &mut self, addr: &str, query: &QueryReq, data: &str,
    ) -> Option<DataChunk> {
        let QueryReq { query_type, max_buffer_len } = query;

        let BufferCursor { reader, .. } = self.get_cursors(addr);

        if let None = reader.get(&query_type) {
            //add the query type to the map if not present
            reader.insert(query_type.clone(), data.len());
        }

        if let Some(remain_len) = reader.get_mut(&query_type) {
            //reset the remain len if it is 0
            if *remain_len == 0 {
                *remain_len = data.len();
            }

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

            return Some(data_chunk);
        } else {
            warn!(
                "Failed to get remain len, mobile with addr: {} was not present of not ready to receive data",
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
    /// * `cmd` - The command request containing the command type and payload.
    ///
    /// # Returns
    ///
    /// An `Option<String>` containing the full buffer if all data has been received.
    ///
    /// # Examples
    ///
    /// ```
    /// let data_chunk = DataChunk { remain_len: 0, buffer: "Hello".to_string() };
    ///
    /// loop {
    ///    if let Some(buffer) = buffer_map.get_complete_buffer("00:11:22:33:44:55", cmd){
    ///       // Do something with the buffer
    ///       break;
    ///    }
    /// }
    /// ```
    pub fn get_complete_buffer(
        &mut self, addr: &str, cmd: &CommandReq,
    ) -> Option<String> {
        // Initialize current buffer if idle
        let CommandReq { cmd_type, payload } = cmd;

        let buffer_limit_size = self.buffer_size_limit;

        //get the writer cursor
        let BufferCursor { writer, .. } = self.get_cursors(addr);

        if let None = writer.get(&cmd_type) {
            //add the query type to the map if not present
            writer.insert(cmd_type.clone(), String::new());
        }

        if let Some(curr_buffer) = writer.get_mut(&cmd_type) {
            curr_buffer.push_str(&payload.buffer);

            if payload.remain_len == 0 || curr_buffer.len() >= buffer_limit_size
            {
                if curr_buffer.len() >= buffer_limit_size {
                    warn!(
                        "Buffer limit reached for mobile with addr: {}",
                        addr
                    );
                }

                // Finalize and reset to idle state
                let buffer = curr_buffer.to_owned();
                writer.insert(cmd_type.clone(), String::new());
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

#[cfg(test)]
mod tests {

    use super::*;
    use env_logger;
    use log::{debug, info};

    fn init_test() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_add_and_contains_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "00:11:22:33:44:55";

        //mobile should not be in the map
        assert!(!buffer_map.contains_mobile(addr));

        // add mobile to the map
        buffer_map.add_mobile(addr);
        assert!(buffer_map.contains_mobile(addr));
    }

    #[test]
    fn test_remove_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "00:11:22:33:44:55";

        buffer_map.add_mobile(addr);
        assert!(buffer_map.contains_mobile(addr));

        buffer_map.remove_mobile(addr);
        assert!(!buffer_map.contains_mobile(addr));
    }

    #[test]
    fn test_contains_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        buffer_map.add_mobile("00:11:22:33:44:55");

        assert!(buffer_map.contains_mobile("00:11:22:33:44:55"));

        assert!(!buffer_map.contains_mobile("FF:EE:DD:CC:BB:AA"));

        buffer_map.remove_mobile("00:11:22:33:44:55");
        assert!(!buffer_map.contains_mobile("00:11:22:33:44:55"));
    }

    #[test]
    fn test_get_next_data_chunk_from_not_present_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "FF:EE:DD:CC:BB:AA";

        let data = "D".repeat(1000);
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 500 };

        let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);

        assert!(chunk.is_none());
    }

    #[test]
    fn test_get_next_data_chunk_simple_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "AA:BB:CC:DD:EE:FF";
        buffer_map.add_mobile(addr);

        let data = "A".repeat(100); // Simple data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 100 };

        if let Some(chunk) = buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            assert_eq!(chunk.remain_len, 0);
            assert_eq!(chunk.buffer.len(), 100);
        }
    }

    #[test]
    fn test_get_next_data_chunk_large_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "AA:BB:CC:DD:EE:FF";
        buffer_map.add_mobile(addr);

        let data = "A".repeat(5000); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 1024 };
        let mut chunks = Vec::new();

        while let Some(chunk) =
            buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            chunks.push(chunk.clone());
            if chunk.remain_len == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 5);
        assert_eq!(chunks[0].buffer.len(), 1024); //5000 - 1024 = 3976
        assert_eq!(chunks[0].remain_len, 3976);
        assert_eq!(chunks[1].buffer.len(), 1024); // 3976 - 1024 = 2952
        assert_eq!(chunks[1].remain_len, 2952);
        assert_eq!(chunks[2].buffer.len(), 1024); // 2952 - 1024 = 1928
        assert_eq!(chunks[2].remain_len, 1928);
        assert_eq!(chunks[3].buffer.len(), 1024); // 1928 - 1024 = 904
        assert_eq!(chunks[3].remain_len, 904);
        assert_eq!(chunks[4].buffer.len(), 904); // 904 - 904 = 0
        assert_eq!(chunks[4].remain_len, 0);
    }

    #[test]
    fn test_get_next_data_chunk_large_data_changing_max_buffer() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "AA:BB:CC:DD:EE:FF";
        buffer_map.add_mobile(addr);

        let data = "A".repeat(300); // Large data
        let mut chunks = Vec::new();

        let mut max_buffer_len = 15;
        let mut query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len };
        while let Some(chunk) =
            buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            chunks.push(chunk.clone());
            debug!("Chunk: {:?}", chunk);
            if chunk.remain_len == 0 {
                break;
            }
            max_buffer_len *= 2;
            query.max_buffer_len = max_buffer_len;
        }

        debug!("Chunks: {:?}", chunks.len());
        assert!(chunks[chunks.len() - 1].remain_len == 0);
    }

    #[test]
    fn test_get_next_data_chunk_large_data_twice() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "AA:BB:CC:DD:EE:FF";
        buffer_map.add_mobile(addr);

        let data = "A".repeat(300); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 15 };
        let mut chunks = Vec::new();

        while let Some(chunk) =
            buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            chunks.push(chunk.clone());
            if chunk.remain_len == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 20);
        assert_eq!(chunks[0].buffer.len(), 15); //300 - 15 = 285
        assert_eq!(chunks[0].remain_len, 285);
        assert_eq!(chunks[19].buffer.len(), 15);
        assert_eq!(chunks[19].remain_len, 0);

        //start again
        let new_query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 13 };
        while let Some(chunk) =
            buffer_map.get_next_data_chunk(addr, &new_query, &data)
        {
            chunks.push(chunk.clone());
            if chunk.remain_len == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 44);
        assert_eq!(chunks[20].buffer.len(), 13); //300 - 13 = 287
        assert_eq!(chunks[20].remain_len, 287);
        assert_eq!(chunks[43].buffer.len(), 1);
        assert_eq!(chunks[43].remain_len, 0);
    }

    #[test]
    fn test_get_complete_buffer_simple_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "11:22:33:44:55:66";
        buffer_map.add_mobile(addr);

        let data = "B".repeat(100); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 100 };

        if let Some(chunk) = buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            assert_eq!(chunk.remain_len, 0);

            let cmd = CommandReq {
                cmd_type: CmdApi::MobileDisconnected,
                payload: chunk,
            };
            if let Some(buffer) = buffer_map.get_complete_buffer(addr, &cmd) {
                assert_eq!(buffer.len(), 100);
            }
        }
    }

    #[test]
    fn test_get_complete_buffer_large_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "11:22:33:44:55:66";
        buffer_map.add_mobile(addr);

        let data = "B".repeat(3355); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 512 };
        let mut chunks = Vec::new();

        while let Some(chunk) =
            buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            chunks.push(chunk.clone());
            if chunk.remain_len == 0 {
                break;
            }
        }

        let mut indx = 0;
        while indx <= chunks.len() {
            let cmd = CommandReq {
                cmd_type: CmdApi::MobileDisconnected,
                payload: chunks[indx].clone(),
            };
            if let Some(buffer) = buffer_map.get_complete_buffer(addr, &cmd) {
                assert_eq!(buffer.len(), 3355);
                break;
            }
            info!("Buffer not ready yet");
            indx += 1;
        }
    }

    //remove this test when parallel transactions are supported
    #[test]
    fn test_not_allowed_parallel_transactions() {
        init_test();
        let mut buffer_map = MobileBufferMap::new();
        let addr = "11:22:33:44:55:66";
        buffer_map.add_mobile(addr);

        let data = "B".repeat(1000); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, max_buffer_len: 100 };

        if let Some(chunk) = buffer_map.get_next_data_chunk(addr, &query, &data)
        {
            assert_eq!(chunk.remain_len, 900);

            let cmd = CommandReq {
                cmd_type: CmdApi::MobileDisconnected,
                payload: chunk.clone(),
            };
            let resp = buffer_map.get_complete_buffer(addr, &cmd);
            assert!(resp.is_none());
        }
    }
}
