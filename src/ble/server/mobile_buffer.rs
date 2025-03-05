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

use crate::ble::api::MAX_BUFFER_LEN;

use crate::ble::api::{
    Address, CmdApi, CommBuffer, CommandReq, QueryApi, QueryReq,
};
use crate::ble::comm_types::DataChunk;
use crate::error::Result;
use log::{error, info, warn};
use std::collections::HashMap;

/// Represents the current state of a mobile buffer.
#[derive(Default)]
pub struct BufferCursor {
    writer: HashMap<CmdApi, CommBuffer>,
    reader: HashMap<QueryApi, usize>,
}

/// Manages the buffer states for multiple mobile devices.
pub struct MobileBufferMap {
    /// A map storing the buffer status for each mobile address.
    mobile_buffer_status: HashMap<Address, BufferCursor>,

    /// Datachunk overhead len
    chunk_len: usize,
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
    pub fn new(chunk_len: usize) -> Self {
        info!("DataChunk length: {}", chunk_len);
        Self { mobile_buffer_status: HashMap::new(), chunk_len }
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
    pub fn get_next_data_chunk<P: AsRef<[u8]>>(
        &mut self, addr: &str, query: &QueryReq, data: &P,
    ) -> Result<Vec<u8>> {
        let QueryReq { query_type, resp_buffer_len } = query;

        let resp_buffer_len = resp_buffer_len - self.chunk_len;

        let data = data.as_ref();

        let BufferCursor { reader, .. } = self.get_cursors(addr);

        //Add the query type to the map if not present
        let remain_len = reader.entry(query_type.clone()).or_insert(data.len());

        let chunk_start = data.len() - *remain_len;
        let chunk_end = (chunk_start + resp_buffer_len).min(data.len());

        // Update remaining length
        if chunk_end == data.len() {
            *remain_len = 0;
        } else {
            *remain_len -= resp_buffer_len;
        }

        let data_chunk = DataChunk {
            r: *remain_len,
            d: data[chunk_start..chunk_end].to_owned(),
        };

        if data_chunk.r == 0 || resp_buffer_len > MAX_BUFFER_LEN {
            if resp_buffer_len > MAX_BUFFER_LEN {
                warn!(
                    "Max buffer limit reached for mobile with addr: {}",
                    addr
                );
            }

            reader.remove(query_type); //remove the reader channel when done
        }

        info!("DataChunk payload len: {}", data_chunk.d.len());

        // Serialize the data chunk
        data_chunk.try_into()
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
    ) -> Result<Option<CommBuffer>> {
        // Initialize current buffer if idle
        let CommandReq { cmd_type, payload } = cmd;

        //deserialize the data chunk
        let payload: DataChunk = payload.clone().try_into()?;

        //get the writer cursor
        let BufferCursor { writer, .. } = self.get_cursors(addr);

        let curr_buffer = writer.entry(cmd_type.clone()).or_default();

        //check if the buffer limit is reached
        if curr_buffer.len() + payload.d.len() > MAX_BUFFER_LEN {
            error!("Buffer limit reached for mobile with addr: {}", addr);
            writer.remove(cmd_type); //remove the writer channel when done
            return Ok(None);
        }

        curr_buffer.extend_from_slice(&payload.d);

        if payload.r == 0 {
            // Finalize and reset to idle state
            let buffer = curr_buffer.to_owned();
            writer.remove(cmd_type); //remove the writer channel when done
            return Ok(Some(buffer));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use env_logger;
    use log::{debug, info};

    const CHUNK_LEN: usize = 5;

    fn init_test() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_remove_mobile() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "00:11:22:33:44:55";

        buffer_map.mobile_buffer_status.insert(
            addr.to_string(),
            BufferCursor { writer: HashMap::new(), reader: HashMap::new() },
        );

        buffer_map.remove_mobile(addr);

        assert!(!buffer_map.mobile_buffer_status.contains_key(addr));
    }

    #[test]
    fn test_get_next_data_chunk_simple_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        let expected_len = 100;
        let allowed_data_len = 100 - CHUNK_LEN;
        let data = vec![55; allowed_data_len]; // Simple data
        let query = QueryReq {
            query_type: QueryApi::HostInfo,
            resp_buffer_len: expected_len,
        };

        let chunk: DataChunk = buffer_map
            .get_next_data_chunk(addr, &query, &data)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(chunk.r, 0);
        assert_eq!(chunk.d.len(), allowed_data_len);
    }

    #[test]
    fn test_get_next_data_chunk_large_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        let expected_len = 5000;
        let data = vec![55; expected_len]; // Large data
        let resp_buffer_len = 1024;
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len };

