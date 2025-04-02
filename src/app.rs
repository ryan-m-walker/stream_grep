use ratatui::style::{Color, Style, Modifier};
use grep::regex::RegexMatcher;
use grep::matcher::Matcher;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Panel {
    Input,
    Output,
    Preview,
}

pub enum AppEvent {
    Output(String),
    Tick,
    CommandExit(i32),
    ChildPid(nix::unistd::Pid),
}

pub struct App {
    pub output_lines: Vec<String>,
    pub filtered_lines: Vec<String>, 
    pub filtered_indices: Vec<usize>,  // Store original indices of filtered lines
    pub selected_index: usize,        // Currently selected index in filtered results
    pub preview_scroll: usize,        // Scroll position for the preview panel
    pub running: bool,
    pub exit_code: Option<i32>,
    pub command_info: String,
    pub child_pid: Option<nix::unistd::Pid>,
    pub active_panel: Panel,
    pub search_query: String,
    pub cursor_position: usize,
    pub theme_mode: dark_light::Mode,
}

impl App {
    pub fn new(command: &str, args: &[String]) -> Self {
        let args_str = args.join(" ");
        let theme = dark_light::detect().unwrap_or(dark_light::Mode::Light);

        App {
            output_lines: Vec::new(),
            filtered_lines: Vec::new(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            preview_scroll: 0,
            running: true,
            exit_code: None,
            command_info: format!("{} {}", command, args_str),
            child_pid: None,
            active_panel: Panel::Input,
            search_query: String::new(),
            cursor_position: 0,
            theme_mode: theme,
        }
    }

    pub fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Input => Panel::Output,
            Panel::Output => Panel::Preview,
            Panel::Preview => {
                // When activating the header panel, position cursor at the end of search query
                self.cursor_position = self.search_query.len();
                Panel::Input
            },
        };
    }

    pub fn prev_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Input => Panel::Preview,
            Panel::Output => {
                // When activating the header panel, position cursor at the end of search query
                self.cursor_position = self.search_query.len();
                Panel::Input
            },
            Panel::Preview => Panel::Output,
        };
    }

    pub fn get_block_style(&self, panel: Panel) -> Style {
        if self.active_panel == panel {
            Style::default().fg(self.get_hl_color()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.get_fg_color())
        }
    }

    pub fn add_output(&mut self, line: String) {
        let current_line_index = self.output_lines.len();
        self.output_lines.push(line.clone());
        
        // Always add lines if no search query (show all)
        if self.search_query.is_empty() {
            // Add line number as prefix
            let line_num = current_line_index + 1;
            self.filtered_lines.push(format!("{:5} | {}", line_num, line));
            self.filtered_indices.push(current_line_index);
            return;
        }
        
        // If we have a search query, check if the new line matches
        if let Ok(matcher) = RegexMatcher::new(&self.search_query) {
            if matcher.is_match(line.as_bytes()).unwrap_or(false) {
                // Add line number as prefix
                let line_num = current_line_index + 1;
                self.filtered_lines.push(format!("{:5} | {}", line_num, line));
                self.filtered_indices.push(current_line_index);
            }
        }
    }

    pub fn set_exit_code(&mut self, code: i32) {
        self.exit_code = Some(code);
        self.running = false;
    }

    pub fn set_child_pid(&mut self, pid: nix::unistd::Pid) {
        self.child_pid = Some(pid);
    }

    pub fn get_fg_color(&self) -> Color {
        match self.theme_mode {
            dark_light::Mode::Dark => Color::White,
            dark_light::Mode::Light => Color::Black,
            dark_light::Mode::Unspecified => Color::Black,
        }
    }

    pub fn get_bg_color(&self) -> Color {
        match self.theme_mode {
            dark_light::Mode::Dark => Color::Black,
            dark_light::Mode::Light => Color::White,
            dark_light::Mode::Unspecified => Color::White,
        }
    }
    
    pub fn get_line_number_color(&self) -> Color {
        match self.theme_mode {
            dark_light::Mode::Dark => Color::DarkGray,
            dark_light::Mode::Light => Color::DarkGray,
            dark_light::Mode::Unspecified => Color::DarkGray,
        }
    }

    pub fn get_hl_color(&self) -> Color {
        Color::Yellow
    }
    
    pub fn select_next(&mut self) {
        if !self.filtered_lines.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.filtered_lines.len();
            self.update_preview_scroll();
        }
    }
    
    pub fn select_prev(&mut self) {
        if !self.filtered_lines.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                self.selected_index = self.filtered_lines.len() - 1;
            }
            self.update_preview_scroll();
        }
    }
    
    // Update the preview scroll position to keep the selected line in view with padding
    fn update_preview_scroll(&mut self) {
        if self.filtered_indices.is_empty() || self.selected_index >= self.filtered_indices.len() {
            return;
        }
        
        let original_index = self.filtered_indices[self.selected_index];
        // Position the selected line with a padding of 3 lines from the top
        let padding = 3;
        
        if original_index >= padding {
            self.preview_scroll = original_index - padding;
        } else {
            self.preview_scroll = 0;
        }
    }
    
    pub fn get_context_for_selected(&self) -> (Vec<String>, Option<usize>) {
        if self.filtered_indices.is_empty() || self.selected_index >= self.filtered_indices.len() {
            return (Vec::new(), None);
        }
        
        let original_index = self.filtered_indices[self.selected_index];
        
        // Show all output lines with line numbers
        let mut context = Vec::new();
        for i in 0..self.output_lines.len() {
            let prefix = if i == original_index { "> " } else { "  " };
            // Add line number (1-based) as prefix
            let line_number = i + 1;
            context.push(format!("{}{:5} | {}", prefix, line_number, self.output_lines[i]));
        }
        
        // Return all lines and the selected line's position relative to visible area
        let selected_visible_index = original_index.saturating_sub(self.preview_scroll);
        (context, Some(selected_visible_index))
    }
    
    // Get visible context lines based on scroll position
    pub fn get_visible_context(&self, height: usize) -> (Vec<String>, Option<usize>) {
        let (all_context, selected_idx) = self.get_context_for_selected();
        
        if all_context.is_empty() {
            return (Vec::new(), None);
        }
        
        // Calculate visible range
        let start = self.preview_scroll;
        let end = std::cmp::min(start + height, all_context.len());
        
        // Extract visible lines
        let visible_lines = all_context[start..end].to_vec();
        
        // Adjust selected index for visible portion
        let visible_selected_idx = if let Some(idx) = selected_idx {
            if idx >= start && idx < end {
                Some(idx - start)
            } else {
                None
            }
        } else {
            None
        };
        
        (visible_lines, visible_selected_idx)
    }
    
    // Get matches for a line to be used for highlighting
    pub fn find_matches_in_line(&self, line: &str) -> Vec<(usize, usize)> {
        if self.search_query.is_empty() {
            return Vec::new();
        }
        
        match RegexMatcher::new(&self.search_query) {
            Ok(matcher) => {
                let mut matches = Vec::new();
                
                // Create a sink that captures match offsets
                let mut match_sink = |m: grep::matcher::Match| {
                    matches.push((m.start(), m.end()));
                    true
                };
                
                // Search the line for matches and capture their offsets
                let _ = matcher.find_iter(line.as_bytes(), &mut match_sink);
                
                matches
            },
            Err(_) => Vec::new(),
        }
    }
    
    pub fn update_search(&mut self) {
        // Clear the filtered lines and indices
        self.filtered_lines.clear();
        self.filtered_indices.clear();
        self.selected_index = 0;
        
        // If search query is empty, show all lines in filtered view
        if self.search_query.is_empty() {
            for (i, line) in self.output_lines.iter().enumerate() {
                // Add line number as prefix
                let line_num = i + 1;
                self.filtered_lines.push(format!("{:5} | {}", line_num, line));
                self.filtered_indices.push(i);
            }
            // Initialize preview scroll
            self.update_preview_scroll();
            return;
        }
        
        // Try to create a regex matcher from the search query
        match RegexMatcher::new(&self.search_query) {
            Ok(matcher) => {
                // Filter lines that match the regex
                for (i, line) in self.output_lines.iter().enumerate() {
                    if matcher.is_match(line.as_bytes()).unwrap_or(false) {
                        // Add line number as prefix
                        let line_num = i + 1;
                        self.filtered_lines.push(format!("{:5} | {}", line_num, line));
                        self.filtered_indices.push(i);
                    }
                }
            },
            Err(_) => {
                // Invalid regex, show all lines in filtered view
                for (i, line) in self.output_lines.iter().enumerate() {
                    // Add line number as prefix
                    let line_num = i + 1;
                    self.filtered_lines.push(format!("{:5} | {}", line_num, line));
                    self.filtered_indices.push(i);
                }
            }
        }
        
        // Initialize preview scroll to show selected line
        self.update_preview_scroll();
    }
}
