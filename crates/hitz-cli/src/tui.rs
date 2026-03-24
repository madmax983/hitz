use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use hitz_api::{ActionVmRequest, MetricsSnapshot, VmAction, VmInfo};
use hyper::Method;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;

use crate::pipe_client;

pub struct App {
    pub pipe_path: String,
    pub tcp_addr: Option<SocketAddr>,
    pub vms: Vec<VmInfo>,
    pub selected: usize,
    pub current_metrics: Option<MetricsSnapshot>,
    pub metrics_error: Option<String>,
    pub should_quit: bool,
}

impl App {
    #[must_use]
    pub const fn new(pipe_path: String, tcp_addr: Option<SocketAddr>) -> Self {
        Self {
            pipe_path,
            tcp_addr,
            vms: Vec::new(),
            selected: 0,
            current_metrics: None,
            metrics_error: None,
            should_quit: false,
        }
    }

    pub fn draw(&self, f: &mut ratatui::Frame) {
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
        use ratatui::text::{Span, Line};

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
            .split(f.area());

        // Left Pane: VM List
        let items: Vec<ListItem> = self
            .vms
            .iter()
            .enumerate()
            .map(|(i, vm)| {
                let state_str = format!("{:?}", vm.state);
                let (color, style) = if i == self.selected {
                    (Color::Black, Style::default().bg(Color::White).add_modifier(Modifier::BOLD))
                } else {
                    (Color::White, Style::default())
                };

                let item_style = match vm.state {
                    hitz_api::VmState::Running => color,
                    hitz_api::VmState::Stopped => Color::DarkGray,
                    hitz_api::VmState::Failed => Color::Red,
                    hitz_api::VmState::Created => Color::Yellow,
                };

                let content = format!(" {} [{}]", vm.id, state_str);
                ListItem::new(Span::styled(content, style.fg(item_style)))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title(" VMs (q: quit, s: start, S: stop) ").borders(Borders::ALL));

        f.render_widget(list, chunks[0]);

        // Right Pane: Details
        if let Some(vm) = self.vms.get(self.selected) {
            let mut details = vec![
                Line::from(vec![Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(&vm.id)]),
                Line::from(vec![Span::styled("State: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(format!("{:?}", vm.state))]),
                Line::from(vec![Span::styled("CPUs: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(vm.config.cpus.to_string())]),
                Line::from(vec![Span::styled("RAM: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(format!("{} MiB", vm.config.ram_mib))]),
            ];

            if let Some(exit_reason) = &vm.exit_reason {
                details.push(Line::from(vec![Span::styled("Exit Reason: ", Style::default().add_modifier(Modifier::BOLD)), Span::raw(exit_reason)]));
            }

            details.push(Line::from(""));
            details.push(Line::from(Span::styled("Metrics:", Style::default().add_modifier(Modifier::BOLD))));

            if let Some(metrics) = &self.current_metrics {
                details.push(Line::from(format!("CPU Total: {:.1}%", metrics.cpu.total_pct)));
                let used_mb = metrics.memory.used_bytes / (1024 * 1024);
                let total_mb = metrics.memory.total_bytes / (1024 * 1024);
                details.push(Line::from(format!("Memory: {used_mb} MiB / {total_mb} MiB")));

                for disk in &metrics.disks {
                    let read_kb = disk.read_bytes / 1024;
                    let write_kb = disk.write_bytes / 1024;
                    details.push(Line::from(format!("Disk ({}): read {}K, write {}K", disk.name, read_kb, write_kb)));
                }

                for net in &metrics.networks {
                    let rx_kb = net.rx_bytes / 1024;
                    let tx_kb = net.tx_bytes / 1024;
                    details.push(Line::from(format!("Net ({}): rx {}K, tx {}K", net.interface, rx_kb, tx_kb)));
                }

                if !metrics.processes.is_empty() {
                    details.push(Line::from(""));
                    details.push(Line::from("Top Processes:"));
                    for p in metrics.processes.iter().take(5) {
                        details.push(Line::from(format!("  [{:>6}] {:<15} {:.1}% cpu", p.pid, p.name, p.cpu_pct)));
                    }
                }
            } else if let Some(err) = &self.metrics_error {
                details.push(Line::from(Span::styled(err, Style::default().fg(Color::DarkGray))));
            } else {
                details.push(Line::from(Span::styled("No metrics available (VM may be stopped or initializing).", Style::default().fg(Color::DarkGray))));
            }

            let p = Paragraph::new(details)
                .block(Block::default().title(format!(" {} Details ", vm.id)).borders(Borders::ALL));

            f.render_widget(p, chunks[1]);
        } else {
            let p = Paragraph::new("No VMs available.")
                .block(Block::default().title(" Details ").borders(Borders::ALL));
            f.render_widget(p, chunks[1]);
        }
    }
}

pub async fn run_tui(pipe_path: String, tcp_addr: Option<SocketAddr>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(pipe_path, tcp_addr);

    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("TUI error: {err:?}");
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(500);

    loop {
        // Refresh data every tick
        if last_tick.elapsed() >= tick_rate {
            refresh_vms(app).await;
            last_tick = std::time::Instant::now();
        }

        let _ = terminal.draw(|f| app.draw(f))?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
                match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Up => {
                        if app.selected > 0 {
                            app.selected -= 1;
                            app.current_metrics = None; // Reset until next fetch
                            app.metrics_error = None;
                        }
                    }
                    KeyCode::Down => {
                        if !app.vms.is_empty() && app.selected < app.vms.len() - 1 {
                            app.selected += 1;
                            app.current_metrics = None;
                            app.metrics_error = None;
                        }
                    }
                    KeyCode::Char('s') => {
                        if let Some(vm) = app.vms.get(app.selected) {
                            let body = serde_json::to_string(&ActionVmRequest {
                                action: VmAction::Start,
                            })?;
                            let _ = pipe_client::pipe_request(
                                &app.pipe_path,
                                app.tcp_addr,
                                Method::POST,
                                &format!("/vms/{}/action", vm.id),
                                Some(&body),
                            ).await;
                            refresh_vms(app).await;
                        }
                    }
                    KeyCode::Char('S') => {
                        if let Some(vm) = app.vms.get(app.selected) {
                            let body = serde_json::to_string(&ActionVmRequest {
                                action: VmAction::Stop,
                            })?;
                            let _ = pipe_client::pipe_request(
                                &app.pipe_path,
                                app.tcp_addr,
                                Method::POST,
                                &format!("/vms/{}/action", vm.id),
                                Some(&body),
                            ).await;
                            refresh_vms(app).await;
                        }
                    }
                    _ => {}
                }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

async fn refresh_vms(app: &mut App) {
    if let Ok((status, resp)) = pipe_client::pipe_request(&app.pipe_path, app.tcp_addr, Method::GET, "/vms", None).await
        && status.is_success()
        && let Ok(vms) = serde_json::from_str::<Vec<VmInfo>>(&resp)
    {
        app.vms = vms;
    }

    if app.selected >= app.vms.len() {
        app.selected = app.vms.len().saturating_sub(1);
    }

    if let Some(vm) = app.vms.get(app.selected) {
        if vm.state == hitz_api::VmState::Running {
            if let Ok((status, resp)) = pipe_client::pipe_request(&app.pipe_path, app.tcp_addr, Method::GET, &format!("/vms/{}/metrics", vm.id), None).await {
                if status.is_success() {
                    if let Ok(metrics) = serde_json::from_str::<MetricsSnapshot>(&resp) {
                        app.current_metrics = Some(metrics);
                        app.metrics_error = None;
                    } else {
                        app.metrics_error = Some("Failed to parse metrics".into());
                    }
                } else {
                    app.metrics_error = Some(format!("Error {status}: {resp}"));
                }
            } else {
                app.metrics_error = Some("Failed to fetch metrics (daemon down?)".into());
            }
        } else {
            app.current_metrics = None;
            app.metrics_error = None;
        }
    } else {
        app.current_metrics = None;
        app.metrics_error = None;
    }
}
