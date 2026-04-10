//! Shared ring buffer for serial output capture.
//!
//! # Abstract
//! [`SerialBuf`] implements [`std::io::Write`] so it can serve as the output
//! sink for `hitz_devices::serial::SerialDevice<SerialBuf>`. Multiple
//! independent [`SerialReader`]s can be spawned from a single buffer; each
//! tracks its own read position and is woken via [`tokio::sync::Notify`]
//! when new data arrives (or the buffer is closed).
//!
//! # The Hero's Journey
//! ```
//! # use hitz_vmm::serial_buf::SerialBuf;
//! # use std::io::Write;
//! // 1. Create a new serial buffer.
//! let mut buf = SerialBuf::new();
//!
//! // 2. Spawn a reader to consume the output.
//! let mut reader = buf.reader();
//!
//! // 3. Write data to the buffer.
//! buf.write_all(b"Booting hitz VM...").unwrap();
//!
//! // 4. The reader can then pull bytes out.
//! # tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
//! let bytes = reader.read_chunk().await.unwrap();
//! assert_eq!(&bytes, b"Booting hitz VM...");
//! # });
//! ```
//!
//! # The Fine Print
//! * **Overwriting**: When the ring is full, the oldest bytes are silently overwritten.
//! * **Closing**: Once `close()` is called, all new readers and pending `read_next` calls will return `None`.

use std::io::{self, Write};

#[cfg(loom)]
use loom::sync::{Arc, Mutex};
#[cfg(not(loom))]
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

/// Default ring buffer capacity (64 KiB).
const DEFAULT_CAPACITY: usize = 64 * 1024;

/// Shared interior of the ring buffer.
struct Inner {
    /// Fixed-size backing store.
    buf: Vec<u8>,
    /// Next write position (mod capacity).
    write_pos: usize,
    /// Monotonically increasing count of bytes written since creation.
    total_written: u64,
    /// When `true`, all pending readers should return `None`.
    closed: bool,
}

/// A clonable, shared ring buffer that implements [`Write`].
///
/// Create one with [`SerialBuf::new`] (or [`with_capacity`](Self::with_capacity)),
/// then spawn readers via [`reader`](Self::reader).  When the VM shuts down,
/// call [`close`](Self::close) so readers drain and terminate.
#[derive(Clone)]
pub struct SerialBuf {
    inner: Arc<Mutex<Inner>>,
    notify: Arc<Notify>,
}

