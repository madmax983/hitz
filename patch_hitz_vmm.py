import sys

def modify_file(filepath, replacements):
    with open(filepath, 'r') as f:
        content = f.read()

    for search_str, replace_str in replacements:
        content = content.replace(search_str, replace_str)

    with open(filepath, 'w') as f:
        f.write(content)

modify_file('crates/hitz-vmm/src/lib.rs', [
    ("""pub(crate) mod run_loop;""", """pub mod run_loop;""")
])

modify_file('crates/hitz-vmm/src/run_loop.rs', [
    ("""        let mmio = hitz_hal::MmioExit {
            gpa: hitz_hal::Gpa::new(0x1000),
            data: [0; 8],
            len: 4,
            is_write: false,
            instruction_bytes: [0; 16],
            instruction_byte_count: 2,
        };""", """        let mmio = hitz_hal::MmioExit {
            gpa: 0x1000,
            data: [0; 8],
            len: 4,
            is_write: false,
            instruction_len: 2,
        };""")
])
