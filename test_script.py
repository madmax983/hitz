with open("crates/hitz-vmm/src/run_loop.rs", "r") as f:
    content = f.read()

assert "fn test_handle_io_port_serial_write" in content
assert "fn test_handle_io_port_serial_read" in content
assert "fn test_handle_io_port_pic_write" in content
assert "fn test_handle_io_port_pic_read" in content
assert "fn test_handle_io_port_unhandled_read" in content
assert "fn test_handle_io_port_unhandled_write" in content
assert "fn test_dispatch_exit_halt" in content
assert "fn test_dispatch_exit_shutdown" in content

print("All tests added correctly.")
