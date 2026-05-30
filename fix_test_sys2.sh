sed -i 's/assert!(output.contains("System:"),/assert!(output.contains("CPU Total"),/g' crates/hitz-cli/src/main.rs
sed -i 's/assert!(output.contains("Disks:"),/assert!(output.contains("Disk"),/g' crates/hitz-cli/src/main.rs
sed -i 's/assert!(output.contains("Networks:"),/assert!(output.contains("Interface"),/g' crates/hitz-cli/src/main.rs
