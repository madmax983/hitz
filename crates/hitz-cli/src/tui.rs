use std::io;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};

use crate::VmMetricsArgs;
use hitz_api::MetricsSnapshot;

pub async fn run_metrics_tui(args: &VmMetricsArgs) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(1000);
    let res = run_app(&mut terminal, tick_rate, args).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

async fn fetch_metrics(args: &VmMetricsArgs) -> Result<MetricsSnapshot> {
    use hyper::Method;
    let (status, resp) = crate::pipe_client::pipe_request(
        &args.pipe,
        args.tcp,
        Method::GET,
        &format!("/vms/{}/metrics", args.id),
        None,
    )
    .await?;

    if status.is_success() {
        let snap: MetricsSnapshot =
            serde_json::from_str(&resp).context("failed to parse metrics response")?;
        Ok(snap)
    } else if let Ok(err) = serde_json::from_str::<hitz_api::ApiError>(&resp) {
        anyhow::bail!("API error ({}): {}", status, err.message)
    } else {
        anyhow::bail!("Failed to fetch metrics: {status} {resp}")
    }
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    tick_rate: Duration,
    args: &VmMetricsArgs,
) -> Result<()>
where
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut last_tick = Instant::now();
    let mut current_metrics: Option<MetricsSnapshot> = None;

    loop {
        if current_metrics.is_none() || last_tick.elapsed() >= tick_rate {
            current_metrics = Some(fetch_metrics(args).await?);
            last_tick = Instant::now();
        }

        let _ = terminal.draw(|f| ui(f, args, current_metrics.as_ref()))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            #[allow(clippy::collapsible_if)]
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    return Ok(());
                }
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn ui(f: &mut ratatui::Frame, args: &VmMetricsArgs, metrics: Option<&MetricsSnapshot>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(f.area());

    let header = Paragraph::new(format!(
        " Live Metrics for VM: {} (Press 'q' to quit)",
        args.id
    ))
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    if let Some(snap) = metrics {
        let mem_percent = if snap.memory.total_bytes > 0 {
            (snap.memory.used_bytes as f64 / snap.memory.total_bytes as f64 * 100.0) as u16
        } else {
            0
        };

        let cpu_gauge = Gauge::default()
            .block(Block::default().title(" CPU Usage ").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Yellow))
            .percent(snap.cpu.total_pct.clamp(0.0, 100.0) as u16);

        let mem_gauge = Gauge::default()
            .block(
                Block::default()
                    .title(" Memory Usage ")
                    .borders(Borders::ALL),
            )
            .gauge_style(Style::default().fg(Color::Green))
            .percent(mem_percent.clamp(0, 100));

        let gauges_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        f.render_widget(cpu_gauge, gauges_layout[0]);
        f.render_widget(mem_gauge, gauges_layout[1]);

        let mut lines = vec![];
        lines.push(Line::from(Span::styled(
            "Processes:",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for proc in &snap.processes {
            lines.push(Line::from(format!(
                " PID: {}, Name: {}, CPU: {:.1}%, Mem: {} KB",
                proc.pid,
                proc.name,
                proc.cpu_pct,
                proc.rss_bytes / 1024
            )));
        }

        let p =
            Paragraph::new(lines).block(Block::default().title(" Details ").borders(Borders::ALL));
        f.render_widget(p, chunks[2]);
    } else {
        let p = Paragraph::new(" Waiting for metrics data...")
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(p, chunks[1]);
    }
}
