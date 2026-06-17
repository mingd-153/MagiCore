use std::{io, path::PathBuf, sync::{Arc, Mutex}, time::Duration};

use anyhow::Result;
use colored::*;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};

use sysinfo::System;





const LOGO: &[&str] = &[
    "███╗   ███╗███████╗ ██████╗  █████╗  ██████╗  █████╗ ████████╗███████╗",
    "████╗ ████║██╔════╝██╔════╝ ██╔══██╗██╔════╝ ██╔══██╗╚══██╔══╝██╔════╝",
    "██╔████╔██║█████╗  ██║  ███╗███████║██║  ███╗███████║   ██║   █████╗",
    "██║╚██╔╝██║██╔══╝  ██║   ██║██╔══██║██║   ██║██╔══██║   ██║   ██╔══╝",
    "██║ ╚═╝ ██║███████╗╚██████╔╝██║  ██║╚██████╔╝██║  ██║   ██║   ███████╗",
    "╚═╝     ╚═╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚══════╝",
];


fn load_logo_lines() -> Vec<String> {
    LOGO.iter().map(|s| s.to_string()).collect()
}

// Retrieve public IP address using a UDP socket trick (outbound IP)
fn get_ip_address() -> Option<String> {
    use std::net::UdpSocket;
    // Bind to an arbitrary local address
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connect to external address; no packets are sent
    socket.connect("8.8.8.8:80").ok()?;
    // Local address now contains the outbound IP
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

// Retrieve local network interface IP (first non‑loopback IPv4)
#[allow(clippy::collapsible_match)]
fn get_local_ip_address() -> Option<String> {
    // Requires the `if-addrs` crate
    if_addrs::get_if_addrs().ok().and_then(|ifaces| {
        for iface in ifaces {
            if let std::net::IpAddr::V4(v4) = iface.ip() {
                if !v4.is_loopback() {
                    return Some(v4.to_string());
                }
            }
        }
        None
    })
}


fn get_system_info_lines() -> Vec<Line<'static>> {
    // Gather system-wide info
    let sys = System::new_all();
    // CPU usage (global CPU info)
    let cpu_usage = sys.global_cpu_info().cpu_usage();
    // RAM usage
    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();
    // SSD info – use Disks helper (first disk)
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let ssd_info = if let Some(disk) = disks.list().first() {
        let gb = disk.total_space() as f64 / 1_073_741_824.0;
        format!("SSD: {:.2} GB", gb)
    } else {
        "SSD: N/A".to_string()
    };
    // GPU info – attempt to detect via components list (may be empty on some platforms)
    let gpu_info = {
        // Use Components helper to list hardware components (may include GPU)
        let components = sysinfo::Components::new_with_refreshed_list();
        if let Some(comp) = components.list().iter().find(|c| c.label().to_ascii_lowercase().contains("gpu")) {
            format!("GPU: {}", comp.label())
        } else {
            "GPU: N/A".to_string()
        }
    };
    // Bandwidth – sum of received + transmitted bytes across interfaces
    let networks = sysinfo::Networks::new_with_refreshed_list();
    let bandwidth_bytes: u64 = networks.values().map(|data| data.total_received() + data.total_transmitted()).sum();
    let bandwidth_str = format!("Bandwidth: {:.2} MB", bandwidth_bytes as f64 / 1_048_576.0);
    // Cache (available memory)
    let cache_str = format!("Cache: {} MB", sys.available_memory() / 1024);

    // Public IP address (outbound)
    let ip_line = match get_ip_address() {
        Some(ip) => format!("IP: {}", ip),
        None => "IP: N/A".to_string(),
    };
    // Local IP address (first non‑loopback interface)
    let local_ip_line = match get_local_ip_address() {
        Some(ip) => format!("Local IP: {}", ip),
        None => "Local IP: N/A".to_string(),
    };
    vec![
        Line::from(Span::styled(format!("CPU: {:.2}%", cpu_usage), Style::default())),
        Line::from(Span::styled(format!("RAM: {} MB / {} MB", used_mem / 1024, total_mem / 1024), Style::default())),
        Line::from(Span::styled(ssd_info, Style::default())),
        Line::from(Span::styled(gpu_info, Style::default())),
        Line::from(Span::styled(bandwidth_str, Style::default())),
        Line::from(Span::styled(cache_str, Style::default())),
        Line::from(Span::styled(ip_line, Style::default())),
        Line::from(Span::styled(local_ip_line, Style::default())),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D LAYERED LOGO (rendered with color blocks for depth)
// ─────────────────────────────────────────────────────────────────────────────
// LOGO will be loaded from logo.txt at runtime
//    r"  ╔╗ ╔╗      ╔╗      ╔═══╗ ╔═══╗ ╔═══╗ ╔╗   ╔╗",
//    r" ║║ ║║     ╔╝║     ║║   ║ ║║   ║ ║║   ║ ║║   ║║",
//    r" ║╚═╝║    ╔╝╔╝     ║║   ║ ║║   ║ ║║   ║ ║╚═══╝║",


const _AUTHOR: &str = "doanmihh153";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROJECT_NAME: &str = "MegaGate";

const MENU_ITEMS: &[(&str, &str, Color)] = &[
    ("Home",      "Home screen",      Color::Green),
    ("Settings",  "Configuration",    Color::Cyan),
    ("Logs",      "View logs",        Color::Red),
    ("Help",      "Help documentation",Color::Blue),
    ("Exit",      "Quit application", Color::Yellow),
];

// ─────────────────────────────────────────────────────────────────────────────
// Application state
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct AppState {
    input: String,
    log: Vec<String>,
    // Index of highlighted item when sidebar has focus
    selected: usize,
    // Which pane currently receives keyboard input
    focus: Focus,
    // Scroll offset for the sidebar list (if it exceeds visible height)
    sidebar_offset: usize,
    // Scroll offset for the console (kept static for now)
    console_offset: usize,
    // Unused placeholders retained for compatibility
    _message: String,
    _progress: u16,
    _showing_progress: bool,
    running: bool,
    _project_dir: PathBuf,
    logo_frame: usize,
    _overlay: Overlay,
}

#[derive(Clone, PartialEq)]
enum Focus {
    Sidebar,
    Console,
}

#[derive(Clone, PartialEq)]
#[allow(dead_code)]
enum Overlay {
    None,
    Help,
}

impl AppState {
    fn new(project_dir: PathBuf) -> Self {
        Self {
            selected: 0,
            focus: Focus::Sidebar,
            sidebar_offset: 0,
            console_offset: 0,
            _message: String::new(),
            _progress: 0,
            _showing_progress: false,
            running: true,
            _project_dir: project_dir,
            input: String::new(),
            log: Vec::new(),
            logo_frame: 0,
            _overlay: Overlay::None,

        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────
pub async fn run_ui(project_dir: PathBuf) -> Result<()> {
    let mut stdout = io::stdout();
    // Enable raw mode and clear the screen for a clean CLI‑style start
    let _ = crossterm::terminal::enable_raw_mode();
    // Clear any existing terminal content
    let _ = crossterm::execute!(io::stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All));
    // Enter alternate screen to lock whole UI (prevents terminal scroll)
    let _ = crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen);
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let state = Arc::new(Mutex::new(AppState::new(project_dir)));
    let mut should_quit = false;

        while !should_quit {
            terminal.draw(|f| draw_ui(f, &state))?;
            // advance logo animation frame
            {
                let mut app = state.lock().unwrap();
                app.logo_frame = (app.logo_frame + 1) % 4;
            }


        if event::poll(Duration::from_millis(150))? {
            let ev = event::read()?;
            let mut app = state.lock().unwrap();
            if let Event::Key(key) = ev {

                    match key.code {
                        // Quit the REPL (Ctrl+Q) or Esc
                        KeyCode::Esc => {
                            should_quit = true;
                        }
                        KeyCode::Char('q') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                should_quit = true;
                            }
                        }
                        // Navigation keys
                        KeyCode::Up => {
                            if app.focus == Focus::Sidebar {
                                if app.selected == 0 {
                                    app.selected = MENU_ITEMS.len() - 1;
                                } else {
                                    app.selected -= 1;
                                }
                                if app.sidebar_offset > app.selected {
                                    app.sidebar_offset = app.selected;
                                }
} else if app.focus == Focus::Console && app.console_offset > 0 {
                                 app.console_offset -= 1;
                             }
                        }
                        KeyCode::Down => {
                            if app.focus == Focus::Sidebar {
                                app.selected = (app.selected + 1) % MENU_ITEMS.len();
                                if app.sidebar_offset + 1 < app.selected {
                                    app.sidebar_offset = app.selected;
                                }
                            } else if app.focus == Focus::Console {
                                let max_offset = if !app.log.is_empty() { app.log.len() - 1 } else { 0 };
                                if app.console_offset < max_offset {
                                    app.console_offset += 1;
                                }
                            }
                        }
                        KeyCode::Char('k') => {
                            if app.focus == Focus::Sidebar {
                                if app.selected == 0 {
                                    app.selected = MENU_ITEMS.len() - 1;
                                } else {
                                    app.selected -= 1;
                                }
                                if app.sidebar_offset > app.selected {
                                    app.sidebar_offset = app.selected;
                                }
                            }
                        }
                        KeyCode::Char('j') => {
                            if app.focus == Focus::Sidebar {
                                app.selected = (app.selected + 1) % MENU_ITEMS.len();
                                if app.sidebar_offset + 1 < app.selected {
                                    app.sidebar_offset = app.selected;
                                }
                            }
                        }
                        KeyCode::Tab => {
                            app.focus = if app.focus == Focus::Sidebar { Focus::Console } else { Focus::Sidebar };
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Enter => {
                            if app.focus == Focus::Sidebar {
                                // Simulate activating the selected menu item
                                let (key, _desc, _color) = MENU_ITEMS[app.selected];
                                app.log.push(format!("Menu selected: {}", key));
                            } else {
                                let cmd = app.input.trim().to_string();
                                app.log.push(cmd.clone());
                                app.input.clear();
                                process_command(&mut app);
                            }
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        _ => {}
                    }
                }


        }

        // Check quit flag
        if !state.lock().unwrap().running {
            should_quit = true;
        }
    }

    // Leave alternate screen and restore terminal
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);

    terminal.show_cursor()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Execute selected action
// ─────────────────────────────────────────────────────────────────────────────
fn process_command(app: &mut AppState) {
    // Trimmed command is already stored in log by the caller
    // If the last line (the command) is empty, we treat it as a blank line separator
    if let Some(last) = app.log.last() {
        if last.is_empty() {
            // just a blank line – nothing else to do
            return;
        }
    }

    // Get the most recent command string (the one we just pushed to log)
    let cmd = app.log.last().cloned().unwrap_or_default();
    let cmd = cmd.trim();

    // Handle built‑in REPL commands
    match cmd {
        "exit" | "quit" => {
            // Signal the main loop to quit
            // We set a field that will be read after the event loop
            app.running = false;
        }
        "clear" => {
            app.log.clear();
        }
        c if c.starts_with("run ") => {
            // Execute a shell command – everything after "run " is passed to the shell
            let shell_cmd = c.strip_prefix("run ").unwrap_or("");
            match std::process::Command::new("sh")
                .arg("-c")
                .arg(shell_cmd)
                .output() {
                Ok(output) => {
                    let out = String::from_utf8_lossy(&output.stdout);
                    let err = String::from_utf8_lossy(&output.stderr);
                    if !out.is_empty() {
                        for line in out.lines() {
                            app.log.push(line.to_string());
                        }
                    }
                    if !err.is_empty() {
                        for line in err.lines() {
                            app.log.push(format!("ERR: {}", line));
                        }
                    }
                }
                Err(e) => {
                    app.log.push(format!("Failed to run command: {}", e));
                }
            }
        }
        _ => {
            // Unknown command – just echo back for now
            if !cmd.is_empty() {
                app.log.push(format!("unknown: {}", cmd));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Prompt for input
// ─────────────────────────────────────────────────────────────────────────────
#[allow(dead_code)]
fn prompt_input(prompt: &str) -> String {
    crossterm::terminal::disable_raw_mode().ok();
    print!("\n  {} ", prompt.yellow().bold());
    io::Write::flush(&mut io::stdout()).ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    crossterm::terminal::enable_raw_mode().ok();
    input.trim().to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Main UI drawing function
// ─────────────────────────────────────────────────────────────────────────────
#[allow(clippy::iter_overeager_cloned, clippy::manual_div_ceil)]
fn draw_ui(f: &mut Frame, state: &Arc<Mutex<AppState>>) {
    let app = state.lock().unwrap();

    let size = f.size();
    let compact = size.width < 80 || size.height < 24;

    // If terminal is too small, show warning and skip drawing UI
    if compact {
        let warning = Paragraph::new("Terminal size too small. Minimum 80x24 required.")
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        f.render_widget(warning, size);
        return;
    }

    // Determine layout percentages based on compact flag.
    // Header height increased to ensure author line is visible.
    // Body (info+menu+console) takes the remaining space.
    let (header_pct, footer_pct): (u16, u16) = if compact {
        // Compact layout: allocate more space to header and footer.
        (25, 15) // header 25%, footer 15%
    } else {
        // Normal layout: larger header for author line.
        (30, 10) // header 30%, footer 10%
    };
    let body_pct = 100 - header_pct - footer_pct;

    // Full screen layout: header, body, footer.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Percentage(header_pct),
            Constraint::Percentage(body_pct),
            Constraint::Percentage(footer_pct),
        ])
        .split(f.size());

    // ── HEADER: Logo + Title + Author ────────────────────────────────────────
    let header_block = Block::default();
    // Load ASCII logo lines and prepend to header content
    let logo_lines = load_logo_lines();
    let mut header_content: Vec<Line> = logo_lines.iter().map(|s| Line::from(Span::styled(s.clone(), Style::default()))).collect();
    // Project title with bold and underline
    header_content.push(Line::from(Span::styled(
        PROJECT_NAME,
        Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )));

    // Open source line
    header_content.push(Line::from(Span::styled(
        "Open Source • Built in Vietnam 🇻🇳",
        Style::default(),
    )));
    // Created by line
        header_content.push(Line::from(Span::styled(
            format!("Created by {}", _AUTHOR),
            Style::default(),
        )));
    // Optional version line
    header_content.push(Line::from(Span::styled(
        format!("Version: {}", VERSION),
        Style::default(),
    )));

    let header_widget = Paragraph::new(header_content)
    .block(header_block)
    .alignment(Alignment::Center);

    f.render_widget(header_widget, chunks[0]);

    // ── MAIN ROW: System Info | Menu | Console (full height) ────────
    // Split the middle area (chunks[1]) into three columns
    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // System Info column
            Constraint::Percentage(35), // Menu column
            Constraint::Percentage(35), // Console column
        ])
        .split(chunks[1]);

    // ── INFO COLUMN ────────────────────────────────────────
    let info_lines = get_system_info_lines(); // 8 lines expected
    let info_paragraph = Paragraph::new(info_lines.clone())
        .block(Block::default().borders(Borders::ALL).title("System Info"))
        .wrap(Wrap { trim: true });
    f.render_widget(info_paragraph, body_cols[0]);

    // ── MENU COLUMN ────────────────────────────────────────
    let all_items: Vec<ListItem> = MENU_ITEMS.iter().enumerate().map(|(i, (key, desc, _color))| {
        let style = if i == app.selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        ListItem::new(Line::from(vec![
            Span::styled(*key, style),
            Span::raw(" "),
            Span::styled(*desc, style),
        ]))
    }).collect();
    // Show only the top portion of the menu (no scrolling)
    let visible_height = body_cols[1].height as usize;
    let end = std::cmp::min(visible_height, all_items.len());
    let items = &all_items[..end];
    let menu_lines: Vec<Line> = items.iter().enumerate().map(|(i, _item)| {
        let (key, desc, color) = MENU_ITEMS[i];
        let base_style = Style::default().fg(color);
        let style = if i == app.selected {
            base_style.add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            base_style
        };
        Line::from(vec![
            Span::styled(key, style),
            Span::raw(" "),
            Span::styled(desc, style),
        ])
    }).collect();
    let menu_paragraph = Paragraph::new(menu_lines)
        .block(Block::default().borders(Borders::ALL).title("Menu"));
    f.render_widget(menu_paragraph, body_cols[1]);

    // ── CONSOLE COLUMN ────────────────────────────────────────
    let console_visible = body_cols[2].height as usize;
    let log_start = app.console_offset;
    let log_end = std::cmp::min(log_start + console_visible, app.log.len());
    let log_slice = &app.log[log_start..log_end];
    let styled_log_lines: Vec<Line> = log_slice.iter().map(|line| {
        let mut style = Style::default();
        if line.contains("[INFO]") {
            style = style.fg(Color::Green);
        } else if line.contains("[WARN]") {
            style = style.fg(Color::Yellow);
        } else if line.contains("[ERROR]") {
            style = style.fg(Color::Red);
        }
        Line::from(Span::styled(line.clone(), style))
    }).collect();
    let log_paragraph = Paragraph::new(styled_log_lines)
        .block(Block::default().borders(Borders::ALL).title("Console"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_paragraph, body_cols[2]);

    // ── INPUT SECTION (footer) ────────────────────────────────────────
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)])
        .split(chunks[2]);

    let input_span = Span::styled(
        format!("> {}", app.input),
        Style::default(),
    );
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Input");
    let input_paragraph = Paragraph::new(input_span)
        .block(input_block);
    f.render_widget(input_paragraph, footer_chunks[0]);
}
