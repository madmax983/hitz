with open("crates/hitz-daemon/src/vm_manager.rs", "r") as f:
    text = f.read()

text = text.replace("""let mut info_list = Vec::with_capacity(vms.len());
        info_list.extend(vms.iter().map(|(id, entry)| entry.to_info(id)));
        Ok(info_list)""", "Ok(vms.iter().map(|(id, entry)| entry.to_info(id)).collect())")

with open("crates/hitz-daemon/src/vm_manager.rs", "w") as f:
    f.write(text)
