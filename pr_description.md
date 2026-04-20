🎯 Target: `hitz-vmm::run_loop` `handle_mmio` function.
💣 Risk: Undecodable MMIO instructions returning `None` might incorrectly fall back and cause a panic or advance RIP incorrectly if not verified via explicit test.
🧪 Strategy: Added `test_dispatch_exit_mmio_undecodable` unit test which feeds a block of `0xFF` instruction bytes representing a truly undecodable event sequence.
🔬 Verification: Run `cargo test -p hitz-vmm --lib --no-default-features --target x86_64-unknown-linux-gnu run_loop::tests::test_dispatch_exit_mmio_undecodable -- --exact`
