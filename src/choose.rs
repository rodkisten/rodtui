//! Interactive selection widget built with Ratatui and Crossterm.
//!
//! The pure `Selection` state is independent of the terminal, making it easy
//! to test navigation without requiring a real TTY.

use std::{
    borrow::Cow,
    io::{self, IsTerminal},
};

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
    Frame, Terminal, TerminalOptions, Viewport,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    choice::{ChoiceOutcome, ChoiceState},
    registry::{AxisPosition, ChooseArgs, InstructionMode, TextAlignment, ThemePalette},
    terminal_backend::TerminalBackend,
};

const FULL_INSTRUCTION: &str = "↑/↓ or j/k: navigate  •  Enter: select  •  Esc/q: cancel";
const COMPACT_INSTRUCTION: &str = "↑↓ move  •  ↵ select  •  esc cancel";
const MINIMAL_INSTRUCTION: &str = "↑↓ move  •  ↵ select";

const FILTER_FULL_INSTRUCTION: &str =
    "type: filter  •  ↑/↓: move  •  Enter: select  •  Esc: cancel";
const FILTER_COMPACT_INSTRUCTION: &str = "type filter  •  ↑↓ move  •  ↵ select";
const FILTER_MINIMAL_INSTRUCTION: &str = "type filter  •  ↑↓  •  ↵";
const MULTI_FULL_INSTRUCTION: &str =
    "↑/↓: move  •  Space/Tab: mark  •  Enter: accept  •  Esc: cancel";
const MULTI_COMPACT_INSTRUCTION: &str = "↑↓ move  •  space mark  •  ↵ accept";
const MULTI_MINIMAL_INSTRUCTION: &str = "↑↓  •  space  •  ↵";

/// Terminal result with distinct success, no-match, and cancellation contracts.
#[derive(Debug)]
pub enum ChooseResult {
    Accepted(ChoiceOutcome),
    NoMatch,
    Cancelled,
}

/// Own the terminal session so Drop restores it even if rendering fails.
///
/// Rust calls `drop` when the guard leaves scope, including when `?` returns
/// early. This is the simplest form of RAII (resource acquisition is initialization).
struct TerminalSession {
    terminal: Terminal<TerminalBackend>,
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
    /// Always attempt both cleanup operations, even if the first one fails.
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(io::stderr(), DisableBracketedPaste);
        if self.alternate_screen {
            let _ = execute!(io::stderr(), LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }
}

/// Run the complete choose interaction or accept a unique filtered match directly.
pub fn choose(args: &ChooseArgs, state: ChoiceState) -> io::Result<ChooseResult> {
    if args.select_1 {
        return Ok(match state.visible_indices().len() {
            0 => ChooseResult::NoMatch,
            1 => ChooseResult::Accepted(
                state
                    .accept()
                    .expect("one visible item always has an accepted outcome"),
            ),
            _ => interactive_choose(args, state)?,
        });
    }
    interactive_choose(args, state)
}

/// Own the terminal event loop while the pure state handles all transitions.
fn interactive_choose(args: &ChooseArgs, mut state: ChoiceState) -> io::Result<ChooseResult> {
    // Piped stdin is already consumed before this point. Crossterm opens the
    // controlling terminal on Unix, while stderr remains the rendering target.
    if !io::stderr().is_terminal() {
        return Err(io::Error::other(
            "interactive choose requires stderr connected to a terminal; use --select-1 for noninteractive selection",
        ));
    }

    let terminal_width = size().map(|(width, _)| width).unwrap_or(80);
    let mut geometry = Geometry::resolve(args, &state, terminal_width);
    let theme = resolve_theme(args);
    let mut session = TerminalSession::new(args.viewport.fullscreen, geometry.viewport_height)?;
    let mut list_state = ListState::default().with_selected(selected_cursor(&state));

    loop {
        session.terminal.draw(|frame| {
            render(frame, args, &state, geometry, theme, &mut list_state);
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Down | KeyCode::Char('n') if control => state.next(),
                    KeyCode::Up | KeyCode::Char('p') if control => state.previous(),
                    KeyCode::Down => state.next(),
                    KeyCode::Up => state.previous(),
                    KeyCode::Enter => {
                        return Ok(state
                            .accept()
                            .map(ChooseResult::Accepted)
                            .unwrap_or(ChooseResult::NoMatch));
                    }
                    KeyCode::Esc => return Ok(ChooseResult::Cancelled),
                    KeyCode::Char('c') if control => return Ok(ChooseResult::Cancelled),
                    KeyCode::Char('u') if control => state.clear_query(),
                    KeyCode::Backspace => state.pop_query(),
                    KeyCode::Tab if state.multi() => {
                        state.toggle_current();
                        state.next();
                    }
                    KeyCode::BackTab if state.multi() => {
                        state.toggle_current();
                        state.previous();
                    }
                    KeyCode::Char(' ') if state.multi() && !state.filter_enabled() => {
                        state.toggle_current();
                    }
                    KeyCode::Char(character) if state.filter_enabled() && !control => {
                        state.push_query(character);
                    }
                    KeyCode::Char('j') if !state.filter_enabled() => state.next(),
                    KeyCode::Char('k') if !state.filter_enabled() => state.previous(),
                    KeyCode::Char('q') if !state.filter_enabled() => {
                        return Ok(ChooseResult::Cancelled);
                    }
                    _ => {}
                }
            }
            Event::Paste(text) => state.append_query(&text),
            Event::Resize(width, _) => {
                geometry = Geometry::resolve(args, &state, width);
            }
            _ => {}
        }

        list_state.select(selected_cursor(&state));
    }
}

