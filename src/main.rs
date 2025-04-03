use std::env;
use std::process::{Child, Command, Stdio};
use std::io::{self, BufRead, BufReader, Error, ErrorKind};
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use logger::Logger;
use nix::sys::signal::{kill, Signal};

use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    widgets::{Block, Borders, BorderType, List, ListItem, Paragraph},
    layout::{Layout, Constraint, Direction, Alignment},
    style::{Style, Modifier},
    text::{Span, Line},
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

mod app;
mod logger;
use app::{App, AppEvent, Panel};

/// Handle keyboard input events. Returns true if the app should exit.
fn handle_key_event(app: &mut App, key: event::KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('q'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Tab, KeyModifiers::SHIFT) => app.prev_panel(),
        (KeyCode::Tab, _) => app.next_panel(),
        (KeyCode::BackTab, _) => app.prev_panel(), // Many terminals send BackTab for Shift+Tab
        (KeyCode::Char(c), _) if app.active_panel == Panel::Input => {
            // Insert character at cursor position
            app.search_query.insert(app.cursor_position, c);
            app.cursor_position += 1;
            // Update search with the new query
            app.update_search();
        },
        (KeyCode::Backspace, _) if app.active_panel == Panel::Input && app.cursor_position > 0 => {
            // Remove character before cursor
            app.cursor_position -= 1;
            app.search_query.remove(app.cursor_position);
            // Update search with the modified query
            app.update_search();
        },
        (KeyCode::Delete, _) if app.active_panel == Panel::Input && app.cursor_position < app.search_query.len() => {
            // Remove character at cursor
            app.search_query.remove(app.cursor_position);
            // Update search with the modified query
            app.update_search();
        },
        (KeyCode::Left, _) if app.active_panel == Panel::Input && app.cursor_position > 0 => {
            app.cursor_position -= 1;
        },
        (KeyCode::Right, _) if app.active_panel == Panel::Input && app.cursor_position < app.search_query.len() => {
            app.cursor_position += 1;
        },
        (KeyCode::Home, _) if app.active_panel == Panel::Input => {
            app.cursor_position = 0;
        },
        (KeyCode::End, _) if app.active_panel == Panel::Input => {
            app.cursor_position = app.search_query.len();
        },
        (KeyCode::Enter, _) if app.active_panel == Panel::Input => {
            // User is done entering search query
            app.update_search();
            app.active_panel = Panel::Output; // Move focus to the output panel with filtered results
        },
        (KeyCode::Down, _) if app.active_panel == Panel::Output => {
            app.select_next();
        },
        (KeyCode::Up, _) if app.active_panel == Panel::Output => {
            app.select_prev();
        },
        _ => {}
    }
    false
}

