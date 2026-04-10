#![no_main]

use libfuzzer_sys::fuzz_target;
use hitz_vmm::mmio_decode::decode_mmio_instruction;

fuzz_target!(|data: &[u8]| {
    // 👺 Havoc fuzz target: send garbage data into the MMIO decoder
    // and make sure it never panics or OOMs.
    let _ = decode_mmio_instruction(data);
});
