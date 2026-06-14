use std::{io, path::PathBuf, sync::{Arc, Mutex}, thread, time::Duration};

use anyhow::Result;
use colored::*;
use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline},
    Frame, Terminal,
};

use crate::commands;

const LOGO_PATH: &str = "src/ui/logo.txt";

fn load_logo_lines() -> Vec<String> {
    std::fs::read_to_string(LOGO_PATH)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D LAYERED LOGO (rendered with color blocks for depth)
// ─────────────────────────────────────────────────────────────────────────────
// LOGO will be loaded from logo.txt at runtime
//    r"  ╔╗ ╔╗      ╔╗      ╔═══╗ ╔═══╗ ╔═══╗ ╔╗   ╔╗",
//    r" ║║ ║║     ╔╝║     ║║   ║ ║║   ║ ║║   ║ ║║   ║║",
//    r" ║╚═╝║    ╔╝╔╝     ║║   ║ ║║   ║ ║║   ║ ║╚═══╝║",


const AUTHOR: &str = "✦ Crafted by doanmihh15.3 ✦";

const MENU_ITEMS: &[(&str, &str, Color)] = &[
    ("➤ Install",   "Install dependencies",      Color::Green),
    ("➤ Update",   "Update a package",          Color::Cyan),
    ("➤ Remove",   "Remove a package",          Color::Red),
    ("➤ List",     "View dependency graph",     Color::Blue),
    ("➤ Audit",    "Run security audit",        Color::Yellow),
    ("➤ Export",   "Export lock file",           Color::Magenta),
    ("➤ Exit",     "Quit application",           Color::White),
];

// ─────────────────────────────────────────────────────────────────────────────
// Application state
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct AppState {
    selected: usize,
    message: String,
    progress: u16,
    showing_progress: bool,
    running: bool,
    project_dir: PathBuf,
}

impl AppState {
    fn new(project_dir: PathBuf) -> Self {
        Self {
            selected: 0,
            message: String::new(),
            progress: 0,
            showing_progress: false,
            running: true,
            project_dir,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────
pub async fn run_ui(project_dir: PathBuf) -> Result<()> {
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let state = Arc::new(Mutex::new(AppState::new(project_dir)));
    let mut should_quit = false;

    while !should_quit {
        terminal.draw(|f| draw_ui(f, &state))?;

        if event::poll(Duration::from_millis(150))? {
            match event::read()? {
                Event::Key(key) => {
                    let mut app = state.lock().unwrap();
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            should_quit = true;
                        }
                        KeyCode::Down => {
                            app.selected = (app.selected + 1) % MENU_ITEMS.len();
                        }
                        KeyCode::Up => {
                            app.selected = if app.selected == 0 {
                                MENU_ITEMS.len() - 1
                            } else {
                                app.selected - 1
                            };
                        }
                        KeyCode::Enter => {
                            execute_action(&mut app).await;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(me) => {
                    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                        let clicked_idx = (me.row as usize).saturating_sub(3); // offset header
                        if clicked_idx < MENU_ITEMS.len() {
                            let mut app = state.lock().unwrap();
                            app.selected = clicked_idx;
                            execute_action(&mut app).await;
                        }
                    }
                }
                _ => {}
            }
        }

        // Check quit flag
        if state.lock().unwrap().running == false {
            should_quit = true;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    terminal.show_cursor()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Execute selected action
// ─────────────────────────────────────────────────────────────────────────────
async fn execute_action(app: &mut AppState) {
use crate::commands;

const LOGO_PATH: &str = "src/ui/logo.txt";

fn load_logo_lines() -> Vec<String> {
    std::fs::read_to_string(LOGO_PATH)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

    app.message = "⚡ Working...".cyan().to_string();
    app.showing_progress = true;
    app.progress = 0;

    // Simulate progress while running
    let app_clone = Arc::new(Mutex::new(app.clone()));
    let handle = thread::spawn({
        let app = Arc::clone(&app_clone);
        move || {
            for i in 0..=100 {
                if let Ok(mut a) = app.lock() {
                    a.progress = i;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    });

    let proj_dir = app.project_dir.clone();
    let action_idx = app.selected;

    // Run actual command based on selection
    match action_idx {
        0 => { let _ = commands::install(Some(proj_dir.to_string_lossy().into())).await; }
        1 => { let _ = commands::update(Some(prompt_input("Package: "))).await; }
        2 => { let _ = commands::remove(prompt_input("Package: ")).await; }
        3 => { let _ = commands::list(true).await; }
        4 => { let _ = commands::audit().await; }
        5 => { let _ = commands::export(prompt_input("Format: ")).await; }
        6 => { app.running = false; }
        _ => {}
    }

    handle.join().ok();

    app.showing_progress = false;
    app.progress = 100;
    app.message = "✓ Done! Press any key...".green().to_string();
}

// ─────────────────────────────────────────────────────────────────────────────
// Prompt for input
// ─────────────────────────────────────────────────────────────────────────────
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
fn draw_ui(f: &mut Frame, state: &Arc<Mutex<AppState>>) {
    let app = state.lock().unwrap();

    // Full screen layout: header (logo + title + author), menu, footer (status + progress)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Percentage(30), // header proportion
            Constraint::Percentage(60), // menu proportion
            Constraint::Percentage(10), // footer proportion
        ])
        .split(f.size());

    // ── HEADER: Logo + Title + Author ──────────────────────────────────────────
    let header_block = Block::default()
        .style(Style::default().bg(Color::Rgb(30, 30, 50)).fg(Color::White))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    // Logo lines with color gradient (3D effect) – responsive truncation
    let logo_strings = load_logo_lines();
    let width = f.size().width as usize;
    let logo_spans: Vec<Line> = logo_strings
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // Truncate line if it exceeds terminal width
            let display = if line.chars().count() > width {
                line.chars().take(width).collect::<String>()
            } else {
                line.clone()
            };
            let color = match i % 4 {
                0 => Color::LightCyan,
                1 => Color::LightMagenta,
                2 => Color::LightBlue,
                _ => Color::LightGreen,
            };
            Line::from(Span::styled(display, Style::default().fg(color).add_modifier(Modifier::BOLD)))
        })
        .collect();
    // Title
    let title_spans = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Developer from VietNam build with ♥️",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(AUTHOR, Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC))),
        ];


    let header_content: Vec<Line> = logo_spans
        .into_iter()
        .chain(title_spans.into_iter())
        .collect();

    let header_widget = Paragraph::new(header_content)
        .block(header_block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(header_widget, chunks[0]);

    // ── MENU: List of actions with colors ─────────────────────────────────────
    let menu_items: Vec<ListItem> = MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(i, (icon, desc, color))| {
            let is_selected = i == app.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(*color)
            };

            let icon_text = if is_selected { " ◉ " } else { " ○ " };
            let text = format!("{} {}  {}", icon_text, icon, desc);

            ListItem::new(Span::styled(text, style))
        })
        .collect();

    let menu_list = List::new(menu_items)
        .block(
            Block::default()
                .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(Color::White))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta))
                .title("  ⚡ Actions  "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_widget(menu_list, chunks[1]);

    // ── FOOTER: Status + Progress ──────────────────────────────────────────────
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[2]);

    // Status message
    let status = Paragraph::new(Span::styled(
        if app.message.is_empty() {
            "  Ready".to_string()
        } else {
            app.message.clone()
        },
        Style::default().fg(Color::White),
    ))
    .block(
        Block::default()
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(Color::White))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .title("  Status  "),
    );
    f.render_widget(status, footer_chunks[0]);

    // Progress bar (only visible when running)
    if app.showing_progress {
let progress_bar = Gauge::default()
            .ratio(app.progress as f64 / 100.0)
            .label(Span::styled(format!("{:>3}%", app.progress), Style::default().fg(Color::Yellow)))
            .block(
                Block::default()
                    .style(Style::default().bg(Color::Rgb(20, 20, 40)))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title("  Progress  "),
            )
            .style(Style::default().fg(Color::Green).bg(Color::Rgb(0, 80, 0)))
            .gauge_style(Style::default().fg(Color::Cyan));
        f.render_widget(progress_bar, footer_chunks[1]);
    } else {
        let empty = Block::default()
            .style(Style::default().bg(Color::Rgb(20, 20, 40)).fg(Color::White))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));
        f.render_widget(empty, footer_chunks[1]);
    }
}
