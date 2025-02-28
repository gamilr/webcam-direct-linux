//! This module handles the chunk data transfer for BLE mobile buffers.
//! Since the BLE communication is limited to mtu negotiated size, the data
//! has to be chunked and sent in multiple packets.
//!
//! The `MobileBufferMap` struct manages the buffer states for multiple mobile devices.
//!
//! The devices can keep multiple channels in parallel, but it cannot interrupt the current
//! channel until it is complete.
//!
//! To support multiple channels in parallel in the same device
//! and the same api we need to add a transaction id or any other identifier.

use super::ble_cmd_api::{
    Address, CmdApi, CommandReq, DataChunk, QueryApi, QueryReq,
};
use log::{error, info, warn};
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
    /// # Arguments
    /// * `buffer_max_len` - The maximum length of the buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// let buffer_map = MobileBufferMap::new(1024);
    /// ```
    pub fn new(buffer_max_len: usize) -> Self {
        Self {
            mobile_buffer_status: HashMap::new(),
            buffer_size_limit: buffer_max_len,
        }
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

    /// Retrieves the buffer cursor for a mobile device.
    /// If the device does not exist, it initializes the buffer cursor.
    /// # Arguments
    /// * `addr` - The address of the mobile device.
    /// # returns
    /// A mutable reference to the buffer cursor.
    /// # Examples
    /// ```
    /// let cursor = buffer_map.get_cursors("00:11:22:33:44:55");
    /// ```
    ///
    fn get_cursors(&mut self, addr: &str) -> &mut BufferCursor {
        self.mobile_buffer_status
            .entry(addr.to_string())
            .or_insert(Default::default())
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
    ) -> DataChunk {
        let QueryReq { query_type, resp_buffer_len } = query;

        let max_buffer_size = self.buffer_size_limit;

        let BufferCursor { reader, .. } = self.get_cursors(addr);

        //Add the query type to the map if not present
        let remain_len = reader.entry(query_type.clone()).or_insert(data.len());

        let chunk_start = data.len() - *remain_len;
        let chunk_end = (chunk_start + resp_buffer_len).min(data.len());

        // Update remaining length
        if chunk_end == data.len() {
            *remain_len = 0;
        } else {
            *remain_len -= *resp_buffer_len;
        }

        let data_chunk = DataChunk {
            r: *remain_len,
            d: data[chunk_start..chunk_end].to_owned(),
        };

        if data_chunk.r == 0 || *resp_buffer_len > max_buffer_size {
            if *resp_buffer_len > max_buffer_size {
                warn!(
                    "Max buffer limit reached for mobile with addr: {}",
                    addr
                );
            }

            reader.remove(query_type); //remove the reader channel when done
        }

        info!("DataChunk payload len: {}", data_chunk.d.len());

        return data_chunk;
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

        let max_buffer_size = self.buffer_size_limit;

        //get the writer cursor
        let BufferCursor { writer, .. } = self.get_cursors(addr);

        let curr_buffer = writer.entry(cmd_type.clone()).or_default();

        //check if the buffer limit is reached
        if curr_buffer.len() + payload.d.len() > max_buffer_size {
            error!("Buffer limit reached for mobile with addr: {}", addr);
            writer.remove(cmd_type); //remove the writer channel when done
            return None;
        }

        curr_buffer.push_str(&payload.d);

        if payload.r == 0 {
            // Finalize and reset to idle state
            let buffer = curr_buffer.to_owned();
            writer.remove(cmd_type); //remove the writer channel when done
            return Some(buffer);
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
    fn test_remove_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "00:11:22:33:44:55";

        buffer_map.add_mobile(addr);
        assert!(buffer_map.contains_mobile(addr));

        buffer_map.remove_mobile(addr);
        assert!(!buffer_map.contains_mobile(addr));
    }

    #[test]
    fn test_contains_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        buffer_map.add_mobile("00:11:22:33:44:55");

        assert!(buffer_map.contains_mobile("00:11:22:33:44:55"));

        assert!(!buffer_map.contains_mobile("FF:EE:DD:CC:BB:AA"));

        buffer_map.remove_mobile("00:11:22:33:44:55");
        assert!(!buffer_map.contains_mobile("00:11:22:33:44:55"));
    }

    #[test]
    fn test_get_next_data_chunk_simple_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data = "A".repeat(100); // Simple data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 100 };

        let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);

        assert_eq!(chunk.r, 0);
        assert_eq!(chunk.d.len(), 100);
    }

    #[test]
    fn test_get_next_data_chunk_simple_data_multiple_queries() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data = "A".repeat(100); // Simple data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 100 };

        let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);

        assert_eq!(chunk.r, 0);
        assert_eq!(chunk.d.len(), 100);
    }

    #[test]
    fn test_get_next_data_chunk_large_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data = "A".repeat(5000); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 1024 };
        let mut chunks = Vec::new();

        loop {
            let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);
            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 5);
        assert_eq!(chunks[0].d.len(), 1024); //5000 - 1024 = 3976
        assert_eq!(chunks[0].r, 3976);
        assert_eq!(chunks[1].d.len(), 1024); // 3976 - 1024 = 2952
        assert_eq!(chunks[1].r, 2952);
        assert_eq!(chunks[2].d.len(), 1024); // 2952 - 1024 = 1928
        assert_eq!(chunks[2].r, 1928);
        assert_eq!(chunks[3].d.len(), 1024); // 1928 - 1024 = 904
        assert_eq!(chunks[3].r, 904);
        assert_eq!(chunks[4].d.len(), 904); // 904 - 904 = 0
        assert_eq!(chunks[4].r, 0);
    }

    #[test]
    fn test_get_next_data_chunk_large_data_changing_max_buffer() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data = "A".repeat(300); // Large data
        let mut chunks = Vec::new();

        let mut max_buffer_len = 15;
        let mut query = QueryReq {
            query_type: QueryApi::HostInfo,
            resp_buffer_len: max_buffer_len,
        };
        loop {
            let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);
            chunks.push(chunk.clone());
            debug!("Chunk: {:?}", chunk);
            if chunk.r == 0 {
                break;
            }
            max_buffer_len *= 2;
            query.resp_buffer_len = max_buffer_len;
        }
        debug!("Chunks: {:?}", chunks.len());
        assert!(chunks[chunks.len() - 1].r == 0);
    }

    #[test]
    fn test_get_next_data_chunk_large_data_twice() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data = "A".repeat(300); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 15 };
        let mut chunks = Vec::new();

        loop {
            let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);
            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 20);
        assert_eq!(chunks[0].d.len(), 15); //300 - 15 = 285
        assert_eq!(chunks[0].r, 285);
        assert_eq!(chunks[19].d.len(), 15);
        assert_eq!(chunks[19].r, 0);

        //start again
        let new_query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 13 };
        loop {
            let chunk = buffer_map.get_next_data_chunk(addr, &new_query, &data);
            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 44);
        assert_eq!(chunks[20].d.len(), 13); //300 - 13 = 287
        assert_eq!(chunks[20].r, 287);
        assert_eq!(chunks[43].d.len(), 1);
        assert_eq!(chunks[43].r, 0);
    }

    #[test]
    fn test_get_complete_buffer_simple_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "11:22:33:44:55:66";

        let data = "B".repeat(100); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 100 };

        let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);
        assert_eq!(chunk.r, 0);

        let cmd =
            CommandReq { cmd_type: CmdApi::MobileDisconnected, payload: chunk };
        if let Some(buffer) = buffer_map.get_complete_buffer(addr, &cmd) {
            assert_eq!(buffer.len(), 100);
        }
    }

    #[test]
    fn test_get_complete_buffer_large_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "11:22:33:44:55:66";

        let data = "B".repeat(3355); // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 512 };
        let mut chunks = Vec::new();

        loop {
            let chunk = buffer_map.get_next_data_chunk(addr, &query, &data);
            chunks.push(chunk.clone());
            if chunk.r == 0 {
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

    #[test]
    fn test_multiple_device_in_parallel_communication() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr1 = "AA:BB:CC:DD:EE:FF";
        let addr2 = "11:22:33:44:55:66";

        let data1 = "A".repeat(1000);
        let data2 = "B".repeat(1000);

        let query1 =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 100 };
        let query2 =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 100 };

        let mut chunks1 = Vec::new();
        let mut chunks2 = Vec::new();

        loop {
            let chunk = buffer_map.get_next_data_chunk(addr1, &query1, &data1);
            chunks1.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        loop {
            let chunk = buffer_map.get_next_data_chunk(addr2, &query2, &data2);
            chunks2.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        // Check that both channels have received the correct number of chunks
        assert_eq!(chunks1.len(), 10); // 1000 / 100 = 10
        assert_eq!(chunks2.len(), 10); // 1000 / 100 = 10

        // Check that the data in the chunks is correct
        for chunk in chunks1 {
            assert_eq!(chunk.d, "A".repeat(100));
        }

        for chunk in chunks2 {
            assert_eq!(chunk.d, "B".repeat(100));
        }
    }

    #[test]
    fn test_single_device_single_parallel_communication() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data1 = "A".repeat(500);
        let data2 = "B".repeat(500);

        let cmd1 = CommandReq {
            cmd_type: CmdApi::MobileDisconnected,
            payload: DataChunk { r: 0, d: data1.clone() },
        };

        let cmd2 = CommandReq {
            cmd_type: CmdApi::RegisterMobile,
            payload: DataChunk { r: 0, d: data2.clone() },
        };

        let mut buffer1 = String::new();
        let mut buffer2 = String::new();

        while let Some(chunk) = buffer_map.get_complete_buffer(addr, &cmd1) {
            buffer1.push_str(&chunk);
            if buffer1.len() >= data1.len() {
                break;
            }
        }

        while let Some(chunk) = buffer_map.get_complete_buffer(addr, &cmd2) {
            buffer2.push_str(&chunk);
            if buffer2.len() >= data2.len() {
                break;
            }
        }

        // Check that both buffers have received the correct data
        assert_eq!(buffer1, data1);
        assert_eq!(buffer2, data2);
    }

    #[test]
    fn test_single_device_multiple_parallel_communication() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(5000);
        let addr = "AA:BB:CC:DD:EE:FF";

        // prepare the data and fill up the chunks
        let data1 = "A".repeat(500);
        let data2 = "B".repeat(500);

        let mut chunks1 = Vec::new();
        let mut chunks2 = Vec::new();

        let mut start_chunk = 0;
        let chunk_len = 100;

        while start_chunk <= 500 - chunk_len {
            let end_chunk = start_chunk + chunk_len;

            chunks1.push(DataChunk {
                r: 500 - end_chunk,
                d: data1[start_chunk..end_chunk].to_string(),
            });

            chunks2.push(DataChunk {
                r: 500 - end_chunk,
                d: data2[start_chunk..end_chunk].to_string(),
            });

            start_chunk = end_chunk;
        }

        let mut chunks_itr = chunks1.iter();
        let mut chunks_itr2 = chunks2.iter();

        while let (Some(chunk1), Some(chunk2)) =
            (chunks_itr.next(), chunks_itr2.next())
        {
            let cmd = CommandReq {
                cmd_type: CmdApi::RegisterMobile,
                payload: chunk1.clone(),
            };

            if let Some(buffer1) = buffer_map.get_complete_buffer(addr, &cmd) {
                assert_eq!(buffer1.len(), 500);
                assert_eq!(buffer1, data1);
            }

            let cmd = CommandReq {
                cmd_type: CmdApi::SdpOffer,
                payload: chunk2.clone(),
            };

            if let Some(buffer2) = buffer_map.get_complete_buffer(addr, &cmd) {
                assert_eq!(buffer2.len(), 500);
                assert_eq!(buffer2, data2);
            }
        }
    }

    #[test]
    fn test_maximum_buffer_size() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(9999);
        let addr = "AA:BB:CC:DD:EE:FF";

        let data = "A".repeat(10000); // Large data
                                      //
        let cmd = CommandReq {
            cmd_type: CmdApi::MobileDisconnected,
            payload: DataChunk { r: 0, d: data.clone() },
        };

        let buffer = buffer_map.get_complete_buffer(addr, &cmd);

        assert!(buffer.is_none());
    }
}
