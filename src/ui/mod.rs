use std::{io, path::PathBuf, sync::{Arc, Mutex}, time::Duration};

use anyhow::Result;
use colored::*;
use crossterm::event::{self, Event, KeyCode};
    use ratatui::{
        backend::CrosstermBackend,
        layout::{Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, ListItem, Paragraph, Wrap, canvas::{Canvas, Line as CanvasLine}},
        Frame, Terminal,
    };




const LOGO_PATH: &str = "logo.txt";

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
const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    message: String,
    progress: u16,
    showing_progress: bool,
    running: bool,
    project_dir: PathBuf,
    logo_frame: usize,
}

#[derive(Clone, PartialEq)]
enum Focus {
    Sidebar,
    Console,
}

impl AppState {
    fn new(project_dir: PathBuf) -> Self {
        Self {
            selected: 0,
            focus: Focus::Sidebar,
            sidebar_offset: 0,
            console_offset: 0,
            message: String::new(),
            progress: 0,
            showing_progress: false,
            running: true,
            project_dir,
            input: String::new(),
            log: Vec::new(),
            logo_frame: 0,

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
            match event::read()? {
                Event::Key(key) => {
                    let mut app = state.lock().unwrap();
                    match key.code {
                        // Quit the REPL
                    KeyCode::Char('q') | KeyCode::Esc => {
                        should_quit = true;
                    }
                    // Navigate or scroll depending on focus
                    KeyCode::Up => {
                        if app.focus == Focus::Sidebar {
                            if app.selected == 0 {
                                app.selected = MENU_ITEMS.len() - 1;
                            } else {
                                app.selected -= 1;
                            }
                            // keep offset so selected is visible (simple)
                            if app.sidebar_offset > app.selected {
                                app.sidebar_offset = app.selected;
                            }
                        }
                    }
                    KeyCode::Down => {
                        if app.focus == Focus::Sidebar {
                            app.selected = (app.selected + 1) % MENU_ITEMS.len();
                            // simple visibility clamp (offset moves down as needed)
                            if app.sidebar_offset + 1 < app.selected {
                                app.sidebar_offset = app.selected;
                            }
                        }
                    }
                    // Switch focus between menu and console
                    KeyCode::Tab => {
                        app.focus = if app.focus == Focus::Sidebar { Focus::Console } else { Focus::Sidebar };
                    },
                    // Delete character
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        // Execute when Enter is pressed
                        KeyCode::Enter => {
                            let cmd = app.input.trim().to_string();
                            // Store the command (even if empty) to keep a blank line
                            app.log.push(format!("{}", cmd));
                            app.input.clear();
                            // Process the command
                            process_command(&mut *app);
                        }
                        // Regular character input
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        _ => {}
                    }
                }
                // Mouse clicks are ignored in REPL mode – keep UI simple
                _ => {}
            }
        }

        // Check quit flag
        if state.lock().unwrap().running == false {
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
    // Determine layout proportions based on terminal height for responsiveness
    let total_height = f.size().height;
    let (header_pct, menu_pct, footer_pct) = if total_height < 20 {
        // Very small terminal – give more space to menu
        (20, 70, 10)
    } else if total_height < 30 {
        // Small terminal – balanced layout
        (25, 65, 10)
    } else {
        // Normal or large terminal – keep expanded header for version line
        (35, 55, 10)
    };
    // Layout: waveform (top), header, main (menu+log), footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Percentage(10),                // waveform
            Constraint::Percentage(header_pct),
            Constraint::Percentage(menu_pct),
            Constraint::Percentage(footer_pct),
        ])
        .split(f.size());

    // ── HEADER: Logo + Title + Author ──────────────────────────────────────────
    let header_block = Block::default()
        .borders(Borders::ALL);

