//! Interactive selection widget built with Ratatui and Crossterm.
//!
//! The pure `Selection` state is independent of the terminal, making it easy
//! to test navigation without requiring a real TTY.

use std::io::{self, IsTerminal, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

/// Selection index with safe wrap-around navigation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    index: usize,
}

impl Selection {
    /// Move to the next item, wrapping from the last item to the first.
    pub fn next(&mut self, len: usize) {
        if len > 0 {
            self.index = (self.index + 1) % len;
        }
    }

    /// Move to the previous item, wrapping from the first item to the last.
    pub fn previous(&mut self, len: usize) {
        if len > 0 {
            self.index = (self.index + len - 1) % len;
        }
    }

    /// Return the selected position in the original items array.
    pub fn index(self) -> usize {
        self.index
    }
}

/// Own the terminal session so Drop restores it even if rendering fails.
///
/// Rust calls `drop` when the guard leaves scope, including when `?` returns
/// early. This is the simplest form of RAII (resource acquisition is initialization).
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    /// Enter raw mode and the alternate screen, then initialize Ratatui.
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }

        match Terminal::new(CrosstermBackend::new(io::stdout())) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    /// Always attempt both cleanup operations, even if the first one fails.
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}

/// Run a single-choice UI and return its selected index, or None on cancel.
pub fn choose(items: &[String], title: Option<&str>) -> io::Result<Option<usize>> {
    // This milestone uses stdout for the TUI. A later milestone will open
    // /dev/tty so redirected stdout can remain a clean data channel.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("choose requires an interactive terminal"));
    }

    let mut session = TerminalSession::new()?;
    let mut selection = Selection::default();

    loop {
        session.terminal.draw(|frame| {
            // Reserve one line for the instructions and the rest for the list.
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(frame.area());

            let rows = items.iter().map(|item| ListItem::new(item.as_str()));
            let list = List::new(rows)
                .block(Block::default().title(title.unwrap_or("Choose")).borders(Borders::ALL))
                .highlight_symbol("❯ ")
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            // Ratatui owns the visual state only for this draw call.
            let mut list_state = ListState::default().with_selected(Some(selection.index()));
            frame.render_stateful_widget(list, chunks[0], &mut list_state);
            frame.render_widget(
                Paragraph::new("↑/↓ or j/k: navigate  •  Enter: select  •  Esc/q: cancel"),
                chunks[1],
            );
        })?;

        // Ignore key-release events, which some terminals emit separately.
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Down | KeyCode::Char('j') => selection.next(items.len()),
                KeyCode::Up | KeyCode::Char('k') => selection.previous(items.len()),
                KeyCode::Enter => return Ok(Some(selection.index())),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Selection;

    /// A pure state test needs no terminal and runs in ordinary CI.
    #[test]
    fn navigation_wraps_in_both_directions() {
        let mut state = Selection::default();
        state.previous(3);
        assert_eq!(state.index(), 2);
        state.next(3);
        assert_eq!(state.index(), 0);
    }

    /// Empty lists must never cause division by zero.
    #[test]
    fn empty_navigation_is_safe() {
        let mut state = Selection::default();
        state.next(0);
        state.previous(0);
        assert_eq!(state.index(), 0);
    }
}