impl SerialBuf {
    /// Create a ring buffer with the default capacity (64 KiB).
    ///
    /// # Examples
    /// ```
    /// # use hitz_vmm::serial_buf::SerialBuf;
    /// let mut buf = SerialBuf::new();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a ring buffer with an explicit capacity in bytes.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be greater than 0");
        Self {
            inner: Arc::new(Mutex::new(Inner {
                buf: vec![0u8; capacity],
                write_pos: 0,
                total_written: 0,
                closed: false,
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Spawn an independent reader starting from the current write position.
    ///
    /// Each reader maintains its own cursor and can be polled independently.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0.
    #[must_use]
    pub fn reader(&self) -> SerialReader {
        #[cfg(not(loom))]
        let read_pos = self.inner.lock().map_or(0, |i| i.total_written);
        #[cfg(loom)]
        let read_pos = self.inner.lock().total_written;
        SerialReader {
            inner: self.inner.clone(),
            notify: self.notify.clone(),
            read_pos,
        }
    }

    /// Mark the buffer as closed.
    ///
    /// All current and future [`SerialReader::read_chunk`] calls will return
    /// `None` once they have drained any remaining data.
    pub fn close(&self) {
        #[cfg(not(loom))]
        if let Ok(mut inner) = self.inner.lock() {
            inner.closed = true;
        }
        #[cfg(loom)]
        {
            let mut inner = self.inner.lock();
            inner.closed = true;
        }
        self.notify.notify_waiters();
    }
}

impl Default for SerialBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for SerialBuf {
    /// Writes data to the circular buffer.
    ///
    /// ⚡ Bolt Optimization:
    /// Instead of writing byte-by-byte in a loop, this implementation uses
    /// `copy_from_slice` for bulk memory copies. This eliminates repetitive bounds
    /// checking and loop overhead, turning a linear O(N) operation into O(1)
    /// (or O(2) if the write wraps around the end of the ring).
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        #[cfg(not(loom))]
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("device lock poisoned"))?;
        #[cfg(loom)]
        let mut inner = self.inner.lock();

        let cap = inner.buf.len();
        let len = data.len();

        let data_to_write = if len > cap { &data[len - cap..] } else { data };
        let write_len = data_to_write.len();

        // If we skipped bytes because len > cap, the starting write position
        // in the ring buffer must mathematically advance to simulate those
        // skipped writes.
        let skipped = len.saturating_sub(cap);
        let pos = (inner.write_pos + skipped) % cap;

        let space_to_end = cap - pos;
        if write_len <= space_to_end {
            // The new data fits entirely before the physical end of the buffer.
            inner.buf[pos..pos + write_len].copy_from_slice(data_to_write);
            inner.write_pos = (pos + write_len) % cap;
        } else {
            // The new data wraps around the end of the buffer.
            inner.buf[pos..].copy_from_slice(&data_to_write[..space_to_end]);
            let remaining = write_len - space_to_end;
            inner.buf[..remaining].copy_from_slice(&data_to_write[space_to_end..]);
            inner.write_pos = remaining;
        }
        inner.total_written += len as u64;
        drop(inner);
        self.notify.notify_waiters();
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// An independent async reader over a [`SerialBuf`].
///
/// Each reader tracks its own position.  If the writer laps a slow reader,
/// the reader skips ahead to the oldest available data.
pub struct SerialReader {
    inner: Arc<Mutex<Inner>>,
    notify: Arc<Notify>,
    /// Monotonic byte offset this reader has consumed up to.
    read_pos: u64,
}

impl SerialReader {
    /// Wait for and return the next chunk of serial output.
    ///
    /// Returns `None` when the buffer has been [`closed`](SerialBuf::close)
    /// and all remaining data has been drained.
    pub async fn read_chunk(&mut self) -> Option<Vec<u8>> {
        loop {
            let notified = self.notify.notified();
            {
                #[cfg(not(loom))]
                let inner = match self.inner.lock() {
                    Ok(guard) => guard,
                    Err(_) => return None,
                };
                #[cfg(loom)]
                let inner = self.inner.lock();
                if self.read_pos < inner.total_written {
                    let available = inner.total_written - self.read_pos;
                    let cap = inner.buf.len();

                    // If the writer has lapped us, skip to the oldest byte
                    // still in the ring.
                    if available > cap as u64 {
                        self.read_pos = inner.total_written - cap as u64;
                    }

                    // Safe: after the lap check above, the delta is at most `cap`
                    // which is a `usize`, so truncation cannot happen.
                    #[allow(clippy::cast_possible_truncation)]
                    let to_read = (inner.total_written - self.read_pos) as usize;

                    let mut result = Vec::with_capacity(to_read);

                    // If `to_read` is 0, we shouldn't attempt to read.
                    if to_read > 0 {
                        // `start` is where the un-read data begins inside the ring.
                        let start = (inner.write_pos + cap - (to_read % cap)) % cap;
                        if start + to_read <= cap {
                            result.extend_from_slice(&inner.buf[start..start + to_read]);
                        } else {
                            result.extend_from_slice(&inner.buf[start..]);
                            result.extend_from_slice(&inner.buf[..to_read - (cap - start)]);
                        }
                    }

                    self.read_pos = inner.total_written;
                    return Some(result);
                }

                if inner.closed {
                    return None;
                }
            }
            // Park until the writer pushes more data or closes.
            notified.await;
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Helper: build a single-threaded tokio runtime for unit tests.
    fn test_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
    }

    #[test]
    fn write_and_read_basic() {
        let rt = test_rt();
        rt.block_on(async {
            let mut buf = SerialBuf::new();
            let mut reader = buf.reader();

            buf.write_all(b"hello").expect("write");

            let chunk = tokio::time::timeout(Duration::from_millis(100), reader.read_chunk())
                .await
                .expect("timeout")
                .expect("should not be None");

            assert_eq!(chunk, b"hello");
        });
    }

    #[test]
    fn ring_buffer_wraps() {
        let rt = test_rt();
        rt.block_on(async {
            let cap = 16;
            let mut buf = SerialBuf::with_capacity(cap);
            let mut reader = buf.reader();

            // Write more than the capacity so the ring wraps.
            let data: Vec<u8> = (0..32).collect();
            buf.write_all(&data).expect("write");

            let chunk = tokio::time::timeout(Duration::from_millis(100), reader.read_chunk())
                .await
                .expect("timeout")
                .expect("should not be None");

            // The reader should get only the last `cap` bytes.
            assert_eq!(chunk.len(), cap);
            assert_eq!(chunk, &data[cap..]);
        });
    }

    #[test]
    fn close_wakes_reader() {
        let rt = test_rt();
        rt.block_on(async {
            let buf = SerialBuf::new();
            let mut reader = buf.reader();

            // Close without writing anything.
            buf.close();

            let result = tokio::time::timeout(Duration::from_millis(100), reader.read_chunk())
                .await
                .expect("timeout");

            assert!(result.is_none(), "closed buffer should return None");
        });
    }

    #[test]
    fn multiple_readers() {
        let rt = test_rt();
        rt.block_on(async {
            let mut buf = SerialBuf::new();
            let mut r1 = buf.reader();
            let mut r2 = buf.reader();

            buf.write_all(b"shared").expect("write");

            let c1 = tokio::time::timeout(Duration::from_millis(100), r1.read_chunk())
                .await
                .expect("timeout")
                .expect("r1 should not be None");

            let c2 = tokio::time::timeout(Duration::from_millis(100), r2.read_chunk())
                .await
                .expect("timeout")
                .expect("r2 should not be None");

            assert_eq!(c1, b"shared");
            assert_eq!(c2, b"shared");
        });
    }

    #[test]
    fn ring_buffer_exact_lap() {
        let rt = test_rt();
        rt.block_on(async {
            let cap = 16;
            let mut buf = SerialBuf::with_capacity(cap);
            let mut reader = buf.reader();

            // Write exactly the capacity.
            let data: Vec<u8> = (0..16).collect();
            buf.write_all(&data).expect("write");

            let chunk = tokio::time::timeout(Duration::from_millis(100), reader.read_chunk())
                .await
                .expect("timeout")
                .expect("should not be None");

            assert_eq!(chunk.len(), cap);
            assert_eq!(chunk, data);

            // Write exact capacity again
            buf.write_all(&data).expect("write");

            let chunk2 = tokio::time::timeout(Duration::from_millis(100), reader.read_chunk())
                .await
                .expect("timeout")
                .expect("should not be None");

            assert_eq!(chunk2.len(), cap);
            assert_eq!(chunk2, data);
        });
    }
}
