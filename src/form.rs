//! Complete confirmation and single-line input interactions.
//!
//! Both components share the same terminal lifecycle, semantic themes,
//! viewports, borders, stdout isolation, cancellation contract, and PTY tests.

use std::io::{self, IsTerminal, Read};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::size,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    registry::{
        resolve_theme, AxisPosition, BlockArgs, ConfirmArgs, ConfirmDefault, InputArgs,
        InstructionMode, TextAlignment, ThemePalette, ViewportArgs,
    },
    terminal_session::TerminalSession,
};

const CONFIRM_FULL: &str = "←/→: choose  •  Enter: accept  •  y/n: answer  •  Esc: cancel";
const CONFIRM_COMPACT: &str = "←→ choose  •  ↵ accept  •  y/n";
const CONFIRM_MINIMAL: &str = "←→  •  ↵";
const INPUT_FULL: &str = "type: edit  •  ←/→: move  •  Enter: accept  •  Esc: cancel";
const INPUT_COMPACT: &str = "type  •  ←→ move  •  ↵ accept";
const INPUT_MINIMAL: &str = "type  •  ↵";

/// Accepted or cancelled interaction without conflating cancellation with false.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormResult<T> {
    Accepted(T),
    Cancelled,
}

/// Interactive text state kept independent from Ratatui rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputState {
    value: Vec<char>,
    cursor: usize,
    error: Option<String>,
}

impl InputState {
    pub fn new(value: String, args: &InputArgs) -> Result<Self, String> {
        if args.max_length.is_some_and(|maximum| args.min_length > maximum) {
            return Err("--min-length cannot exceed --max-length".into());
        }
        if args
            .char_limit
            .is_some_and(|limit| args.min_length > limit)
        {
            return Err("--min-length cannot exceed --char-limit".into());
        }
        let value: Vec<char> = value.chars().collect();
        if args.char_limit.is_some_and(|limit| value.len() > limit) {
            return Err(format!("initial value exceeds --char-limit ({})", value.len()));
        }
        let cursor = value.len();
        Ok(Self {
            value,
            cursor,
            error: None,
        })
    }

    pub fn value(&self) -> String {
        self.value.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn insert(&mut self, character: char, limit: Option<usize>) {
        if !character.is_control() && limit.is_none_or(|limit| self.value.len() < limit) {
            self.value.insert(self.cursor, character);
            self.cursor += 1;
            self.error = None;
        }
    }

    pub fn paste(&mut self, text: &str, limit: Option<usize>) {
        for character in text.chars().filter(|character| !character.is_control()) {
            self.insert(character, limit);
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.value.remove(self.cursor);
            self.error = None;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
            self.error = None;
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.len();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.error = None;
    }

    pub fn delete_word(&mut self) {
        while self.cursor > 0 && self.value[self.cursor - 1].is_whitespace() {
            self.backspace();
        }
        while self.cursor > 0 && !self.value[self.cursor - 1].is_whitespace() {
            self.backspace();
        }
    }

    pub fn accept(&mut self, args: &InputArgs) -> Option<String> {
        let raw = self.value();
        let value = if args.no_trim {
            raw
        } else {
            raw.trim().to_string()
        };
        let length = value.chars().count();
        let minimum = args.min_length.max(usize::from(args.required));
        self.error = if length < minimum {
            Some(if args.required && value.is_empty() {
                "A value is required".into()
            } else {
                format!("Enter at least {minimum} characters")
            })
        } else if args.max_length.is_some_and(|maximum| length > maximum) {
            Some(format!(
                "Enter at most {} characters",
                args.max_length.expect("checked maximum")
            ))
        } else {
            None
        };
        self.error.is_none().then_some(value)
    }
}

/// Read an optional pipeline seed without making ordinary interactive input block.
pub fn initial_input(args: &InputArgs) -> io::Result<String> {
    if let Some(value) = &args.value {
        return Ok(value.clone());
    }
    if args.read_stdin {
        if io::stdin().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--read-stdin requires piped stdin",
            ));
        }
        let mut value = String::new();
        io::stdin().read_to_string(&mut value)?;
        return Ok(value.trim_end_matches(['\r', '\n']).to_string());
    }
    Ok(String::new())
}

