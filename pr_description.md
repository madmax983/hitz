📖 Chapter: The Core Execution & Initialization Modules (`hitz-vmm::cpio`, `hitz-vmm::memory`, `hitz-vmm::run_loop`, `hitz-api::config`)

🔦 Insight: Refactored existing module documentation to explicitly explain *why* structs and functions exist using the Bard philosophy. Added `# Abstract`, `# The Hero's Journey`, `# Details`, and `# Errors`/`# Panics` headers to clearly separate purpose, usage, edge cases, and safety contracts without lying.

🧪 Example: Added multiple executable doctests across `GuestMemory::add_region`, `CpioBuilder::finish`, `VmConfig`, and others.

🖼️ Preview: Documentation can be generated and viewed via `cargo doc -p hitz-vmm -p hitz-api --open`.
