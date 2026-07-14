import re

with open('crates/hitz-devices/src/virtio/queue.rs', 'r') as f:
    content = f.read()

replacement = """/// and whether it's writable by the device.
///
/// # The Hero's Journey
/// ```
/// use hitz_devices::virtio::VirtQueue;
///
/// // Note: Descriptor is returned by VirtQueue::next_descriptor()
/// // Descriptors are typically returned by the chain via the VirtQueue.
/// // They are simple structs containing GPA and length.
/// // Example of what a descriptor would look like when populated:
/// // let desc = Descriptor { gpa: 0x1000, len: 4096, is_device_writable: true };
/// // assert_eq!(desc.len, 4096);
/// ```"""

content = re.sub(r"/// and whether it's writable by the device\.\n///\n/// # The Hero's Journey\n/// ```\n/// use hitz_devices::virtio::VirtQueue;\n///\n/// // Note: Descriptor is returned by VirtQueue::next_descriptor\(\)\n/// // Descriptors are typically returned by the chain via the VirtQueue\.\n/// // They are simple structs containing GPA and length\.\n/// // Example of what a descriptor would look like when populated:\n/// # let desc = hitz_devices::virtio::queue::Descriptor { gpa: 0x1000, len: 4096, is_device_writable: true };\n/// # assert_eq!\(desc\.len, 4096\);\n/// ```", replacement, content)

with open('crates/hitz-devices/src/virtio/queue.rs', 'w') as f:
    f.write(content)
