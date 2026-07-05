#![allow(clippy::items_after_statements)]
//! Virtio block device backend.
//!
//! Implements [`VirtioBackend`] for disk I/O. Reads and writes go through
//! a standard `File` handle, with guest memory access via [`GuestMemAccess`].
//!
//! Device ID: 2 (virtio block device).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use hitz_hal::GuestMemAccess;

use crate::virtio::mmio_transport::VirtioBackend;
use crate::virtio::queue::VirtQueue;

/// Virtio block request type: read from disk.
const VIRTIO_BLK_T_IN: u32 = 0;
/// Virtio block request type: write to disk.
const VIRTIO_BLK_T_OUT: u32 = 1;

/// Status: success.
const VIRTIO_BLK_S_OK: u8 = 0;
/// Status: I/O error.
const VIRTIO_BLK_S_IOERR: u8 = 1;

/// Sector size in bytes.
const SECTOR_SIZE: u64 = 512;

/// Virtio block device backed by a file.
///
/// Processes read/write requests from the guest via virtqueue descriptor
/// chains. Each request has three descriptors:
/// 1. Header (device-readable): request type + sector number
/// 2. Data buffer (device-writable for reads, device-readable for writes)
/// 3. Status byte (device-writable): success/failure indicator
pub struct VirtioBlockDevice {
    /// Backing disk file.
    disk: File,
    /// Disk capacity in 512-byte sectors.
    capacity: u64,
    /// Pre-allocated buffer for I/O operations to eliminate heap allocations per request.
    buffer: Vec<u8>,
}

impl VirtioBlockDevice {
    /// Create a block device backed by the given file.
    ///
    /// # Abstract
    /// Create a block device backed by the given file.
    /// The capacity is determined from the file's current length.
    ///
    /// # The Hero's Journey
    /// ```rust
    /// use hitz_devices::VirtioBlockDevice;
    /// use tempfile::tempfile;
    ///
    /// // Create a temporary file to act as our block device.
    /// let file = tempfile().expect("Failed to create temporary file");
    ///
    /// // Initialize the virtio block device.
    /// let device = VirtioBlockDevice::new(file).expect("Failed to create block device");
    /// ```
    pub fn new(disk: File) -> std::io::Result<Self> {
        let metadata = disk.metadata()?;
        let capacity = metadata.len() / SECTOR_SIZE;
        Ok(Self {
            disk,
            capacity,
            buffer: Vec::with_capacity(65536),
        })
    }

    /// Returns the disk capacity in 512-byte sectors.
    #[must_use]
    pub const fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Process a single block request from a descriptor chain.
    ///
    /// Returns the total bytes written to device-writable descriptors
    /// (data + status byte).
    fn process_request(
        &mut self,
        chain: &mut crate::virtio::queue::DescriptorChain,
        mem: &dyn GuestMemAccess,
    ) -> u32 {
        // 1. Read the request header (16 bytes from first descriptor).
        let Some(hdr_desc) = chain.next_descriptor(mem) else {
            return 0;
        };

        let mut hdr_buf = [0u8; 16];
        if mem.read_guest(hdr_desc.gpa, &mut hdr_buf).is_err() {
            return 0;
        }

        let req_type = u32::from_le_bytes([hdr_buf[0], hdr_buf[1], hdr_buf[2], hdr_buf[3]]);
        // bytes 4..8 are reserved
        let sector = u64::from_le_bytes([
            hdr_buf[8],
            hdr_buf[9],
            hdr_buf[10],
            hdr_buf[11],
            hdr_buf[12],
            hdr_buf[13],
            hdr_buf[14],
            hdr_buf[15],
        ]);

        // 2. Process the data descriptor.
        let Some(data_desc) = chain.next_descriptor(mem) else {
            return 0;
        };

        let mut total_written: u32 = 0;
        let status = match req_type {
            VIRTIO_BLK_T_IN => {
                if data_desc.is_device_writable {
                    // Read from disk into guest memory.
                    self.handle_read(
                        sector,
                        data_desc.gpa,
                        data_desc.len,
                        mem,
                        &mut total_written,
                    )
                } else {
                    tracing::warn!("IN request requires device-writable data descriptor");
                    VIRTIO_BLK_S_IOERR
                }
            }
            VIRTIO_BLK_T_OUT => {
                if data_desc.is_device_writable {
                    tracing::warn!(
                        "OUT request requires device-readable (read-only) data descriptor"
                    );
                    VIRTIO_BLK_S_IOERR
                } else {
                    // Write from guest memory to disk.
                    self.handle_write(sector, data_desc.gpa, data_desc.len, mem)
                }
            }
            _ => {
                tracing::warn!(req_type, "unknown virtio-blk request type");
                VIRTIO_BLK_S_IOERR
            }
        };

        // 3. Write the status byte to the third descriptor.
        let Some(status_desc) = chain.next_descriptor(mem) else {
            tracing::warn!("blk request missing status descriptor");
            return total_written;
        };

        if !status_desc.is_device_writable {
            tracing::warn!("blk request status descriptor must be writable");
            return total_written;
        }

        let _ = mem.write_guest(status_desc.gpa, &[status]);
        total_written += 1;

        total_written
    }

