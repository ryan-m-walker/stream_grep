use std::env;
use std::process::{Command, Stdio};
use std::io::{self, BufRead, BufReader, Error, ErrorKind};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use nix::sys::signal::{kill, Signal};

use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    widgets::{Block, Borders, BorderType, List, ListItem, Paragraph},
    layout::{Layout, Constraint, Direction},
    style::{Color, Style, Modifier},
    text::{Span, Line},
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

enum AppEvent {
    Output(String),
    Tick,
    CommandExit(i32),
    ChildPid(nix::unistd::Pid),
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Panel {
    Header,
    Output,
    Preview,
}

struct App {
    output_lines: Vec<String>,
    running: bool,
    exit_code: Option<i32>,
    command_info: String,
    child_pid: Option<nix::unistd::Pid>,
    active_panel: Panel,
    search_query: String,
    cursor_position: usize,
}

impl App {
    fn new(command: &str, args: &[String]) -> Self {
        let args_str = args.join(" ");
        
        App {
            output_lines: Vec::new(),
            running: true,
            exit_code: None,
            command_info: format!("Running: {} {}", command, args_str),
            child_pid: None,
            active_panel: Panel::Output,
            search_query: String::new(),
            cursor_position: 0,
        }
    }
    
    fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Header => Panel::Output,
            Panel::Output => Panel::Preview,
            Panel::Preview => {
                // When activating the header panel, position cursor at the end of search query
                self.cursor_position = self.search_query.len();
                Panel::Header
            },
        };
    }
    
    fn prev_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Header => Panel::Preview,
            Panel::Output => {
                // When activating the header panel, position cursor at the end of search query
                self.cursor_position = self.search_query.len();
                Panel::Header
            },
            Panel::Preview => Panel::Output,
        };
    }
    
    fn get_block_style(&self, panel: Panel) -> Style {
        if self.active_panel == panel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Black)
        }
    }

    fn add_output(&mut self, line: String) {
        self.output_lines.push(line);
    }

    fn set_exit_code(&mut self, code: i32) {
        self.exit_code = Some(code);
        self.running = false;
    }
    
    fn set_child_pid(&mut self, pid: nix::unistd::Pid) {
        self.child_pid = Some(pid);
    }
}