fn main() -> Result<(), io::Error> {
    let logger = Logger::new();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        return Err(Error::new(ErrorKind::InvalidInput, "Usage: cargo run <command> [args...]"));
    }

    let command = args[1].clone();
    let command_args = args[2..].to_vec();

    // Setup terminal
    // let mut stdout = io::stdout();
    // execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let mut terminal = ratatui::init();

    // enable_raw_mode()?;
    // let backend = CrosstermBackend::new(stdout);
    // let mut terminal = Terminal::new(backend)?;

    // Setup app
    let mut app = App::new(&command, &command_args);

    // Setup channels
    let (tx, rx) = mpsc::channel();
    let tx_clone = tx.clone();
    
    // Setup shared running flag for clean shutdown
    let running = Arc::new(AtomicBool::new(true));
    let command_running = running.clone();
    let ticker_running = running.clone();

    let mut thread_logger = logger.clone();

    // Spawn command in a thread
    let command_handle = thread::spawn(move || {
        let mut cmd = Command::new(&command);
        cmd.args(&command_args);
        cmd.stdout(Stdio::piped());

        match cmd.spawn() {
            Ok(mut child) => {
                thread_logger.info("Command spawned");

                // Send the child PID to the main thread
                let pid = child.id();
                let nix_pid = nix::unistd::Pid::from_raw(pid as i32);
                let _ = tx.send(AppEvent::ChildPid(nix_pid));

                // Check if we should continue running
                if !command_running.load(Ordering::SeqCst) {
                    // Try to kill the child process and exit
                    let _ = child.kill();
                    return;
                }

                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().map_while(Result::ok) {
                        // Check if we should continue running
                        if !command_running.load(Ordering::SeqCst) {
                            // Try to kill the child process and exit
                            let _ = child.kill();
                            return;
                        }
                        
                        if tx.send(AppEvent::Output(line)).is_err() {
                            break;
                        }
                    }
                }

                thread_logger.info("Command completed reading output");

                // One final check before waiting
                if !command_running.load(Ordering::SeqCst) {
                    // Try to kill the child process and exit
                    let _ = child.kill();
                    return;
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
                let _ = tx.send(AppEvent::Output(format!("Error: {}", e)));
                let _ = tx.send(AppEvent::CommandExit(-1));
            }
        };
    });

    // Ticker thread for UI updates
    let ticker_handle = thread::spawn(move || {
        while ticker_running.load(Ordering::SeqCst) {
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
                // .title(Line::from("Grep").centered().style(Style::default().fg(app.get_fg_color())))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(app.get_block_style(Panel::Input));
            
            // Create a search input inside the header with cursor
            let header_content = if app.active_panel == Panel::Input {
                // Active search box with cursor
                let mut spans = vec![];
                
                // Display text with cursor
                if app.cursor_position >= app.search_query.len() {
                    // Cursor at the end
                    spans.push(Span::styled(format!("> {}", app.search_query.clone()), Style::default().fg(app.get_hl_color())));
                    spans.push(Span::styled("█".to_string(), Style::default().fg(app.get_hl_color()))); // Block cursor
                } else {
                    // Cursor in the middle
                    let (before, after) = app.search_query.split_at(app.cursor_position);
                    let mut after_chars = after.chars();
                    let cursor_char = after_chars.next().unwrap_or(' ');
                    let remaining: String = after_chars.collect();

                    spans.push(Span::styled(before.to_string(), Style::default().fg(app.get_hl_color())));
                    let cursor_text = cursor_char.to_string();
                    spans.push(Span::styled(cursor_text, Style::default().fg(app.get_fg_color()).bg(app.get_hl_color())));
                    spans.push(Span::styled(remaining, Style::default().fg(app.get_hl_color())));
                }

                Line::from(spans)
            } else {
                // Inactive search box (no cursor)
                Line::from(vec![
                    Span::styled(format!("> {}", app.search_query.clone()), Style::default().fg(app.get_fg_color()))
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

            // Create filtered output list with rounded borders and highlight selected item
            let filtered_items: Vec<ListItem> = app.filtered_lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    let mut spans = Vec::new();
                    
                    // Extract the line number and content parts (format is: "12345 | content")
                    if let Some(pipe_pos) = line.find('|') {
                        let (number_part, content) = line.split_at(pipe_pos + 1);
                        
                        // Add line number in dark gray
                        spans.push(Span::styled(
                            number_part.to_string(),
                            Style::default().fg(app.get_line_number_color())
                        ));
                        
                        // Add content with appropriate styling
                        spans.push(Span::raw(content.to_string()));
                        
                        // Create the item with proper styling
                        if i == app.selected_index && app.active_panel == Panel::Output {
                            // Highlight the selected item when output panel is active
                            ListItem::new(Line::from(spans)).style(Style::default().fg(app.get_hl_color()))
                        } else {
                            ListItem::new(Line::from(spans))
                        }
                    } else {
                        // Fallback if pipe not found
                        if i == app.selected_index && app.active_panel == Panel::Output {
                            ListItem::new(line.as_str()).style(Style::default().fg(app.get_hl_color()))
                        } else {
                            ListItem::new(line.as_str())
                        }
                    }
                })
                .collect();

            // Append exit code message if available
            let mut output_items = filtered_items;
            if let Some(code) = app.exit_code {
                let exit_msg = format!("[Command exited with code: {}]", code);
                let exit_item = ListItem::new(exit_msg);
                output_items.push(exit_item);
            }

            let output_title = if app.search_query.is_empty() {
                "All Output"
            } else {
                "Filtered Results"
            };

            let output_list = List::new(output_items)
                .block(Block::default()
                    .title(output_title)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(app.get_block_style(Panel::Output)))
                .style(Style::default().fg(app.get_fg_color()));

            f.render_widget(output_list, output_layout[0]);

            // Only show preview content if there's a search query
            if app.search_query.is_empty() {
                // Empty preview panel with a message
                let empty_preview = Paragraph::new("Enter a search pattern in the input box")
                    .block(Block::default()
                        .title("Preview")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(app.get_block_style(Panel::Preview)))
                    .style(Style::default().fg(app.get_fg_color()))
                    .alignment(Alignment::Center);
                
                f.render_widget(empty_preview, output_layout[1]);
            } else {
                // Calculate available height for the preview content
                let preview_height = output_layout[1].height.saturating_sub(2); // Subtract borders
                let (context_lines, selected_idx) = app.get_visible_context(preview_height as usize);
                
                // Create styled context items with highlighted matches
                let context_items: Vec<ListItem> = context_lines
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let mut spans = Vec::new();
                        
                        // Get the line without the prefix (first 2 chars)
                        let (prefix, content) = line.split_at(2);
                        spans.push(Span::raw(prefix)); // Add prefix first
                        
                        // Extract the line number and content parts (format is: "  12345 | content")
                        if let Some(pipe_pos) = content.find('|') {
                            let (number_part, actual_content) = content.split_at(pipe_pos + 1);
                            
                            // Add line number in dark gray
                            spans.push(Span::styled(
                                number_part.to_string(),
                                Style::default().fg(app.get_line_number_color())
                            ));
                            
                            // Find matches to highlight in the actual content
                            let matches = app.find_matches_in_line(actual_content);
                            
                            if matches.is_empty() {
                                // No matches, add the whole content
                                spans.push(Span::raw(actual_content.to_string()));
                            } else {
                                // Add segments with highlighting for matches
                                let mut last_end = 0;
                                for (start, end) in matches {
                                    // Add text before match
                                    if start > last_end {
                                        spans.push(Span::raw(actual_content[last_end..start].to_string()));
                                    }
                                    
                                    // Add highlighted match
                                    let match_style = Style::default()
                                        .fg(app.get_hl_color())
                                        .add_modifier(Modifier::BOLD);
                                    spans.push(Span::styled(actual_content[start..end].to_string(), match_style));
                                    
                                    last_end = end;
                                }
                                
                                // Add remaining text after last match
                                if last_end < actual_content.len() {
                                    spans.push(Span::raw(actual_content[last_end..].to_string()));
                                }
                            }
                        } else {
                            // Fallback if pipe not found
                            spans.push(Span::raw(content.to_string()));
                        }
                        
                        // Create a list item with all the styled spans
                        let line_style = if selected_idx.is_some() && i == selected_idx.unwrap() {
                            // Make the selected line stand out more
                            Style::default().fg(app.get_fg_color()).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(app.get_fg_color())
                        };
                        
                        ListItem::new(Line::from(spans)).style(line_style)
                    })
                    .collect();
                
                let preview_title = if app.filtered_indices.is_empty() || app.selected_index >= app.filtered_indices.len() {
                    "Preview".to_string()
                } else {
                    let line_num = app.filtered_indices[app.selected_index] + 1; // +1 for 1-based line numbering
                    format!("Preview (line {})", line_num)
                };
                
                let preview_list = List::new(context_items)
                    .block(Block::default()
                        .title(preview_title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(app.get_block_style(Panel::Preview)))
                    .style(Style::default().fg(app.get_fg_color()));

                f.render_widget(preview_list, output_layout[1]);
            }
        })?;

        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if handle_key_event(&mut app, key) {
                    break;
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

        // If command has exited, check for key events
        if !app.running && event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if handle_key_event(&mut app, key) {
                    break;
                }
            }
        }
    }

    // Signal all threads to stop
    running.store(false, Ordering::SeqCst);
    
    // Kill child process if it exists and is still running
    if let Some(pid) = app.child_pid {
        // Send SIGTERM to terminate the process
        let _ = kill(pid, Signal::SIGTERM);
    }
    
    // Give threads a moment to clean up
    thread::sleep(Duration::from_millis(100));
    
    // Try to gracefully shut down the command thread with timeout
    let _ = command_handle.join();
    
    // Try to gracefully shut down the ticker thread with timeout
    let _ = ticker_handle.join();

    ratatui::restore();

    // TODO: write output to stdout
    for line in app.output_lines {
        println!("{}", line);
    }

    println!("---");

    for log in logger.dump() {
        println!("{}", log);
    }

    Ok(())
}