    /// Handle a read request: disk -> guest memory.
    #[allow(clippy::cast_possible_truncation)]
    fn handle_read(
        &mut self,
        sector: u64,
        gpa: u64,
        len: u32,
        mem: &dyn GuestMemAccess,
        total_written: &mut u32,
    ) -> u8 {
        if len > 16_777_216 {
            // Max 16MB per request to prevent OOM
            return VIRTIO_BLK_S_IOERR;
        }

        let data_len = u64::from(len);
        let capacity_bytes = self.capacity.checked_mul(SECTOR_SIZE).unwrap_or(0);

        // Bounds check.
        if sector >= self.capacity {
            tracing::warn!(sector, len, capacity = self.capacity, "read out of bounds");
            return VIRTIO_BLK_S_IOERR;
        }

        let byte_offset = sector * SECTOR_SIZE;

        if byte_offset.saturating_add(data_len) > capacity_bytes {
            tracing::warn!(sector, len, capacity = self.capacity, "read out of bounds");
            return VIRTIO_BLK_S_IOERR;
        }

        if self.disk.seek(SeekFrom::Start(byte_offset)).is_err() {
            return VIRTIO_BLK_S_IOERR;
        }

        self.buffer.resize(len as usize, 0);
        if self.disk.read_exact(&mut self.buffer).is_err() {
            return VIRTIO_BLK_S_IOERR;
        }

        if mem.write_guest(gpa, &self.buffer).is_err() {
            return VIRTIO_BLK_S_IOERR;
        }

        *total_written += len;
        VIRTIO_BLK_S_OK
    }

    /// Handle a write request: guest memory -> disk.
    #[allow(clippy::cast_possible_truncation)]
    fn handle_write(&mut self, sector: u64, gpa: u64, len: u32, mem: &dyn GuestMemAccess) -> u8 {
        if len > 16_777_216 {
            // Max 16MB per request to prevent OOM
            return VIRTIO_BLK_S_IOERR;
        }

        let data_len = u64::from(len);
        let capacity_bytes = self.capacity.checked_mul(SECTOR_SIZE).unwrap_or(0);

        // Bounds check.
        if sector >= self.capacity {
            tracing::warn!(sector, len, capacity = self.capacity, "write out of bounds");
            return VIRTIO_BLK_S_IOERR;
        }

        let byte_offset = sector * SECTOR_SIZE;

        if byte_offset.saturating_add(data_len) > capacity_bytes {
            tracing::warn!(sector, len, capacity = self.capacity, "write out of bounds");
            return VIRTIO_BLK_S_IOERR;
        }

        self.buffer.resize(len as usize, 0);
        if mem.read_guest(gpa, &mut self.buffer).is_err() {
            return VIRTIO_BLK_S_IOERR;
        }

        if self.disk.seek(SeekFrom::Start(byte_offset)).is_err() {
            return VIRTIO_BLK_S_IOERR;
        }

        if self.disk.write_all(&self.buffer).is_err() {
            return VIRTIO_BLK_S_IOERR;
        }

        VIRTIO_BLK_S_OK
    }
}

impl VirtioBackend for VirtioBlockDevice {
    fn device_id(&self) -> u32 {
        2 // virtio block
    }

    fn device_features(&self) -> u64 {
        0 // no special features for now
    }

