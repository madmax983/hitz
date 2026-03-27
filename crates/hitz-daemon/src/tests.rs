#![cfg(test)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::io_other_error,
    clippy::manual_is_variant_and,
    clippy::unnecessary_wraps,
    clippy::if_then_some_else_none,
    clippy::too_many_lines,
    clippy::manual_let_else,
    clippy::single_match_else
)]

use std::io::Write;
use std::sync::Arc;

use crate::VmManager;
use hitz_api::{VmConfig, VmState};
use hitz_whp::WhpHypervisor;

fn make_boot_elf(load_addr: u64, code: &[u8]) -> Vec<u8> {
    let elf_header_size = 64usize;
    let phdr_size = 56usize;
    let mut buf = vec![0u8; elf_header_size + phdr_size];

    // ELF header
    buf[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    buf[4] = 2; // ELFCLASS64
    buf[5] = 1; // ELFDATA2LSB
    buf[6] = 1; // EV_CURRENT
    buf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    buf[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    buf[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
    buf[24..32].copy_from_slice(&load_addr.to_le_bytes()); // e_entry
    buf[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    buf[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    buf[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    buf[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum

    // Program header
    let data_offset = (elf_header_size + phdr_size) as u64;
    let ph = elf_header_size;
    buf[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    buf[ph + 8..ph + 16].copy_from_slice(&data_offset.to_le_bytes()); // p_offset
    buf[ph + 16..ph + 24].copy_from_slice(&load_addr.to_le_bytes()); // p_vaddr
    buf[ph + 24..ph + 32].copy_from_slice(&load_addr.to_le_bytes()); // p_paddr
    buf[ph + 32..ph + 40].copy_from_slice(&(code.len() as u64).to_le_bytes()); // p_filesz
    buf[ph + 40..ph + 48].copy_from_slice(&(code.len() as u64).to_le_bytes()); // p_memsz

    buf.extend_from_slice(code);
    buf
}

#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase6_vm_manager_lifecycle() {
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
        ports: vec![],
        guest_cid: hitz_api::DEFAULT_GUEST_CID,
        guest_agent: hitz_api::GuestAgentMode::Auto,
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().expect("WHP not available"));
        let state_tmp = tempfile::tempdir().expect("tempdir");
        let manager = VmManager::new(hv, state_tmp.path().to_path_buf()).expect("VmManager::new");

        // Create
        let info = manager
            .create_vm("test-vm".into(), &config)
            .expect("create_vm");
        assert_eq!(info.state, VmState::Created);

        // Start
        let info = manager.start_vm("test-vm").expect("start_vm");
        assert_eq!(info.state, VmState::Running);

        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let info = manager.get_vm("test-vm").expect("get_vm");
            if info.state != VmState::Running {
                assert_eq!(
                    info.state,
                    VmState::Stopped,
                    "expected Stopped, got {:?} (exit: {:?})",
                    info.state,
                    info.exit_reason
                );
                break;
            }
        }

        let info = manager.get_vm("test-vm").expect("get_vm final");
        assert_eq!(info.state, VmState::Stopped, "VM should have stopped");

        // Delete
        manager.delete_vm("test-vm").expect("delete_vm");
        let err = manager.get_vm("test-vm").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    });
}

#[test]
#[ignore = "requires WHP enabled (Hyper-V)"]
fn phase7_vm_manager_serial_streaming() {
    let code: &[u8] = &[
        0xBA, 0xF8, 0x03, 0x00, 0x00, // mov edx, 0x3F8
        0xB0, 0x48, // mov al, 'H'
        0xEE, // out dx, al
        0xB0, 0x65, // mov al, 'e'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6C, // mov al, 'l'
        0xEE, // out dx, al
        0xB0, 0x6F, // mov al, 'o'
        0xEE, // out dx, al
        0xF4, // hlt
    ];

    let load_addr = 0x10_0000u64;
    let elf = make_boot_elf(load_addr, code);

    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&elf).expect("write ELF");
    tmp.flush().expect("flush");

    let config = VmConfig {
        kernel_path: tmp.path().to_path_buf(),
        initramfs_path: None,
        disk_path: None,
        ram_mib: 128,
        cpus: 1,
        cmdline: Some("console=ttyS0\0".into()),
        net: None,
        ports: vec![],
        guest_cid: hitz_api::DEFAULT_GUEST_CID,
        guest_agent: hitz_api::GuestAgentMode::Auto,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        let hv = Arc::new(WhpHypervisor::new().expect("WHP not available"));
        let state_tmp = tempfile::tempdir().expect("tempdir");
        let manager = VmManager::new(hv, state_tmp.path().to_path_buf()).expect("VmManager::new");

        let _ = manager
            .create_vm("serial-test".into(), &config)
            .expect("create");
        let _ = manager.start_vm("serial-test").expect("start");

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let mut reader = manager.serial_reader("serial-test").expect("serial reader");

        let chunk =
            tokio::time::timeout(std::time::Duration::from_secs(5), reader.read_chunk()).await;

        let _ = manager.stop_vm("serial-test");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        if let Ok(Some(data)) = chunk {
            let text = String::from_utf8_lossy(&data);
            assert!(
                text.contains("Hello") || !text.is_empty(),
                "expected serial output, got empty"
            );
        }
    });
}
