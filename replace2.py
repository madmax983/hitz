import re
import sys

with open("crates/hitz-cli/src/main.rs", "r") as f:
    content = f.read()

search = """                .header(
                    Row::new(["ID", "State", "RAM (MiB)", "CPUs", "Exit Reason"]).style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                )"""

replace = """                .header(
                    Row::new(["ID", "State", "RAM (MiB)", "CPUs", "Exit Reason"]).style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                )"""

content = content.replace(search, replace)

search_2 = """                    Row::new([
                        Cell::from(vm.id.as_str()),
                        Cell::from(state_str).style(Style::default().fg(state_color)),"""

replace_2 = """                    Row::new([
                        Cell::from(vm.id.as_str()).style(Style::default().add_modifier(Modifier::BOLD)),
                        Cell::from(state_str).style(Style::default().fg(state_color)),"""

content = content.replace(search_2, replace_2)

search_3 = """            let _ = table.set_header(["ID", "State", "RAM (MiB)", "CPUs", "Exit Reason"]);"""

replace_3 = """            let _ = table.set_header([
                Cell::new("ID").add_attribute(comfy_table::Attribute::Bold).fg(Color::Cyan),
                Cell::new("State").add_attribute(comfy_table::Attribute::Bold).fg(Color::Cyan),
                Cell::new("RAM (MiB)").add_attribute(comfy_table::Attribute::Bold).fg(Color::Cyan),
                Cell::new("CPUs").add_attribute(comfy_table::Attribute::Bold).fg(Color::Cyan),
                Cell::new("Exit Reason").add_attribute(comfy_table::Attribute::Bold).fg(Color::Cyan),
            ]);"""

content = content.replace(search_3, replace_3)

search_4 = """                let _ = table.add_row([
                    Cell::new(&info.id),
                    state_cell,"""

replace_4 = """                let _ = table.add_row([
                    Cell::new(&info.id).add_attribute(comfy_table::Attribute::Bold),
                    state_cell,"""

content = content.replace(search_4, replace_4)

with open("crates/hitz-cli/src/main.rs", "w") as f:
    f.write(content)