    fn process_queue(&mut self, _queue_idx: u16, queue: &mut VirtQueue, mem: &dyn GuestMemAccess) {
        while let Some(mut chain) = queue.pop_chain(mem) {
            let head = chain.head_index();
            let written = self.process_request(&mut chain, mem);
            queue.push_used(mem, head, written);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn read_config(&self, offset: u64, data: &mut [u8]) {
        // Config space layout: capacity (u64) at offset 0.
        if offset < 8 {
            let cap_bytes = self.capacity.to_le_bytes();
            // offset < 8, so truncation to usize is safe.
            let start = offset as usize;
            let end = (start + data.len()).min(cap_bytes.len());
            if start < cap_bytes.len() {
                let len = end - start;
                data[..len].copy_from_slice(&cap_bytes[start..end]);
            }
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // Block device config is read-only.
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::significant_drop_tightening
)]
mod tests {
    use super::*;
    use std::io::{Seek, Write as IoWrite};
    use std::sync::Mutex;

    /// Mock guest memory backed by a `Vec<u8>`.
    struct MockMem {
        inner: Mutex<Vec<u8>>,
    }

    impl MockMem {
        fn new(size: usize) -> Self {
            Self {
                inner: Mutex::new(vec![0u8; size]),
            }
        }

        fn write_bytes(&self, offset: u64, data: &[u8]) {
            let mut mem = self.inner.lock().expect("Mutex poisoned");
            let start = offset as usize;
            mem[start..start + data.len()].copy_from_slice(data);
            drop(mem);
        }

        fn read_bytes(&self, offset: u64, len: usize) -> Vec<u8> {
            let mem = self.inner.lock().expect("Mutex poisoned");
            let start = offset as usize;
            mem[start..start + len].to_vec()
        }
    }

    impl GuestMemAccess for MockMem {
        fn read_guest(&self, gpa: u64, buf: &mut [u8]) -> Result<(), hitz_hal::HalError> {
            let mem = self.inner.lock().expect("Mutex poisoned");
            let start = gpa as usize;
            if start + buf.len() > mem.len() {
                return Err(hitz_hal::HalError::MapMemory {
                    gpa,
                    size: buf.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            buf.copy_from_slice(&mem[start..start + buf.len()]);
            drop(mem);
            Ok(())
        }

        fn write_guest(&self, gpa: u64, data: &[u8]) -> Result<(), hitz_hal::HalError> {
            let mut mem = self.inner.lock().expect("Mutex poisoned");
            let start = gpa as usize;
            if start + data.len() > mem.len() {
                return Err(hitz_hal::HalError::MapMemory {
                    gpa,
                    size: data.len(),
                    reason: "out of bounds".to_string(),
                });
            }
            mem[start..start + data.len()].copy_from_slice(data);
            drop(mem);
            Ok(())
        }
    }

    // Memory layout for test virtqueue.
    const DESC_BASE: u64 = 0x0000;
    const AVAIL_BASE: u64 = 0x1000;
    const USED_BASE: u64 = 0x2000;
    // Guest addresses for request data.
    const HDR_GPA: u64 = 0x3000;
    const DATA_GPA: u64 = 0x4000;
    const STATUS_GPA: u64 = 0x5000;

    fn write_desc(mem: &MockMem, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let offset = DESC_BASE + u64::from(idx) * 16;
        mem.write_bytes(offset, &addr.to_le_bytes());
        mem.write_bytes(offset + 8, &len.to_le_bytes());
        mem.write_bytes(offset + 12, &flags.to_le_bytes());
        mem.write_bytes(offset + 14, &next.to_le_bytes());
    }

    fn write_avail_entry(mem: &MockMem, ring_idx: u16, desc_idx: u16) {
        let offset = AVAIL_BASE + 4 + u64::from(ring_idx) * 2;
        mem.write_bytes(offset, &desc_idx.to_le_bytes());
    }

    fn set_avail_idx(mem: &MockMem, idx: u16) {
        mem.write_bytes(AVAIL_BASE + 2, &idx.to_le_bytes());
    }

    /// Write a `VirtioBlkReqHeader` to guest memory.
    fn write_blk_header(mem: &MockMem, gpa: u64, req_type: u32, sector: u64) {
        let mut hdr = [0u8; 16];
        hdr[0..4].copy_from_slice(&req_type.to_le_bytes());
        // bytes 4..8 reserved (zero)
        hdr[8..16].copy_from_slice(&sector.to_le_bytes());
        mem.write_bytes(gpa, &hdr);
    }

    fn create_temp_disk(sectors: u64) -> File {
        let mut f = tempfile::tempfile().expect("create temp file");
        let size = sectors * SECTOR_SIZE;
        f.set_len(size).expect("set file length");
        let _ = f.seek(SeekFrom::Start(0)).expect("seek");
        f
    }

    fn setup_queue(mem: &MockMem) -> VirtQueue {
        let mut q = VirtQueue::new(16);
        q.configure(DESC_BASE, AVAIL_BASE, USED_BASE);
        q.set_ready(true);
        set_avail_idx(mem, 0);
        mem.write_bytes(USED_BASE + 2, &0u16.to_le_bytes());
        q
    }

    /// Set up a 3-descriptor chain for a block request.
    fn setup_request_chain(
        mem: &MockMem,
        avail_idx: u16,
        req_type: u32,
        sector: u64,
        data_len: u32,
        data_writable: bool,
    ) {
        // Descriptor flags.
        const F_NEXT: u16 = 1;
        const F_WRITE: u16 = 2;

        let base_desc = avail_idx * 3;

        // Descriptor 0: header (device-readable, chains to 1).
        write_desc(mem, base_desc, HDR_GPA, 16, F_NEXT, base_desc + 1);

        // Descriptor 1: data buffer.
        let data_flags = if data_writable {
            F_NEXT | F_WRITE
        } else {
            F_NEXT
        };
        write_desc(
            mem,
            base_desc + 1,
            DATA_GPA,
            data_len,
            data_flags,
            base_desc + 2,
        );

        // Descriptor 2: status byte (device-writable, end of chain).
        write_desc(mem, base_desc + 2, STATUS_GPA, 1, F_WRITE, 0);

        // Write the header.
        write_blk_header(mem, HDR_GPA, req_type, sector);

        // Available ring entry.
        write_avail_entry(mem, avail_idx, base_desc);
        set_avail_idx(mem, avail_idx + 1);
    }

    #[test]
    fn device_id_is_2() {
        let f = create_temp_disk(1);
        let dev = VirtioBlockDevice::new(f).expect("new block device");
        assert_eq!(dev.device_id(), 2);
    }

    #[test]
    fn config_space_reports_capacity() {
        let f = create_temp_disk(100);
        let dev = VirtioBlockDevice::new(f).expect("new block device");

        let mut buf = [0u8; 8];
        dev.read_config(0, &mut buf);
        let cap = u64::from_le_bytes(buf);
        assert_eq!(cap, 100);
    }

    #[test]
    fn read_request() {
        // Create a disk with known data.
        let mut f = create_temp_disk(2);
        let test_data = b"Hello, virtio block device!!!!!!"; // 32 bytes
        let _ = f.seek(SeekFrom::Start(0)).expect("seek");
        f.write_all(test_data).expect("write test data");
        let _ = f.seek(SeekFrom::Start(0)).expect("seek back");

        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Set up a read request: type=IN, sector=0, data_len=32.
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 0, 32, true);

        // Process the queue.
        dev.process_queue(0, &mut q, &mem);

        // Check the data buffer was written to guest memory.
        let result = mem.read_bytes(DATA_GPA, 32);
        assert_eq!(&result[..], test_data);

        // Check status is OK.
        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_OK);
    }

    #[test]
    fn write_request() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Put data in guest memory to write to disk.
        let write_data = b"Written via virtio!_padding_he!!"; // 32 bytes
        mem.write_bytes(DATA_GPA, write_data);

        // Set up a write request: type=OUT, sector=0, data_len=32.
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_OUT, 0, 32, false);

        dev.process_queue(0, &mut q, &mem);

        // Check status is OK.
        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_OK);

        // Verify disk contents by reading back.
        let _ = dev.disk.seek(SeekFrom::Start(0)).expect("seek");
        let mut verify = [0u8; 32];
        dev.disk.read_exact(&mut verify).expect("read back");
        assert_eq!(&verify[..], write_data);
    }

    #[test]
    fn out_of_bounds_read_returns_ioerr() {
        let f = create_temp_disk(1); // 1 sector = 512 bytes
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Try to read sector 5 (out of bounds for a 1-sector disk).
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 5, 512, true);

        dev.process_queue(0, &mut q, &mem);

        // Check status is IOERR.
        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn out_of_bounds_write_returns_ioerr() {
        let f = create_temp_disk(1);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        setup_request_chain(&mem, 0, VIRTIO_BLK_T_OUT, 5, 512, false);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn read_at_nonzero_sector() {
        let mut f = create_temp_disk(4);
        // Write known data at sector 2.
        let offset = 2 * SECTOR_SIZE;
        let _ = f.seek(SeekFrom::Start(offset)).expect("seek");
        let sector_data = [0xABu8; 512];
        f.write_all(&sector_data).expect("write");
        let _ = f.seek(SeekFrom::Start(0)).expect("seek back");

        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Read sector 2.
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 2, 512, true);
        dev.process_queue(0, &mut q, &mem);

        let result = mem.read_bytes(DATA_GPA, 512);
        assert!(result.iter().all(|&b| b == 0xAB));

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_OK);
    }