fn main() -> Result<(), io::Error> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        return Err(Error::new(ErrorKind::InvalidInput, "Usage: cargo run <command> [args...]"));
    }

    let command = args[1].clone();
    let command_args = args[2..].to_vec();
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup app
    let mut app = App::new(&command, &command_args);

    // Setup channels
    let (tx, rx) = mpsc::channel();
    let tx_clone = tx.clone();

    // Spawn command in a thread
    thread::spawn(move || {
        let mut cmd = Command::new(&command);
        cmd.args(&command_args);
        cmd.stdout(Stdio::piped());

        match cmd.spawn() {
            Ok(mut child) => {
                // Send the child PID to the main thread
                let pid = child.id();
                let nix_pid = nix::unistd::Pid::from_raw(pid as i32);
                let _ = tx.send(AppEvent::ChildPid(nix_pid));
                
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().map_while(Result::ok) {
                        if tx.send(AppEvent::Output(line)).is_err() {
                            break;
                        }
                    }
                }
                
                match child.wait() {
                    Ok(status) => {
                        let code = status.code().unwrap_or(-1);
                        let _ = tx.send(AppEvent::CommandExit(code));
                    }
                    Err(_) => {
                        let _ = tx.send(AppEvent::CommandExit(-1));
                    }
                }
            }
            Err(e) => {
                // let _ = tx.send(AppEvent::Output(format!("Error: {}", e)));
                let _ = tx.send(AppEvent::CommandExit(-1));
            }
        };
    });

    // Ticker thread for UI updates
    thread::spawn(move || {
        loop {
            if tx_clone.send(AppEvent::Tick).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(250));
        }
    });
    
    loop {
        terminal.draw(|f| {
            let size = f.area();

            let main_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(1),
                ])
                .split(size);

            let output_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(main_layout[1]);

            // Create header block with rounded borders and search box
            let header_block = Block::default()
                .title(Span::styled(app.command_info.clone(), Style::default().fg(Color::Black)))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(app.get_block_style(Panel::Header));
            
            // Create a search input inside the header with cursor
            let header_content = if app.active_panel == Panel::Header {
                // Active search box with cursor
                let mut spans = vec![];
                
                // Display text with cursor
                if app.cursor_position >= app.search_query.len() {
                    // Cursor at the end
                    spans.push(Span::styled(&app.search_query, Style::default().fg(Color::Yellow)));
                    spans.push(Span::styled("█", Style::default().fg(Color::Yellow))); // Block cursor
                } else {
                    // Cursor in the middle
                    let (before, after) = app.search_query.split_at(app.cursor_position);
                    let mut after_chars = after.chars();
                    let cursor_char = after_chars.next().unwrap_or(' ');
                    let remaining: String = after_chars.collect();
                    
                    spans.push(Span::styled(before, Style::default().fg(Color::Yellow)));
                    spans.push(Span::styled(&cursor_char.to_string(), Style::default().fg(Color::Black).bg(Color::Yellow)));
                    spans.push(Span::styled(&remaining, Style::default().fg(Color::Yellow)));
                }
                
                Line::from(spans)
            } else {
                // Inactive search box (no cursor)
                Line::from(vec![
                    Span::styled(&app.search_query, Style::default().fg(Color::DarkGray))
                ])
            };
            
            let search_paragraph = Paragraph::new(header_content)
                .block(Block::default());
            
            // Render the header block first, then the search input inside it
            f.render_widget(header_block, main_layout[0]);
            f.render_widget(search_paragraph, Layout::default()
                .horizontal_margin(2)
                .vertical_margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(main_layout[0])[0]);

            // Create output list with rounded borders
            let mut items: Vec<ListItem> = app.output_lines
                .iter()
                .map(|line| ListItem::new(line.as_str()))
                .collect();

            if let Some(code) = app.exit_code {
                let exit_msg = format!("[Command exited with code: {}]", code);
                let exit_item = ListItem::new(exit_msg);
                items.push(exit_item);
            }

            let output_list = List::new(items)
                .block(Block::default()
                    .title("Output")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(app.get_block_style(Panel::Output)))
                .style(Style::default().fg(Color::Black));

            f.render_widget(output_list, output_layout[0]);

            // Create preview panel with rounded borders
            let preview_panel = Block::default()
                .title("Preview")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(app.get_block_style(Panel::Preview));

            f.render_widget(preview_panel, output_layout[1]);
        })?;

        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                    (KeyCode::Tab, KeyModifiers::SHIFT) => app.prev_panel(),
                    (KeyCode::Tab, _) => app.next_panel(),
                    (KeyCode::BackTab, _) => app.prev_panel(), // Many terminals send BackTab for Shift+Tab
                    (KeyCode::Char(c), _) if app.active_panel == Panel::Header => {
                        // Insert character at cursor position
                        app.search_query.insert(app.cursor_position, c);
                        app.cursor_position += 1;
                    },
                    (KeyCode::Backspace, _) if app.active_panel == Panel::Header && app.cursor_position > 0 => {
                        // Remove character before cursor
                        app.cursor_position -= 1;
                        app.search_query.remove(app.cursor_position);
                    },
                    (KeyCode::Delete, _) if app.active_panel == Panel::Header && app.cursor_position < app.search_query.len() => {
                        // Remove character at cursor
                        app.search_query.remove(app.cursor_position);
                    },
                    (KeyCode::Left, _) if app.active_panel == Panel::Header && app.cursor_position > 0 => {
                        app.cursor_position -= 1;
                    },
                    (KeyCode::Right, _) if app.active_panel == Panel::Header && app.cursor_position < app.search_query.len() => {
                        app.cursor_position += 1;
                    },
                    (KeyCode::Home, _) if app.active_panel == Panel::Header => {
                        app.cursor_position = 0;
                    },
                    (KeyCode::End, _) if app.active_panel == Panel::Header => {
                        app.cursor_position = app.search_query.len();
                    },
                    (KeyCode::Enter, _) if app.active_panel == Panel::Header => {
                        // User is done entering search query
                        app.active_panel = Panel::Output;
                    },
                    _ => {}
                }
            }
        }

        // Check for app events
        if let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::Output(line) => {
                    app.add_output(line);
                }
                AppEvent::CommandExit(code) => {
                    app.set_exit_code(code);
                }
                AppEvent::ChildPid(pid) => {
                    app.set_child_pid(pid);
                }
                AppEvent::Tick => {
                    // Just trigger a redraw
                }
            }
        }

        // If command has exited and user pressed 'q', break
        if !app.running && event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                    (KeyCode::Tab, KeyModifiers::SHIFT) => app.prev_panel(),
                    (KeyCode::Tab, _) => app.next_panel(),
                    (KeyCode::BackTab, _) => app.prev_panel(), // Many terminals send BackTab for Shift+Tab
                    (KeyCode::Char(c), _) if app.active_panel == Panel::Header => {
                        // Insert character at cursor position
                        app.search_query.insert(app.cursor_position, c);
                        app.cursor_position += 1;
                    },
                    (KeyCode::Backspace, _) if app.active_panel == Panel::Header && app.cursor_position > 0 => {
                        // Remove character before cursor
                        app.cursor_position -= 1;
                        app.search_query.remove(app.cursor_position);
                    },
                    (KeyCode::Delete, _) if app.active_panel == Panel::Header && app.cursor_position < app.search_query.len() => {
                        // Remove character at cursor
                        app.search_query.remove(app.cursor_position);
                    },
                    (KeyCode::Left, _) if app.active_panel == Panel::Header && app.cursor_position > 0 => {
                        app.cursor_position -= 1;
                    },
                    (KeyCode::Right, _) if app.active_panel == Panel::Header && app.cursor_position < app.search_query.len() => {
                        app.cursor_position += 1;
                    },
                    (KeyCode::Home, _) if app.active_panel == Panel::Header => {
                        app.cursor_position = 0;
                    },
                    (KeyCode::End, _) if app.active_panel == Panel::Header => {
                        app.cursor_position = app.search_query.len();
                    },
                    (KeyCode::Enter, _) if app.active_panel == Panel::Header => {
                        // User is done entering search query
                        app.active_panel = Panel::Output;
                    },
                    _ => {}
                }
            }
        }
    }

    // Kill child process if it exists and is still running
    if let Some(pid) = app.child_pid {
        // Send SIGTERM to terminate the process
        let _ = kill(pid, Signal::SIGTERM);
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