/// Resolve a confirmation directly or through its keyboard interaction.
pub fn confirm(args: &ConfirmArgs) -> io::Result<FormResult<bool>> {
    if args.yes {
        return Ok(FormResult::Accepted(true));
    }
    if args.no {
        return Ok(FormResult::Accepted(false));
    }

    let mut selected = matches!(args.default, ConfirmDefault::Yes);
    let mut terminal_width = size().map(|(width, _)| width).unwrap_or(80);
    let mut geometry = confirm_geometry(args, terminal_width);
    let theme = resolve_theme(args.theme, &args.style, &args.block);
    let mut session = TerminalSession::new(
        args.viewport.fullscreen,
        geometry.height,
        "interactive confirm requires stderr connected to a terminal; use --yes or --no for noninteractive confirmation",
    )?;

    loop {
        session.terminal.draw(|frame| {
            render_confirm(frame, args, selected, geometry, theme);
        })?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                        selected = !selected;
                    }
                    KeyCode::Enter => return Ok(FormResult::Accepted(selected)),
                    KeyCode::Char('y') | KeyCode::Char('Y') if !control => {
                        return Ok(FormResult::Accepted(true));
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') if !control => {
                        return Ok(FormResult::Accepted(false));
                    }
                    KeyCode::Esc => return Ok(FormResult::Cancelled),
                    KeyCode::Char('c') if control => return Ok(FormResult::Cancelled),
                    _ => {}
                }
            }
            Event::Resize(width, _) => {
                terminal_width = width;
                geometry = confirm_geometry(args, terminal_width);
            }
            _ => {}
        }
    }
}

/// Validate directly with --accept or run the complete text editor.
pub fn input(args: &InputArgs, initial: String) -> io::Result<FormResult<String>> {
    let mut state = InputState::new(initial, args)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    if args.accept {
        return state.accept(args).map(FormResult::Accepted).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                state.error().unwrap_or("invalid input"),
            )
        });
    }

    let mut terminal_width = size().map(|(width, _)| width).unwrap_or(80);
    let mut geometry = input_geometry(args, terminal_width);
    let theme = resolve_theme(args.theme, &args.style, &args.block);
    let mut session = TerminalSession::new(
        args.viewport.fullscreen,
        geometry.height,
        "interactive input requires stderr connected to a terminal; use --accept for noninteractive validation",
    )?;

    loop {
        session.terminal.draw(|frame| {
            render_input(frame, args, &state, geometry, theme);
        })?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Enter => {
                        if let Some(value) = state.accept(args) {
                            return Ok(FormResult::Accepted(value));
                        }
                    }
                    KeyCode::Esc => return Ok(FormResult::Cancelled),
                    KeyCode::Char('c') if control => return Ok(FormResult::Cancelled),
                    KeyCode::Char('a') if control => state.home(),
                    KeyCode::Char('e') if control => state.end(),
                    KeyCode::Char('u') if control => state.clear(),
                    KeyCode::Char('w') if control => state.delete_word(),
                    KeyCode::Left => state.left(),
                    KeyCode::Right => state.right(),
                    KeyCode::Home => state.home(),
                    KeyCode::End => state.end(),
                    KeyCode::Backspace => state.backspace(),
                    KeyCode::Delete => state.delete(),
                    KeyCode::Char(character) if !control => {
                        state.insert(character, args.char_limit);
                    }
                    _ => {}
                }
            }
            Event::Paste(text) => state.paste(&text, args.char_limit),
            Event::Resize(width, _) => {
                terminal_width = width;
                geometry = input_geometry(args, terminal_width);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FormGeometry {
    width: u16,
    height: u16,
    instructions: bool,
}

fn confirm_geometry(args: &ConfirmArgs, terminal_width: u16) -> FormGeometry {
    let labels = UnicodeWidthStr::width(args.affirmative.as_str())
        + UnicodeWidthStr::width(args.negative.as_str())
        + 12;
    resolve_geometry(
        &args.title,
        &args.message,
        labels as u16,
        &args.viewport,
        &args.block,
        args.instruction.as_deref(),
        args.instruction_mode,
        args.no_instructions,
        (CONFIRM_FULL, CONFIRM_COMPACT, CONFIRM_MINIMAL),
        terminal_width,
        2,
    )
}

fn input_geometry(args: &InputArgs, terminal_width: u16) -> FormGeometry {
    let content = args
        .placeholder
        .as_deref()
        .map(UnicodeWidthStr::width)
        .unwrap_or(20)
        .saturating_add(UnicodeWidthStr::width(args.prompt.as_str())) as u16;
    resolve_geometry(
        &args.title,
        "",
        content,
        &args.viewport,
        &args.block,
        args.instruction.as_deref(),
        args.instruction_mode,
        args.no_instructions,
        (INPUT_FULL, INPUT_COMPACT, INPUT_MINIMAL),
        terminal_width,
        2,
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_geometry(
    title: &str,
    message: &str,
    content_width: u16,
    viewport: &ViewportArgs,
    block: &BlockArgs,
    custom_instruction: Option<&str>,
    instruction_mode: InstructionMode,
    no_instructions: bool,
    instructions: (&str, &str, &str),
    terminal_width: u16,
    body_height: u16,
) -> FormGeometry {
    let inset = block
        .padding
        .saturating_mul(2)
        .saturating_add(block.border.inset());
    let natural = (UnicodeWidthStr::width(title) as u16)
        .max(UnicodeWidthStr::width(message) as u16)
        .max(content_width)
        .saturating_add(inset)
        .clamp(24, 72);
    let width = viewport
        .width
        .unwrap_or(natural)
        .clamp(1, terminal_width.max(1));
    let instruction = instruction_text(
        custom_instruction,
        instruction_mode,
        no_instructions,
        width,
        instructions,
    );
    let minimum = body_height
        .saturating_add(block.border.inset())
        .saturating_add(u16::from(instruction.is_some()));
    FormGeometry {
        width,
        height: viewport.height.unwrap_or(minimum).max(minimum),
        instructions: instruction.is_some(),
    }
}

fn render_confirm(
    frame: &mut Frame<'_>,
    args: &ConfirmArgs,
    yes: bool,
    geometry: FormGeometry,
    theme: ThemePalette,
) {
    let content = prepare_surface(
        frame,
        geometry,
        &args.viewport,
        &args.block,
        &args.title,
        theme,
    );
    let chunks = surface_chunks(content, geometry.instructions);
    let yes_style = choice_style(yes, theme);
    let no_style = choice_style(!yes, theme);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(args.message.as_str()).alignment(Alignment::Center),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!(" {} ", args.affirmative), yes_style),
                Span::raw("    "),
                Span::styled(format!(" {} ", args.negative), no_style),
            ])
            .alignment(Alignment::Center),
        ])
        .style(Style::default().fg(theme.foreground).bg(theme.background)),
        chunks[0],
    );
    render_instruction(
        frame,
        chunks[1],
        args.instruction.as_deref(),
        args.instruction_mode,
        args.no_instructions,
        content.width,
        (CONFIRM_FULL, CONFIRM_COMPACT, CONFIRM_MINIMAL),
        theme,
    );
}