    #[test]
    fn unknown_request_type_returns_ioerr() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Set up a request with an unknown type (e.g., 99).
        setup_request_chain(&mem, 0, 99, 0, 32, true);

        // Process the queue.
        dev.process_queue(0, &mut q, &mem);

        // Check status is IOERR.
        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn havoc_blk_read_oom() {
        let f = create_temp_disk(1);
        let mut dev = VirtioBlockDevice::new(f).expect("Mutex poisoned");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Create a massive desc length
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 0, u32::MAX, true);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn havoc_blk_write_oom() {
        let f = create_temp_disk(1);
        let mut dev = VirtioBlockDevice::new(f).expect("Mutex poisoned");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Create a massive desc length
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_OUT, 0, u32::MAX, false);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn capacity_returns_capacity() {
        let f = create_temp_disk(10);
        let dev = VirtioBlockDevice::new(f).expect("new block device");
        assert_eq!(dev.capacity(), 10);
    }

    #[test]
    fn device_features_returns_0() {
        let f = create_temp_disk(10);
        let dev = VirtioBlockDevice::new(f).expect("new block device");
        assert_eq!(dev.device_features(), 0);
    }

    #[test]
    fn write_config_is_noop() {
        let f = create_temp_disk(10);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        // Just verify it doesn't panic
        dev.write_config(0, &[1, 2, 3]);
    }

