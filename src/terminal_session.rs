//! Shared terminal lifecycle for every interactive rodtui component.

use std::io::{self, IsTerminal};

use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::terminal_backend::TerminalBackend;

/// RAII terminal session that restores raw mode, cursor, paste, and screen state.
pub struct TerminalSession {
    pub terminal: Terminal<TerminalBackend>,
    alternate_screen: bool,
}

impl TerminalSession {
    /// Verify a controlling terminal and initialize an inline or fullscreen surface.
    pub fn new(fullscreen: bool, inline_height: u16, error_hint: &str) -> io::Result<Self> {
        if !io::stderr().is_terminal() {
            return Err(io::Error::other(error_hint));
        }

        enable_raw_mode()?;
        if fullscreen {
            if let Err(error) = execute!(io::stderr(), EnterAlternateScreen) {
                let _ = disable_raw_mode();
                return Err(error);
            }
        }

        let backend = TerminalBackend::new();
        let result = if fullscreen {
            Terminal::new(backend)
        } else {
            Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Inline(inline_height),
                },
            )
        };

        match result {
            Ok(terminal) => {
                if let Err(error) = execute!(io::stderr(), EnableBracketedPaste) {
                    if fullscreen {
                        let _ = execute!(io::stderr(), LeaveAlternateScreen);
                    }
                    let _ = disable_raw_mode();
                    return Err(error);
                }
                Ok(Self {
                    terminal,
                    alternate_screen: fullscreen,
                })
            }
            Err(error) => {
                if fullscreen {
                    let _ = execute!(io::stderr(), LeaveAlternateScreen);
                }
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(io::stderr(), DisableBracketedPaste);
        if self.alternate_screen {
            let _ = execute!(io::stderr(), LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }
}