fn selected_cursor(state: &ChoiceState) -> Option<usize> {
    (!state.visible_indices().is_empty()).then_some(state.cursor())
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
    fn resolve(args: &ChooseArgs, state: &ChoiceState, terminal_width: u16) -> Self {
        let border = args.block.border.inset();
        let prompt_width = UnicodeWidthStr::width(args.prompt.as_str()) as u16;
        let longest_item = state
            .items()
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
        let preferred_instruction = preferred_instruction(args, state);
        let instruction_width = if args.no_instructions {
            0
        } else {
            UnicodeWidthStr::width(preferred_instruction) as u16
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

        let instructions = instruction_text(args, state, width).is_some();
        let natural_list_height = (state.items().len() as u16).clamp(1, 10);
        let search_height = u16::from(state.filter_enabled());
        let natural_panel_height = natural_list_height
            .saturating_add(search_height)
            .saturating_add(border);
        let requested_total = args
            .viewport
            .height
            .unwrap_or_else(|| natural_panel_height.saturating_add(u16::from(instructions)));
        let minimum_height = border
            .saturating_add(1)
            .saturating_add(search_height)
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
    state: &ChoiceState,
    geometry: Geometry,
    theme: ThemePalette,
    list_state: &mut ListState,
) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
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
    let status = if state.multi() {
        format!(
            "{}/{} · {} selected ",
            state.visible_indices().len(),
            state.items().len(),
            state.selected_count()
        )
    } else if state.filter_enabled() {
        format!("{}/{} ", state.visible_indices().len(), state.items().len())
    } else {
        String::new()
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
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        )
        .borders(borders)
        .border_style(Style::default().fg(theme.border))
        .padding(Padding::horizontal(args.block.padding))
        .style(Style::default().fg(theme.foreground).bg(theme.background));
    if !status.is_empty() {
        block = block.title(Line::from(status).right_aligned());
    }
    if let Some(border_type) = args.block.border.ratatui() {
        block = block.border_type(border_type);
    }

    let body = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);
    let body_chunks = if state.filter_enabled() && body.height > 1 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(body)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(0), Constraint::Min(1)])
            .split(body)
    };

    if state.filter_enabled() && body_chunks[0].height > 0 {
        let query = if state.query().is_empty() {
            Span::styled("type to filter…", Style::default().fg(theme.muted))
        } else {
            Span::styled(state.query(), Style::default().fg(theme.foreground))
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "⌕ ",
                    Style::default()
                        .fg(theme.title)
                        .add_modifier(Modifier::BOLD),
                ),
                query,
                Span::styled("▏", Style::default().fg(theme.selected_background)),
            ]))
            .style(Style::default().bg(theme.background)),
            body_chunks[0],
        );
    }

    let rows = state.visible_indices().iter().map(|original_index| {
        let selected = state.is_selected(*original_index);
        let marker = if state.multi() {
            if selected {
                args.selected_prefix.as_str()
            } else {
                args.unselected_prefix.as_str()
            }
        } else {
            ""
        };
        let marker_style = if selected {
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        let item = ListItem::new(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::raw(display_record(&state.items()[*original_index])),
        ]));
        if selected {
            item.style(
                Style::default()
                    .fg(theme.title)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            item
        }
    });
    let list = List::new(rows)
        .highlight_symbol(args.prompt.as_str())
        .highlight_spacing(args.style.highlight_spacing.into())
        .highlight_style(
            Style::default()
                .fg(theme.selected_foreground)
                .bg(theme.selected_background)
                .add_modifier(Modifier::BOLD),
        );
    if state.visible_indices().is_empty() {
        frame.render_widget(
            Paragraph::new("No matches")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            body_chunks[1],
        );
    } else {
        frame.render_stateful_widget(list, body_chunks[1], list_state);
    }

    if geometry.instructions && chunks[1].height > 0 {
        let instruction = instruction_text(args, state, content.width)
            .expect("geometry reserves a row only when instruction text exists");
        frame.render_widget(
            Paragraph::new(instruction.as_ref())
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            chunks[1],
        );
    }
}