fn render_input(
    frame: &mut Frame<'_>,
    args: &InputArgs,
    state: &InputState,
    geometry: FormGeometry,
    theme: ThemePalette,
) {
    let content = prepare_surface(
        frame,
        geometry,
        &args.viewport,
        &args.block,
        &args.title,
        theme,
    );
    let chunks = surface_chunks(content, geometry.instructions);
    let raw = state.value();
    let shown = if args.password {
        args.password_character.to_string().repeat(raw.chars().count())
    } else {
        raw
    };
    let mut characters: Vec<char> = shown.chars().collect();
    let cursor = state.cursor().min(characters.len());
    let after = characters.split_off(cursor);
    let before: String = characters.into_iter().collect();
    let after: String = after.into_iter().collect();
    let empty = before.is_empty() && after.is_empty();
    let value_line = if empty {
        Line::from(vec![
            Span::styled(
                args.prompt.as_str(),
                Style::default()
                    .fg(theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                args.placeholder.as_deref().unwrap_or(""),
                Style::default().fg(theme.muted),
            ),
            Span::styled("▏", Style::default().fg(theme.selected_background)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                args.prompt.as_str(),
                Style::default()
                    .fg(theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(before),
            Span::styled("▏", Style::default().fg(theme.selected_background)),
            Span::raw(after),
        ])
    };
    let error = state.error().unwrap_or("");
    frame.render_widget(
        Paragraph::new(vec![
            value_line,
            Line::from(error).style(Style::default().fg(ratatui::style::Color::LightRed)),
        ])
        .style(Style::default().fg(theme.foreground).bg(theme.background)),
        chunks[0],
    );
    render_instruction(
        frame,
        chunks[1],
        args.instruction.as_deref(),
        args.instruction_mode,
        args.no_instructions,
        content.width,
        (INPUT_FULL, INPUT_COMPACT, INPUT_MINIMAL),
        theme,
    );
}

fn prepare_surface(
    frame: &mut Frame<'_>,
    geometry: FormGeometry,
    viewport: &ViewportArgs,
    block_args: &BlockArgs,
    title: &str,
    theme: ThemePalette,
) -> Rect {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    let content = positioned_rect(
        area,
        geometry.width.min(area.width),
        geometry.height.min(area.height),
        viewport.position,
        if viewport.fullscreen {
            viewport.vertical_position
        } else {
            AxisPosition::Start
        },
    );
    frame.render_widget(Clear, content);
    let borders = if block_args.border.ratatui().is_some() {
        Borders::ALL
    } else {
        Borders::NONE
    };
    let mut block = Block::default()
        .title(title)
        .title_alignment(match block_args.title_alignment {
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
        .padding(Padding::horizontal(block_args.padding))
        .style(Style::default().fg(theme.foreground).bg(theme.background));
    if let Some(border_type) = block_args.border.ratatui() {
        block = block.border_type(border_type);
    }
    let inner = block.inner(content);
    frame.render_widget(block, content);
    inner
}

fn surface_chunks(area: Rect, instructions: bool) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(u16::from(instructions)),
        ])
        .split(area)
}

#[allow(clippy::too_many_arguments)]
fn render_instruction(
    frame: &mut Frame<'_>,
    area: Rect,
    custom: Option<&str>,
    mode: InstructionMode,
    hidden: bool,
    width: u16,
    variants: (&str, &str, &str),
    theme: ThemePalette,
) {
    if area.height == 0 {
        return;
    }
    if let Some(text) = instruction_text(custom, mode, hidden, width, variants) {
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            area,
        );
    }
}

fn instruction_text<'a>(
    custom: Option<&'a str>,
    mode: InstructionMode,
    hidden: bool,
    width: u16,
    (full, compact, minimal): (&'a str, &'a str, &'a str),
) -> Option<&'a str> {
    if hidden || matches!(mode, InstructionMode::Hidden) {
        return None;
    }
    if let Some(custom) = custom {
        return (!custom.is_empty()).then_some(custom);
    }
    Some(match mode {
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
    })
}

