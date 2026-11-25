//! Interactive selection widget built with Ratatui and Crossterm.
//!
//! The pure `Selection` state is independent of the terminal, making it easy
//! to test navigation without requiring a real TTY.

use std::io::{self, IsTerminal, Stderr};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
    Frame, Terminal, TerminalOptions, Viewport,
};
use unicode_width::UnicodeWidthStr;

use crate::registry::{AxisPosition, ChooseArgs, TextAlignment};

/// Selection index with safe wrap-around navigation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    index: usize,
}

impl Selection {
    /// Start at the requested item, clamping invalid indices to the last item.
    pub fn new(index: usize, len: usize) -> Self {
        Self {
            index: index.min(len.saturating_sub(1)),
        }
    }

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
    terminal: Terminal<CrosstermBackend<Stderr>>,
    alternate_screen: bool,
}

impl TerminalSession {
    /// Enter raw mode and initialize either an inline or fullscreen terminal.
    fn new(fullscreen: bool, inline_height: u16) -> io::Result<Self> {
        enable_raw_mode()?;

        if fullscreen {
            if let Err(error) = execute!(io::stderr(), EnterAlternateScreen) {
                let _ = disable_raw_mode();
                return Err(error);
            }
        }

        let backend = CrosstermBackend::new(io::stderr());
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
            Ok(terminal) => Ok(Self {
                terminal,
                alternate_screen: fullscreen,
            }),
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
    /// Always attempt both cleanup operations, even if the first one fails.
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        if self.alternate_screen {
            let _ = execute!(io::stderr(), LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }
}

/// Run a single-choice UI and return its selected index, or None on cancel.
pub fn choose(args: &ChooseArgs) -> io::Result<Option<usize>> {
    // The TUI renders to stderr, preserving stdout for the selected value.
    // Keyboard events still require an interactive stdin; /dev/tty is future work.
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(io::Error::other("choose requires an interactive terminal"));
    }

    let terminal_width = size().map(|(width, _)| width).unwrap_or(80);
    let geometry = Geometry::resolve(args, terminal_width);
    let mut session = TerminalSession::new(args.viewport.fullscreen, geometry.viewport_height)?;
    let mut selection = Selection::new(args.initial, args.items.len());
    let mut list_state = ListState::default().with_selected(Some(selection.index()));

    loop {
        session.terminal.draw(|frame| {
            render(frame, args, geometry, &mut list_state);
        })?;

        // Ignore key-release events, which some terminals emit separately.
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Down | KeyCode::Char('j') => selection.next(args.items.len()),
                KeyCode::Up | KeyCode::Char('k') => selection.previous(args.items.len()),
                KeyCode::Enter => return Ok(Some(selection.index())),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                _ => {}
            }

            // Keeping the state object alive lets Ratatui preserve its scroll offset.
            list_state.select(Some(selection.index()));
        }
    }
}

/// Resolved cell geometry, calculated before rendering to keep layout deterministic.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    width: u16,
    viewport_height: u16,
    instructions: bool,
}

impl Geometry {
    /// Size the component from its content while respecting explicit overrides.
    fn resolve(args: &ChooseArgs, terminal_width: u16) -> Self {
        let border = args.block.border.inset();
        let prompt_width = UnicodeWidthStr::width(args.prompt.as_str()) as u16;
        let longest_item = args
            .items
            .iter()
            .map(|item| UnicodeWidthStr::width(item.as_str()) as u16)
            .max()
            .unwrap_or(0);
        let list_width = longest_item
            .saturating_add(prompt_width)
            .saturating_add(args.block.padding.saturating_mul(2))
            .saturating_add(border);
        let title_width = (UnicodeWidthStr::width(args.title.as_str()) as u16)
            .saturating_add(args.block.padding.saturating_mul(2))
            .saturating_add(border);
        let instruction_width = if !args.no_instructions {
            UnicodeWidthStr::width(args.instruction.as_str()) as u16
        } else {
            0
        };
        let natural_width = list_width
            .max(title_width)
            .max(instruction_width)
            .clamp(24, 72);
        let width = args
            .viewport
            .width
            .unwrap_or(natural_width)
            .clamp(1, terminal_width.max(1));

        let instructions = !args.no_instructions && !args.instruction.is_empty();
        let natural_list_height = (args.items.len() as u16).clamp(1, 10);
        let natural_panel_height = natural_list_height.saturating_add(border);
        let requested_total = args
            .viewport
            .height
            .unwrap_or_else(|| natural_panel_height.saturating_add(u16::from(instructions)));
        let minimum_height = border
            .saturating_add(1)
            .saturating_add(u16::from(instructions));
        let viewport_height = requested_total.max(minimum_height);
        Self {
            width,
            viewport_height,
            instructions,
        }
    }
}