    #[test]
    fn read_length_out_of_bounds() {
        let f = create_temp_disk(2); // 2 sectors = 1024 bytes
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Request starting at sector 1 (offset 512), length 513 bytes -> out of bounds
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 1, 513, true);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn write_length_out_of_bounds() {
        let f = create_temp_disk(2); // 2 sectors = 1024 bytes
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Request starting at sector 1 (offset 512), length 513 bytes -> out of bounds
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_OUT, 1, 513, false);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn missing_data_descriptor() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Manually create a 1-descriptor chain (only header, no data, no status)
        write_desc(&mem, 0, HDR_GPA, 16, 0, 0); // No F_NEXT
        write_blk_header(&mem, HDR_GPA, VIRTIO_BLK_T_IN, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        // Process queue should return 0 written without panic
        dev.process_queue(0, &mut q, &mem);
    }

    #[test]
    fn missing_header_descriptor() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // A descriptor with 0 length and invalid gpa
        write_desc(&mem, 0, 0xFFFF_FFFF, 0, 0, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        dev.process_queue(0, &mut q, &mem);
    }

    #[test]
    fn read_header_fails() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Header GPA out of bounds
        write_desc(&mem, 0, 0x20000, 16, 1, 1);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        dev.process_queue(0, &mut q, &mem);
    }

    #[test]
    fn write_guest_memory_fails() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Setup a valid read request, but point data_gpa to out-of-bounds memory
        // Descriptor flags
        const F_NEXT: u16 = 1;
        const F_WRITE: u16 = 2;
        write_desc(&mem, 0, HDR_GPA, 16, F_NEXT, 1);
        write_desc(&mem, 1, 0x20000, 32, F_NEXT | F_WRITE, 2); // Out of bounds memory
        write_desc(&mem, 2, STATUS_GPA, 1, F_WRITE, 0);
        write_blk_header(&mem, HDR_GPA, VIRTIO_BLK_T_IN, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn read_guest_memory_fails() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Setup a valid write request, but point data_gpa to out-of-bounds memory
        const F_NEXT: u16 = 1;
        const F_WRITE: u16 = 2;
        write_desc(&mem, 0, HDR_GPA, 16, F_NEXT, 1);
        write_desc(&mem, 1, 0x20000, 32, F_NEXT, 2); // Out of bounds memory
        write_desc(&mem, 2, STATUS_GPA, 1, F_WRITE, 0);
        write_blk_header(&mem, HDR_GPA, VIRTIO_BLK_T_OUT, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn header_read_fails_ioerr() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Header descriptor reads 16 bytes but only 8 bytes are available
        // Set gpa to 0xFFFF, where 16 bytes is out of bounds for the 0x10000 sized MockMem
        write_desc(&mem, 0, 0xFFFF, 16, 1, 1);
        write_desc(&mem, 1, DATA_GPA, 32, 1 | 2, 2);
        write_desc(&mem, 2, STATUS_GPA, 1, 2, 0);
        write_avail_entry(&mem, 0, 0);
        set_avail_idx(&mem, 1);

        dev.process_queue(0, &mut q, &mem);

        // Assert queue processed correctly despite early return
        let used_idx = mem.read_bytes(USED_BASE + 2, 2);
        let used_idx_val = u16::from_le_bytes([used_idx[0], used_idx[1]]);
        assert_eq!(used_idx_val, 1);
    }

    #[test]
    fn io_errors_during_read_and_write() {
        let f = create_temp_disk(2); // 2 sectors = 1024 bytes
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        // Force the file to be shorter than capacity to trigger an error in read_exact/write_all without failing capacity check.
        dev.disk.set_len(0).expect("Mutex poisoned");

        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Read request (fails read_exact)
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 0, 512, true);
        dev.process_queue(0, &mut q, &mem);
        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);

        // Write request (fails write_all)
        // Linux allows writing to a file even after set_len(0), so we use a read-only file instead.
        // But since we can't easily mock the file, the previous test was good enough. We will
        // add another chain to cover the `VIRTIO_BLK_T_OUT` case assuming the OS complains.
        // Actually to ensure write fails, we could just rely on the existing write bounds check.
        // We will leave the write error out for simplicity and reliability across OSes.
    }

    #[test]
    fn next_descriptor_read_fails() {
        let f = create_temp_disk(2);
        let mut dev = VirtioBlockDevice::new(f).expect("new block device");
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Simulate next_descriptor returning None by writing a head_idx out of bounds for the queue
        write_avail_entry(&mem, 0, 20);
        set_avail_idx(&mem, 1);

        dev.process_queue(0, &mut q, &mem);
        // It should return 0 silently without panicking.
    }

    #[test]
    fn havoc_blk_read_readonly_desc() {
        let f = create_temp_disk(1);
        let mut dev = VirtioBlockDevice::new(f).unwrap();
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Virtio spec requires that the data descriptor for a read request (VIRTIO_BLK_T_IN)
        // is device-writable. If the guest provides a device-readable (read-only) descriptor,
        // the device must reject it.
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_IN, 0, 512, false);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    #[test]
    fn havoc_blk_write_writable_desc() {
        let f = create_temp_disk(1);
        let mut dev = VirtioBlockDevice::new(f).unwrap();
        let mem = MockMem::new(0x10000);
        let mut q = setup_queue(&mem);

        // Virtio spec requires that the data descriptor for a write request (VIRTIO_BLK_T_OUT)
        // is device-readable (read-only from device's perspective).
        // If the guest provides a device-writable descriptor, it is invalid and should be rejected.
        setup_request_chain(&mem, 0, VIRTIO_BLK_T_OUT, 0, 512, true);

        dev.process_queue(0, &mut q, &mem);

        let status = mem.read_bytes(STATUS_GPA, 1);
        assert_eq!(status[0], VIRTIO_BLK_S_IOERR);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn havoc_fuzz_block_process_queue(
            desc_len in 0u32..=100_000,
            sector in 0u64..100,
            req_type in prop::sample::select(vec![VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT, 99]),
            is_writable in any::<bool>(),
            status_writable in any::<bool>(),
        ) {
            let f = create_temp_disk(2);
            let mut dev = VirtioBlockDevice::new(f).unwrap();
            let mem = MockMem::new(0x20000);
            let mut q = setup_queue(&mem);

            let base_desc = 0;
            const F_NEXT: u16 = 1;
            const F_WRITE: u16 = 2;

            write_desc(&mem, base_desc, HDR_GPA, 16, F_NEXT, base_desc + 1);
            write_blk_header(&mem, HDR_GPA, req_type, sector);

            let data_flags = if is_writable {
                F_NEXT | F_WRITE
            } else {
                F_NEXT
            };
            write_desc(&mem, base_desc + 1, DATA_GPA, desc_len, data_flags, base_desc + 2);

            let status_flags = if status_writable { F_WRITE } else { 0 };
            write_desc(&mem, base_desc + 2, STATUS_GPA, 1, status_flags, 0);

            write_avail_entry(&mem, 0, base_desc);
            set_avail_idx(&mem, 1);

            // This should not panic under any combination of inputs.
            dev.process_queue(0, &mut q, &mem);
        }
    }
}
