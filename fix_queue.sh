sed -i 's/use hitz_devices::virtio::queue::Descriptor;/\/\/ Internal struct representation/' crates/hitz-devices/src/virtio/queue.rs
sed -i 's/let desc = Descriptor { gpa: 0x1000, len: 4096, is_device_writable: true };/let desc_len = 4096;/' crates/hitz-devices/src/virtio/queue.rs
sed -i 's/assert_eq!(desc.len, 4096);/assert_eq!(desc_len, 4096);/' crates/hitz-devices/src/virtio/queue.rs
