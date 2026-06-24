import re

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

pattern = r"fn draw_vm_top_ui\(\s*f: &mut ratatui::Frame<'_>,\s*vm_id: &str,\s*last_err: Option<&str>,\s*last_snap: Option<&hitz_api::MetricsSnapshot>,\s*\)\s*\{.*?(?=\nasync fn handle_vm_metrics)"
replacement = """fn draw_vm_top_ui(
    f: &mut ratatui::Frame<'_>,
    vm_id: &str,
    last_err: Option<&str>,
    last_snap: Option<&hitz_api::MetricsSnapshot>,
) {
    use ratatui::{
        layout::{Constraint, Direction, Layout},
        widgets::{Block, Borders, Paragraph},
    };

    let size = f.area();
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(4), // CPU & Mem Gauges
            Constraint::Min(5),    // Main content (Tables)
        ])
        .split(size);

    draw_top_header(f, vm_id, main_chunks[0]);

    if let Some(err) = last_err {
        draw_top_error(f, err, main_chunks[1]);
        return;
    }

    if let Some(snap) = last_snap {
        draw_top_gauges(f, snap, main_chunks[1]);

        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[2]);

        let io_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(bottom_chunks[0]);

        draw_disk_table(f, snap, io_chunks[0]);
        draw_network_table(f, snap, io_chunks[1]);
        draw_process_table(f, snap, bottom_chunks[1]);
    } else if last_err.is_none() {
        let loading =
            Paragraph::new("Loading metrics...").block(Block::default().borders(Borders::ALL));
        f.render_widget(loading, main_chunks[1]);
    }
}
"""

new_content = re.sub(pattern, replacement, content, flags=re.DOTALL)

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(new_content)