/// Render the component from typed arguments without reading terminal events.
fn render(
    frame: &mut Frame<'_>,
    args: &ChooseArgs,
    geometry: Geometry,
    list_state: &mut ListState,
) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(args.style.background.0)),
        area,
    );

    let content_height = geometry.viewport_height.min(area.height);
    let content = positioned_rect(
        area,
        geometry.width.min(area.width),
        content_height,
        args.viewport.position,
        if args.viewport.fullscreen {
            args.viewport.vertical_position
        } else {
            AxisPosition::Start
        },
    );
    frame.render_widget(Clear, content);

    let chunks = if geometry.instructions && content.height > 1 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(content)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(0)])
            .split(content)
    };

    let borders = if args.block.border.ratatui().is_some() {
        Borders::ALL
    } else {
        Borders::NONE
    };
    let mut block = Block::default()
        .title(args.title.as_str())
        .title_alignment(match args.block.title_alignment {
            TextAlignment::Left => Alignment::Left,
            TextAlignment::Center => Alignment::Center,
            TextAlignment::Right => Alignment::Right,
        })
        .title_style(
            Style::default()
                .fg(args.block.title_color.0)
                .add_modifier(Modifier::BOLD),
        )
        .borders(borders)
        .border_style(Style::default().fg(args.block.border_color.0))
        .padding(Padding::horizontal(args.block.padding))
        .style(
            Style::default()
                .fg(args.style.foreground.0)
                .bg(args.style.background.0),
        );
    if let Some(border_type) = args.block.border.ratatui() {
        block = block.border_type(border_type);
    }

    let rows = args.items.iter().map(|item| ListItem::new(item.as_str()));
    let list = List::new(rows)
        .block(block)
        .highlight_symbol(args.prompt.as_str())
        .highlight_spacing(args.style.highlight_spacing.into())
        .highlight_style(
            Style::default()
                .fg(args.style.selected_fg.0)
                .bg(args.style.selected_bg.0)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, chunks[0], list_state);

    if geometry.instructions && chunks[1].height > 0 {
        frame.render_widget(
            Paragraph::new(args.instruction.as_str())
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(args.style.instruction_fg.0)
                        .bg(args.style.background.0),
                ),
            chunks[1],
        );
    }
}

/// Position a rectangle without allocating a nested layout tree.
fn positioned_rect(
    area: Rect,
    width: u16,
    height: u16,
    horizontal: AxisPosition,
    vertical: AxisPosition,
) -> Rect {
    let x = aligned_offset(area.width, width, horizontal);
    let y = aligned_offset(area.height, height, vertical);
    Rect::new(
        area.x.saturating_add(x),
        area.y.saturating_add(y),
        width,
        height,
    )
    .inner(Margin::new(0, 0))
}

/// Calculate the free space before an aligned child rectangle.
fn aligned_offset(available: u16, requested: u16, position: AxisPosition) -> u16 {
    let free = available.saturating_sub(requested);
    match position {
        AxisPosition::Start => 0,
        AxisPosition::Center => free / 2,
        AxisPosition::End => free,
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