    // ── WAVEFORM (scope) ─────────────────────────────────────
    let wf_width = f.size().width as usize;
    let wave_points: Vec<(f64, f64)> = (0..wf_width)
        .map(|x| {
            let xf = x as f64;
            let y = ((xf + app.logo_frame as f64) * 0.2).sin();
            (xf, y)
        })
        .collect();
    // Draw a moving sine‑wave using many tiny line segments
    let waveform = Canvas::default()
        .block(Block::default().borders(Borders::ALL).title("Scope"))
        .x_bounds([0.0, wf_width as f64])
        .y_bounds([-1.5, 1.5])
        .paint(|ctx| {
            // Iterate over consecutive points and draw a short line for each segment
            for pair in wave_points.windows(2) {
                let (x1, y1) = pair[0];
                let (x2, y2) = pair[1];
                ctx.draw(&CanvasLine {
                    x1,
                    y1,
                    x2,
                    y2,
                    color: Color::Cyan,
                });
            }
        });
    f.render_widget(waveform, chunks[0]);

    // Logo lines with color gradient (3D effect) – responsive truncation
    let logo_strings = load_logo_lines();
    let width = f.size().width as usize;
    // Animated logo (beige) – shift each line horizontally based on frame
    let logo_spans: Vec<Line> = logo_strings
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // If line longer than width, apply horizontal scrolling offset
            let display = if line.chars().count() > width {
                let offset = ((app.logo_frame + i) % width) as usize;
                let chars: Vec<char> = line.chars().collect();
                let mut shifted = String::new();
                for idx in 0..width {
                    let ch = chars[(offset + idx) % chars.len()];
                    shifted.push(ch);
                }
                shifted
            } else {
                line.clone()
            };
            let color = Color::Rgb(245, 222, 179); // beige
            Line::from(Span::styled(display, Style::default().fg(color).add_modifier(Modifier::BOLD)))
        })
        .collect();
    // Title
    let title_spans = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Developer from VietNam build with ♥️",
        Style::default(),
        )),
        Line::from(Span::styled(AUTHOR, Style::default())),
        Line::from(Span::styled(format!("megagate v{}", VERSION), Style::default())),
        ];


    let header_content: Vec<Line> = logo_spans
        .into_iter()
        .chain(title_spans.into_iter())
        .collect();

    let header_widget = Paragraph::new(header_content)
        .block(header_block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(header_widget, chunks[1]);

    // Split the middle (menu + log) area horizontally
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // menu pane
            Constraint::Percentage(60), // log pane
        ])
        .split(chunks[2]);

    // ── MAIN AREA – split horizontally into MENU and LOG panels �n    // Build full list of menu items with styling
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
    let visible_height = middle_chunks[0].height as usize;
    let end = std::cmp::min(visible_height, all_items.len());
    let items = &all_items[..end];
    // Render menu as a simple Paragraph (no auto‑scroll)
    let menu_lines: Vec<Line> = items.iter().enumerate().map(|(i, _item)| {
        // Extract the key and description from the ListItem's line (we stored them in order)
        // Since we built items from MENU_ITEMS, we can reuse that source directly.
        let (key, desc, _) = MENU_ITEMS[i];
        let style = if i == app.selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::styled(key, style),
            Span::raw(" "),
            Span::styled(desc, style),
        ])
    }).collect();
    let menu_paragraph = Paragraph::new(menu_lines)
        .block(Block::default().borders(Borders::ALL).title("Menu"));
    f.render_widget(menu_paragraph, middle_chunks[0]);

    // ── LOG PANEL (right side) ────────────────────────────────────────
        // Show a slice of the log based on scroll offset and pane height
        let console_visible = middle_chunks[1].height as usize;
        let log_start = app.console_offset;
        let log_end = std::cmp::min(log_start + console_visible, app.log.len());
        let log_slice = &app.log[log_start..log_end];
        let log_text = log_slice.join("\n");
        let log_paragraph = Paragraph::new(log_text)
            .block(Block::default().borders(Borders::ALL).title("Console"))
            .wrap(Wrap { trim: true });
    f.render_widget(log_paragraph, middle_chunks[1]);

    // ── FOOTER: Input Prompt ──────────────────────────────────────────────
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)])
        .split(chunks[3]);

    // Input line (prompt)
    let input_span = Span::styled(
        format!("> {}", app.input),
        Style::default(),
    );
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Input")
;
    let input_paragraph = Paragraph::new(input_span)
        .block(input_block);
    f.render_widget(input_paragraph, footer_chunks[0]);
}