fn choice_style(selected: bool, theme: ThemePalette) -> Style {
    if selected {
        Style::default()
            .fg(theme.selected_foreground)
            .bg(theme.selected_background)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted).bg(theme.background)
    }
}

fn positioned_rect(
    area: Rect,
    width: u16,
    height: u16,
    horizontal: AxisPosition,
    vertical: AxisPosition,
) -> Rect {
    let offset = |available: u16, requested: u16, position: AxisPosition| {
        let free = available.saturating_sub(requested);
        match position {
            AxisPosition::Start => 0,
            AxisPosition::Center => free / 2,
            AxisPosition::End => free,
        }
    };
    Rect::new(
        area.x + offset(area.width, width, horizontal),
        area.y + offset(area.height, height, vertical),
        width,
        height,
    )
    .inner(Margin::new(0, 0))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::InputState;
    use crate::{Cli, Commands};

    fn input_args(arguments: &[&str]) -> crate::registry::InputArgs {
        let cli = Cli::try_parse_from(arguments).expect("valid input arguments");
        let Commands::Input(args) = cli.command else {
            panic!("expected input command")
        };
        args
    }

    #[test]
    fn unicode_cursor_editing_and_validation_are_deterministic() {
        let args = input_args(&[
            "rodtui",
            "input",
            "--required",
            "--min-length",
            "3",
            "--char-limit",
            "5",
        ]);
        let mut state = InputState::new("aç".into(), &args).expect("state");
        assert_eq!(state.accept(&args), None);
        state.insert('ã', args.char_limit);
        state.left();
        state.insert('b', args.char_limit);
        assert_eq!(state.accept(&args), Some("açbã".into()));
    }

    #[test]
    fn delete_word_and_trim_match_shell_input_expectations() {
        let args = input_args(&["rodtui", "input", "--required"]);
        let mut state = InputState::new("hello world  ".into(), &args).expect("state");
        state.delete_word();
        assert_eq!(state.accept(&args), Some("hello".into()));
    }
}
