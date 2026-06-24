import re

with open('crates/hitz-cli/src/main.rs', 'r') as f:
    content = f.read()

helpers = """
fn draw_top_header(f: &mut ratatui::Frame<'_>, vm_id: &str, area: ratatui::layout::Rect) {
    use ratatui::{
        style::{Color, Modifier, Style},
        widgets::{Block, Borders, Paragraph},
    };
    let header = Paragraph::new(format!("Hitz Top - VM: {}", vm_id))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, area);
}

fn draw_top_error(f: &mut ratatui::Frame<'_>, err: &str, area: ratatui::layout::Rect) {
    use ratatui::{
        style::{Color, Style},
        widgets::{Block, Borders, Paragraph},
    };
    let err_p = Paragraph::new(err)
        .style(Style::default().fg(Color::Red))
        .block(Block::default().borders(Borders::ALL).title("Error"));
    f.render_widget(err_p, area);
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
fn draw_top_gauges(
    f: &mut ratatui::Frame<'_>,
    snap: &hitz_api::MetricsSnapshot,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        layout::{Constraint, Direction, Layout},
        style::{Color, Style},
        widgets::{Block, Borders, Gauge},
    };

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let cpu_label = format!(
        "{:.1}% (Load: {:.2}, {:.2}, {:.2})",
        snap.cpu.total_pct, snap.cpu.load_avg[0], snap.cpu.load_avg[1], snap.cpu.load_avg[2]
    );
    let cpu_gauge = Gauge::default()
        .block(Block::default().title("CPU").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .percent((snap.cpu.total_pct as u16).min(100))
        .label(cpu_label);
    f.render_widget(cpu_gauge, top_chunks[0]);

    let used_mb = snap.memory.used_bytes / (1024 * 1024);
    let total_mb = snap.memory.total_bytes / (1024 * 1024);
    let mem_pct = if total_mb > 0 {
        ((used_mb as f64 / total_mb as f64) * 100.0) as u16
    } else {
        0
    };
    let mem_label = format!("{used_mb} MB / {total_mb} MB");
    let mem_gauge = Gauge::default()
        .block(Block::default().title("Memory").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Yellow))
        .percent(mem_pct.min(100))
        .label(mem_label);
    f.render_widget(mem_gauge, top_chunks[1]);
}

fn draw_disk_table(
    f: &mut ratatui::Frame<'_>,
    snap: &hitz_api::MetricsSnapshot,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        layout::Constraint,
        style::{Modifier, Style},
        widgets::{Block, Borders, Cell, Row, Table},
    };

    let disk_table = Table::new(
        snap.disks.iter().map(|d| {
            Row::new([
                Cell::from(d.name.as_str()),
                Cell::from(format!("{}", d.read_bytes / 1024)),
                Cell::from(format!("{}", d.write_bytes / 1024)),
            ])
        }),
        [
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    )
    .header(
        Row::new(["Device", "Read KB", "Write KB"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().title("Disks").borders(Borders::ALL));
    f.render_widget(disk_table, area);
}

fn draw_network_table(
    f: &mut ratatui::Frame<'_>,
    snap: &hitz_api::MetricsSnapshot,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        layout::Constraint,
        style::{Modifier, Style},
        widgets::{Block, Borders, Cell, Row, Table},
    };

    let net_table = Table::new(
        snap.networks.iter().map(|n| {
            Row::new([
                Cell::from(n.interface.as_str()),
                Cell::from(format!("{}", n.rx_bytes / 1024)),
                Cell::from(format!("{}", n.tx_bytes / 1024)),
            ])
        }),
        [
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    )
    .header(
        Row::new(["Interface", "Rx KB", "Tx KB"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().title("Networks").borders(Borders::ALL));
    f.render_widget(net_table, area);
}

fn draw_process_table(
    f: &mut ratatui::Frame<'_>,
    snap: &hitz_api::MetricsSnapshot,
    area: ratatui::layout::Rect,
) {
    use ratatui::{
        layout::Constraint,
        style::{Modifier, Style},
        widgets::{Block, Borders, Cell, Row, Table},
    };

    let proc_table = Table::new(
        snap.processes.iter().map(|p| {
            Row::new([
                Cell::from(p.pid.to_string()),
                Cell::from(p.name.as_str()),
                Cell::from(format!("{:.1}%", p.cpu_pct)),
                Cell::from(format!("{} MB", p.rss_bytes / (1024 * 1024))),
            ])
        }),
        [
            Constraint::Percentage(15),
            Constraint::Percentage(45),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(
        Row::new(["PID", "Name", "CPU", "RSS"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .title("Top Processes")
            .borders(Borders::ALL),
    );
    f.render_widget(proc_table, area);
}

"""

# Insert helpers just before `fn draw_vm_top_ui`
new_content = content.replace("#[allow(\n    clippy::too_many_lines,\n    clippy::cast_possible_truncation,\n    clippy::cast_sign_loss,\n    clippy::cast_precision_loss\n)]\nfn draw_vm_top_ui(", helpers + "\nfn draw_vm_top_ui(")

with open('crates/hitz-cli/src/main.rs', 'w') as f:
    f.write(new_content)