/// Merge a reusable preset with any explicit semantic color overrides.
fn resolve_theme(args: &ChooseArgs) -> ThemePalette {
    let mut palette = args.theme.palette();
    if let Some(color) = args.style.foreground {
        palette.foreground = color.0;
    }
    if let Some(color) = args.style.background {
        palette.background = color.0;
    }
    if let Some(color) = args.style.selected_fg {
        palette.selected_foreground = color.0;
    }
    if let Some(color) = args.style.selected_bg {
        palette.selected_background = color.0;
    }
    if let Some(color) = args.block.border_color {
        palette.border = color.0;
    }
    if let Some(color) = args.block.title_color {
        palette.title = color.0;
    }
    if let Some(color) = args.style.instruction_fg {
        palette.muted = color.0;
    }
    palette
}

/// Select instruction copy that fits the resolved component width.
fn instruction_text<'a>(
    args: &'a ChooseArgs,
    state: &ChoiceState,
    width: u16,
) -> Option<Cow<'a, str>> {
    if args.no_instructions || matches!(args.instruction_mode, InstructionMode::Hidden) {
        return None;
    }
    if let Some(custom) = args.instruction.as_deref() {
        return (!custom.is_empty()).then_some(Cow::Borrowed(custom));
    }

    let (full, compact, minimal) = instruction_variants(state);
    let text = match args.instruction_mode {
        InstructionMode::Full => full,
        InstructionMode::Compact => compact,
        InstructionMode::Minimal => minimal,
        InstructionMode::Hidden => return None,
        InstructionMode::Auto => {
            if width >= UnicodeWidthStr::width(full) as u16 {
                full
            } else if width >= UnicodeWidthStr::width(compact) as u16 {
                compact
            } else {
                minimal
            }
        }
    };
    Some(Cow::Borrowed(text))
}

fn preferred_instruction<'a>(args: &'a ChooseArgs, state: &ChoiceState) -> &'a str {
    if let Some(custom) = args.instruction.as_deref() {
        return custom;
    }
    let (full, compact, minimal) = instruction_variants(state);
    match args.instruction_mode {
        InstructionMode::Auto | InstructionMode::Full => full,
        InstructionMode::Compact => compact,
        InstructionMode::Minimal => minimal,
        InstructionMode::Hidden => "",
    }
}

fn instruction_variants(state: &ChoiceState) -> (&'static str, &'static str, &'static str) {
    if state.multi() && state.filter_enabled() {
        (
            "type: filter  •  ↑/↓: move  •  Tab: mark  •  Enter: accept",
            "type filter  •  Tab mark  •  ↵ accept",
            "Tab mark  •  ↵ accept",
        )
    } else if state.multi() {
        (
            MULTI_FULL_INSTRUCTION,
            MULTI_COMPACT_INSTRUCTION,
            MULTI_MINIMAL_INSTRUCTION,
        )
    } else if state.filter_enabled() {
        (
            FILTER_FULL_INSTRUCTION,
            FILTER_COMPACT_INSTRUCTION,
            FILTER_MINIMAL_INSTRUCTION,
        )
    } else {
        (FULL_INSTRUCTION, COMPACT_INSTRUCTION, MINIMAL_INSTRUCTION)
    }
}

/// Keep control characters in output records while rendering inert text in the TUI.
fn display_record(value: &str) -> Cow<'_, str> {
    if !value.chars().any(char::is_control) {
        return Cow::Borrowed(value);
    }
    Cow::Owned(
        value
            .chars()
            .map(|ch| if ch.is_control() { '�' } else { ch })
            .collect(),
    )
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