        let allowed_data_len = resp_buffer_len - CHUNK_LEN;

        let mut chunks = Vec::new();

        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr, &query, &data)
                .unwrap()
                .try_into()
                .unwrap();

            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 5);
        assert_eq!(chunks[0].d.len(), allowed_data_len); //5000 - 1024 + CHUNK_LEN = 3981
        assert_eq!(chunks[0].r, 3981);
        assert_eq!(chunks[1].d.len(), allowed_data_len); // 3971 - 1024 + CHUNK_LEN = 2962
        assert_eq!(chunks[1].r, 2962);
        assert_eq!(chunks[2].d.len(), allowed_data_len); // 2952 - 1024 + CHUNK_LEN = 1943
        assert_eq!(chunks[2].r, 1943);
        assert_eq!(chunks[3].d.len(), allowed_data_len); // 1928 - 1024 + CHUNK_LEN= 924
        assert_eq!(chunks[3].r, 924);
        assert_eq!(chunks[4].d.len(), 924); // 0
        assert_eq!(chunks[4].r, 0);
    }

    #[test]
    fn test_get_next_data_chunk_large_data_changing_max_buffer() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        let expected_len = 300;
        let data = vec![55; expected_len]; // Large data
        let mut chunks = Vec::new();

        let mut max_buffer_len = 15;
        let mut query = QueryReq {
            query_type: QueryApi::HostInfo,
            resp_buffer_len: max_buffer_len,
        };
        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr, &query, &data)
                .unwrap()
                .try_into()
                .unwrap();
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
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        let expected_len = 300;
        let data = vec![55; expected_len]; // Large data

        let resp_buffer_len = 15;

        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len };

        let allowed_data_len = resp_buffer_len - CHUNK_LEN;

        let mut chunks = Vec::new();

        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr, &query, &data)
                .unwrap()
                .try_into()
                .unwrap();
            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 30);
        assert_eq!(chunks[0].d.len(), allowed_data_len);
        assert_eq!(chunks[0].r, 290);
        assert_eq!(chunks[29].d.len(), allowed_data_len);
        assert_eq!(chunks[29].r, 0);

        //start again
        chunks = Vec::new();
        let resp_buffer_len = 13;
        let new_query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len };

        let allowed_data_len = resp_buffer_len - CHUNK_LEN;
        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr, &new_query, &data)
                .unwrap()
                .try_into()
                .unwrap();
            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        //test partial chunks
        assert_eq!(chunks.len(), 38);
        assert_eq!(chunks[0].d.len(), allowed_data_len);
        assert_eq!(chunks[0].r, 292); // 300 - 13 + CHUNK_LEN = 292
        assert_eq!(chunks[37].d.len(), 4);
        assert_eq!(chunks[37].r, 0);
    }

    #[test]
    fn test_get_complete_buffer_simple_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "11:22:33:44:55:66";

        let expected_len = 100;
        let allowed_data_len = 100 - CHUNK_LEN;
        let data = vec![55; allowed_data_len]; // Large data
        let query = QueryReq {
            query_type: QueryApi::HostInfo,
            resp_buffer_len: expected_len,
        };

        let chunk: DataChunk = buffer_map
            .get_next_data_chunk(addr, &query, &data)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(chunk.r, 0);

        let cmd = CommandReq {
            cmd_type: CmdApi::MobileDisconnected,
            payload: chunk.try_into().unwrap(),
        };

        if let Some(buffer) =
            buffer_map.get_complete_buffer(addr, &cmd).unwrap()
        {
            assert_eq!(buffer.len(), allowed_data_len);
        }
    }

    #[test]
    fn test_get_complete_buffer_large_data() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "11:22:33:44:55:66";

        let expected_len = 3355;
        let data = vec![55; expected_len]; // Large data
        let query =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len: 512 };
        let mut chunks = Vec::new();

        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr, &query, &data)
                .unwrap()
                .try_into()
                .unwrap();
            chunks.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        let mut indx = 0;
        while indx <= chunks.len() {
            let cmd = CommandReq {
                cmd_type: CmdApi::MobileDisconnected,
                payload: chunks[indx].clone().try_into().unwrap(),
            };
            if let Some(buffer) =
                buffer_map.get_complete_buffer(addr, &cmd).unwrap()
            {
                assert_eq!(buffer.len(), expected_len);
                break;
            }
            info!("Buffer not ready yet");
            indx += 1;
        }
    }

    #[test]
    fn test_multiple_device_in_parallel_communication() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr1 = "AA:BB:CC:DD:EE:FF";
        let addr2 = "11:22:33:44:55:66";

        let expected_len = 1000;
        let data1 = vec![55; expected_len]; // Large data
        let data2 = vec![66; expected_len]; // Large data

        let resp_buffer_len = 100 + CHUNK_LEN;
        let query1 =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len };
        let query2 =
            QueryReq { query_type: QueryApi::HostInfo, resp_buffer_len };

        let allowed_data_len = resp_buffer_len - CHUNK_LEN;

        let mut chunks1 = Vec::new();
        let mut chunks2 = Vec::new();

        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr1, &query1, &data1)
                .unwrap()
                .try_into()
                .unwrap();
            chunks1.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        loop {
            let chunk: DataChunk = buffer_map
                .get_next_data_chunk(addr2, &query2, &data2)
                .unwrap()
                .try_into()
                .unwrap();
            chunks2.push(chunk.clone());
            if chunk.r == 0 {
                break;
            }
        }

        // Check that both channels have received the correct number of chunks
        assert_eq!(chunks1.len(), expected_len / allowed_data_len); // 1000 / 100 = 10
        assert_eq!(chunks2.len(), expected_len / allowed_data_len); // 1000 / 100 = 10

        // Check that the data in the chunks is correct
        for chunk in chunks1 {
            assert_eq!(chunk.d, vec![55; allowed_data_len]);
        }

        for chunk in chunks2 {
            assert_eq!(chunk.d, vec![66; allowed_data_len]);
        }
    }

    #[test]
    fn test_single_device_single_parallel_communication() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        let expected_len = 500;
        let data1 = vec![55; expected_len]; // Large data
        let data2 = vec![66; expected_len]; // Large data

        let cmd1 = CommandReq {
            cmd_type: CmdApi::MobileDisconnected,
            payload: DataChunk { r: 0, d: data1.clone() }.try_into().unwrap(),
        };

        let cmd2 = CommandReq {
            cmd_type: CmdApi::RegisterMobile,
            payload: DataChunk { r: 0, d: data2.clone() }.try_into().unwrap(),
        };

        let mut buffer1 = Vec::new();
        let mut buffer2 = Vec::new();

        while let Some(chunk) =
            buffer_map.get_complete_buffer(addr, &cmd1).unwrap()
        {
            buffer1.extend_from_slice(&chunk);
            if buffer1.len() >= data1.len() {
                break;
            }
        }

        while let Some(chunk) =
            buffer_map.get_complete_buffer(addr, &cmd2).unwrap()
        {
            buffer2.extend_from_slice(&chunk);
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
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        // prepare the data and fill up the chunks
        let expected_len = 500;
        let data1 = vec![55; expected_len]; // Large data
        let data2 = vec![66; expected_len]; // Large data

        let mut chunks1 = Vec::new();
        let mut chunks2 = Vec::new();

        let mut start_chunk = 0;
        let chunk_len = 100;

        while start_chunk <= expected_len - chunk_len {
            let end_chunk = start_chunk + chunk_len;

            chunks1.push(DataChunk {
                r: expected_len - end_chunk,
                d: data1[start_chunk..end_chunk].to_owned(),
            });

            chunks2.push(DataChunk {
                r: expected_len - end_chunk,
                d: data2[start_chunk..end_chunk].to_owned(),
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
                payload: chunk1.clone().try_into().unwrap(),
            };

            if let Some(buffer1) =
                buffer_map.get_complete_buffer(addr, &cmd).unwrap()
            {
                assert_eq!(buffer1.len(), expected_len);
                assert_eq!(buffer1, data1);
            }

            let cmd = CommandReq {
                cmd_type: CmdApi::SdpOffer,
                payload: chunk2.clone().try_into().unwrap(),
            };

            if let Some(buffer2) =
                buffer_map.get_complete_buffer(addr, &cmd).unwrap()
            {
                assert_eq!(buffer2.len(), expected_len);
                assert_eq!(buffer2, data2);
            }
        }
    }

    #[test]
    fn test_maximum_buffer_size() {
        init_test();
        let mut buffer_map = MobileBufferMap::new(CHUNK_LEN);
        let addr = "AA:BB:CC:DD:EE:FF";

        let expected_len = 5001;
        let data = vec![55; expected_len]; // Large data
                                           //
        let cmd = CommandReq {
            cmd_type: CmdApi::MobileDisconnected,
            payload: DataChunk { r: 0, d: data.clone() }.try_into().unwrap(),
        };

        let buffer = buffer_map.get_complete_buffer(addr, &cmd).unwrap();

        assert!(buffer.is_none());
    }
}
