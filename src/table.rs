//! Interactive tabular data viewer and row selector built with Ratatui.
//!
//! Shares the same terminal lifecycle, semantic themes, fuzzy highlight tokens,
//! viewports, borders, and cancellation contract as `choose` and `form`.

use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    io::{self, IsTerminal, Read, Write},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::size,
};
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, Padding, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    registry::{
        resolve_theme, AxisPosition, CaseMode, InstructionMode, TableArgs, TableOutput,
        TextAlignment, ThemePalette,
    },
    terminal_session::TerminalSession,
};

const TABLE_FULL: &str = "↑/↓ or j/k: navigate  •  Enter: select  •  Esc/q: cancel";
const TABLE_COMPACT: &str = "↑↓ move  •  ↵ select  •  esc cancel";
const TABLE_MINIMAL: &str = "↑↓  •  ↵";

const FILTER_FULL: &str = "type: filter  •  ↑/↓: move  •  Enter: select  •  Esc: cancel";
const FILTER_COMPACT: &str = "type filter  •  ↑↓ move  •  ↵ select";
const FILTER_MINIMAL: &str = "type  •  ↑↓  •  ↵";

const MULTI_FULL: &str = "↑/↓: move  •  Space/Tab: mark  •  Enter: accept  •  Esc: cancel";
const MULTI_COMPACT: &str = "↑↓ move  •  space mark  •  ↵ accept";
const MULTI_MINIMAL: &str = "↑↓  •  space  •  ↵";

/// Distinct success, empty, and cancellation outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableResult {
    Accepted(TableOutcome),
    NoMatch,
    Cancelled,
}

/// Selected rows and metadata ready for output serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOutcome {
    pub headers: Vec<String>,
    pub selected_indices: Vec<usize>,
    pub selected_rows: Vec<Vec<String>>,
}

/// Pure tabular state kept independent from terminal handles.
#[derive(Debug)]
pub struct TableStateModel {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    visible: Vec<usize>,
    cell_matches: Vec<Vec<Vec<u32>>>,
    cursor: usize,
    selected: BTreeSet<usize>,
    query: String,
    filter_enabled: bool,
    case: CaseMode,
    multi: bool,
    limit: Option<usize>,
}

impl TableStateModel {
    pub fn new(raw_lines: Vec<String>, args: &TableArgs) -> Result<Self, String> {
        if matches!(args.limit, Some(0)) {
            return Err("--limit must be greater than zero".into());
        }
        let delimiter = parse_delimiter(&args.delimiter)?;
        let mut parsed: Vec<Vec<String>> = raw_lines
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| split_record(&line, &delimiter))
            .collect();

        let headers = if let Some(explicit) = &args.headers {
            split_record(explicit, &delimiter)
        } else if !parsed.is_empty() {
            parsed.remove(0)
        } else {
            Vec::new()
        };

        if parsed.is_empty() {
            return Err("table requires at least one data row".into());
        }

        let col_count = headers
            .len()
            .max(parsed.iter().map(Vec::len).max().unwrap_or(0));

        let mut normalized_headers = headers;
        normalized_headers.resize_with(col_count, String::new);
        for row in &mut parsed {
            row.resize_with(col_count, String::new);
        }

        let filter_enabled = args.filter || args.query.is_some();
        let mut state = Self {
            headers: normalized_headers,
            visible: (0..parsed.len()).collect(),
            cell_matches: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            query: args.query.clone().unwrap_or_default(),
            filter_enabled,
            case: args.case,
            multi: args.multi,
            limit: args.limit,
            rows: parsed,
        };
        state.rebuild_visible();
        Ok(state)
    }

    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    pub fn visible_indices(&self) -> &[usize] {
        &self.visible
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn filter_enabled(&self) -> bool {
        self.filter_enabled
    }

    pub fn multi(&self) -> bool {
        self.multi
    }

    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn is_selected(&self, original_index: usize) -> bool {
        self.selected.contains(&original_index)
    }

    pub fn cell_match_indices(&self, visible_row: usize, col: usize) -> &[u32] {
        self.cell_matches
            .get(visible_row)
            .and_then(|cols| cols.get(col))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn current_original_index(&self) -> Option<usize> {
        self.visible.get(self.cursor).copied()
    }

    pub fn next(&mut self) {
        if !self.visible.is_empty() {
            self.cursor = (self.cursor + 1) % self.visible.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.visible.is_empty() {
            self.cursor = (self.cursor + self.visible.len() - 1) % self.visible.len();
        }
    }

    pub fn toggle_current(&mut self) -> bool {
        let Some(idx) = self.current_original_index() else {
            return false;
        };
        if self.selected.remove(&idx) {
            return true;
        }
        if self.limit.is_some_and(|limit| self.selected.len() >= limit) {
            return false;
        }
        self.selected.insert(idx);
        true
    }

    pub fn push_query(&mut self, ch: char) {
        if self.filter_enabled {
            self.query.push(ch);
            self.rebuild_visible();
        }
    }

    pub fn pop_query(&mut self) {
        if self.filter_enabled && self.query.pop().is_some() {
            self.rebuild_visible();
        }
    }

    pub fn clear_query(&mut self) {
        if self.filter_enabled && !self.query.is_empty() {
            self.query.clear();
            self.rebuild_visible();
        }
    }

    pub fn append_query(&mut self, text: &str) {
        if self.filter_enabled {
            self.query
                .extend(text.chars().map(|c| if c.is_control() { ' ' } else { c }));
            self.rebuild_visible();
        }
    }

    pub fn accept(&self) -> Option<TableOutcome> {
        let indices: Vec<usize> = if self.multi && !self.selected.is_empty() {
            self.selected.iter().copied().collect()
        } else {
            vec![self.current_original_index()?]
        };
        let selected_rows = indices.iter().map(|&idx| self.rows[idx].clone()).collect();
        Some(TableOutcome {
            headers: self.headers.clone(),
            selected_indices: indices,
            selected_rows,
        })
    }

    fn rebuild_visible(&mut self) {
        let focused = self.current_original_index();
        if !self.filter_enabled || self.query.is_empty() {
            self.visible = (0..self.rows.len()).collect();
            self.cell_matches.clear();
        } else {
            let case = match self.case {
                CaseMode::Smart => CaseMatching::Smart,
                CaseMode::Sensitive => CaseMatching::Respect,
                CaseMode::Insensitive => CaseMatching::Ignore,
            };
            let pattern = Pattern::parse(&self.query, case, Normalization::Smart);
            let mut matcher = Matcher::new(Config::DEFAULT);
            let mut buffer = Vec::new();

            let mut matched = Vec::new();
            for (row_idx, row) in self.rows.iter().enumerate() {
                let mut best_score: Option<u32> = None;
                let mut per_cell = Vec::with_capacity(row.len());

                for cell in row {
                    let mut indices = Vec::new();
                    if let Some(score) = pattern.indices(
                        Utf32Str::new(cell.as_str(), &mut buffer),
                        &mut matcher,
                        &mut indices,
                    ) {
                        indices.sort_unstable();
                        indices.dedup();
                        best_score = Some(best_score.map_or(score, |s| s.max(score)));
                    }
                    per_cell.push(indices);
                }

                if let Some(score) = best_score {
                    matched.push((row_idx, score, per_cell));
                }
            }

            matched.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

            self.visible = Vec::with_capacity(matched.len());
            self.cell_matches = Vec::with_capacity(matched.len());
            for (row_idx, _, per_cell) in matched {
                self.visible.push(row_idx);
                self.cell_matches.push(per_cell);
            }
        }

        self.cursor = focused
            .and_then(|idx| self.visible.iter().position(|&c| c == idx))
            .unwrap_or(0)
            .min(self.visible.len().saturating_sub(1));
    }
}

/// Collect raw delimited lines from CLI arguments, files, and piped stdin.
pub fn collect_rows(args: &TableArgs) -> io::Result<Vec<String>> {
    let mut lines = args.rows.clone();
    for path in &args.input_file {
        let content = fs::read_to_string(path)?;
        lines.extend(content.lines().map(ToOwned::to_owned));
    }
    if !args.no_stdin && !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        lines.extend(buf.lines().map(ToOwned::to_owned));
    }
    Ok(lines)
}

/// Run the interactive table component.
pub fn table(args: &TableArgs, mut state: TableStateModel) -> io::Result<TableResult> {
    let mut terminal_width = size().map(|(w, _)| w).unwrap_or(80);
    let mut geometry = TableGeometry::resolve(args, &state, terminal_width);
    let theme = resolve_theme(args.theme, &args.style, &args.block);
    let mut session = TerminalSession::new(
        args.viewport.fullscreen,
        geometry.height,
        "interactive table requires stderr connected to a terminal",
    )?;
    let mut table_state = TableState::default().with_selected(selected_cursor(&state));

    loop {
        session.terminal.draw(|frame| {
            render_table(frame, args, &state, geometry, theme, &mut table_state);
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
                            .map(TableResult::Accepted)
                            .unwrap_or(TableResult::NoMatch));
                    }
                    KeyCode::Esc => return Ok(TableResult::Cancelled),
                    KeyCode::Char('c') if control => return Ok(TableResult::Cancelled),
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
                    KeyCode::Char(ch) if state.filter_enabled() && !control => {
                        state.push_query(ch);
                    }
                    KeyCode::Char('j') if !state.filter_enabled() => state.next(),
                    KeyCode::Char('k') if !state.filter_enabled() => state.previous(),
                    KeyCode::Char('q') if !state.filter_enabled() => {
                        return Ok(TableResult::Cancelled);
                    }
                    _ => {}
                }
            }
            Event::Paste(text) => state.append_query(&text),
            Event::Resize(width, _) => {
                terminal_width = width;
                geometry = TableGeometry::resolve(args, &state, terminal_width);
            }
            _ => {}
        }

        table_state.select(selected_cursor(&state));
    }
}

/// Serialize the accepted outcome according to `--output` and `--delimiter`.
pub fn write_outcome<W: Write>(
    writer: &mut W,
    args: &TableArgs,
    outcome: &TableOutcome,
) -> io::Result<()> {
    let delimiter = parse_delimiter(&args.delimiter).unwrap_or_else(|_| ",".into());
    match args.output {
        TableOutput::Text | TableOutput::Csv => {
            for row in &outcome.selected_rows {
                writeln!(writer, "{}", row.join(&delimiter))?;
            }
        }
        TableOutput::Index => {
            for idx in &outcome.selected_indices {
                writeln!(writer, "{idx}")?;
            }
        }
        TableOutput::Json => {
            let records: Vec<serde_json::Value> = outcome
                .selected_rows
                .iter()
                .map(|row| {
                    let mut map = serde_json::Map::new();
                    for (idx, val) in row.iter().enumerate() {
                        let key = outcome
                            .headers
                            .get(idx)
                            .filter(|h| !h.is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("col_{idx}"));
                        map.insert(key, serde_json::Value::String(val.clone()));
                    }
                    serde_json::Value::Object(map)
                })
                .collect();
            let payload = if args.multi {
                serde_json::Value::Array(records)
            } else {
                records.into_iter().next().unwrap_or(serde_json::Value::Null)
            };
            writeln!(writer, "{}", serde_json::to_string(&payload)?)?;
        }
    }
    Ok(())
}

fn selected_cursor(state: &TableStateModel) -> Option<usize> {
    (!state.visible_indices().is_empty()).then_some(state.cursor())
}

#[derive(Debug, Clone)]
struct TableGeometry {
    width: u16,
    height: u16,
    col_widths: Vec<u16>,
    instructions: bool,
}

impl TableGeometry {
    fn resolve(args: &TableArgs, state: &TableStateModel, terminal_width: u16) -> Self {
        let col_count = state.headers().len();
        let mut col_widths = vec![0u16; col_count];

        for (col, w) in col_widths.iter_mut().enumerate() {
            if let Some(&explicit) = args.widths.get(col) {
                *w = explicit;
                continue;
            }
            let header_w = if args.no_header {
                0
            } else {
                UnicodeWidthStr::width(state.headers()[col].as_str()) as u16
            };
            let max_cell_w = state
                .rows()
                .iter()
                .map(|r| UnicodeWidthStr::width(r[col].as_str()) as u16)
                .max()
                .unwrap_or(0);
            *w = header_w.max(max_cell_w).clamp(3, 36);
        }

        let border = args.block.border.inset();
        let prompt_w = UnicodeWidthStr::width(args.prompt.as_str()) as u16;
        let col_gaps = (col_count.saturating_sub(1) as u16).saturating_mul(2);
        let cols_total: u16 = col_widths.iter().sum::<u16>().saturating_add(col_gaps);

        let natural_width = cols_total
            .saturating_add(prompt_w)
            .saturating_add(args.block.padding.saturating_mul(2))
            .saturating_add(border)
            .clamp(28, terminal_width.max(28));

        let width = args
            .viewport
            .width
            .unwrap_or(natural_width)
            .clamp(1, terminal_width.max(1));

        let instructions = instruction_text(args, state, width).is_some();
        let header_rows = if args.no_header {
            0
        } else if args.no_separator {
            1
        } else {
            2
        };
        let search_rows = u16::from(state.filter_enabled());
        let data_rows = (state.rows().len() as u16).clamp(1, 12);

        let natural_height = data_rows
            .saturating_add(header_rows)
            .saturating_add(search_rows)
            .saturating_add(border)
            .saturating_add(u16::from(instructions));

        let minimum_height = border
            .saturating_add(header_rows)
            .saturating_add(search_rows)
            .saturating_add(1)
            .saturating_add(u16::from(instructions));

        Self {
            width,
            height: args.viewport.height.unwrap_or(natural_height).max(minimum_height),
            col_widths,
            instructions,
        }
    }
}

fn render_table(
    frame: &mut Frame<'_>,
    args: &TableArgs,
    state: &TableStateModel,
    geometry: TableGeometry,
    theme: ThemePalette,
    table_state: &mut TableState,
) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );

    let content = positioned_rect(
        area,
        geometry.width.min(area.width),
        geometry.height.min(area.height),
        args.viewport.position,
        if args.viewport.fullscreen {
            args.viewport.vertical_position
        } else {
            AxisPosition::Start
        },
    );
    frame.render_widget(Clear, content);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(u16::from(geometry.instructions && content.height > 1)),
        ])
        .split(content);

    let borders = if args.block.border.ratatui().is_some() {
        Borders::ALL
    } else {
        Borders::NONE
    };
    let status = if state.multi() {
        format!(
            "{}/{} · {} selected ",
            state.visible_indices().len(),
            state.rows().len(),
            state.selected_count()
        )
    } else if state.filter_enabled() {
        format!("{}/{} ", state.visible_indices().len(), state.rows().len())
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
                        .fg(theme.prompt)
                        .add_modifier(Modifier::BOLD),
                ),
                query,
                Span::styled("▏", Style::default().fg(theme.selected_background)),
            ]))
            .style(Style::default().bg(theme.background)),
            body_chunks[0],
        );
    }

    let active_cursor = selected_cursor(state);

    if state.visible_indices().is_empty() {
        frame.render_widget(
            Paragraph::new("No matching rows")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            body_chunks[1],
        );
    } else {
        let constraints: Vec<Constraint> = geometry
            .col_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        let rows = state
            .visible_indices()
            .iter()
            .enumerate()
            .map(|(visible_idx, &orig_idx)| {
                let is_selected = state.is_selected(orig_idx);
                let is_active = active_cursor == Some(visible_idx);
                let hl_color = if is_active {
                    theme.selected_match_foreground
                } else {
                    theme.match_foreground
                };

                let cells = state.rows()[orig_idx].iter().enumerate().map(|(col, val)| {
                    let mut spans = Vec::new();
                    if col == 0 && state.multi() {
                        spans.push(Span::styled(
                            if is_selected { "● " } else { "○ " },
                            Style::default().fg(if is_selected {
                                theme.title
                            } else {
                                theme.muted
                            }),
                        ));
                    }
                    spans.extend(highlight_indexed_spans(
                        val,
                        state.cell_match_indices(visible_idx, col),
                        Style::default()
                            .fg(hl_color)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ));
                    Cell::from(Line::from(spans))
                });

                let row = Row::new(cells);
                if is_selected {
                    row.style(
                        Style::default()
                            .fg(theme.title)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    row
                }
            });

        let mut table_widget = Table::new(rows, constraints)
            .column_spacing(2)
            .highlight_symbol(Span::styled(
                args.prompt.as_str(),
                Style::default()
                    .fg(theme.prompt)
                    .add_modifier(Modifier::BOLD),
            ))
            .highlight_spacing(args.style.highlight_spacing.into())
            .row_highlight_style(
                Style::default()
                    .fg(theme.selected_foreground)
                    .bg(theme.selected_background)
                    .add_modifier(Modifier::BOLD),
            );

        if !args.no_header {
            let header_cells = state.headers().iter().map(|h| {
                Cell::from(h.as_str()).style(
                    Style::default()
                        .fg(theme.title)
                        .add_modifier(Modifier::BOLD),
                )
            });
            let bottom_margin = u16::from(!args.no_separator);
            table_widget = table_widget.header(Row::new(header_cells).bottom_margin(bottom_margin));
        }

        frame.render_stateful_widget(table_widget, body_chunks[1], table_state);

        if args.style.scrollbar && state.visible_indices().len() > body_chunks[1].height as usize {
            let mut scroll_state = ScrollbarState::new(state.visible_indices().len())
                .position(active_cursor.unwrap_or(0));
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(theme.border));
            frame.render_stateful_widget(scrollbar, body_chunks[1], &mut scroll_state);
        }
    }

    if geometry.instructions && chunks[1].height > 0 {
        if let Some(text) = instruction_text(args, state, content.width) {
            frame.render_widget(
                Paragraph::new(text.as_ref())
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(theme.muted).bg(theme.background)),
                chunks[1],
            );
        }
    }
}

fn highlight_indexed_spans<'a>(
    text: &'a str,
    indices: &[u32],
    highlight_style: Style,
) -> Vec<Span<'a>> {
    if indices.is_empty() {
        return vec![Span::raw(text)];
    }
    let mut spans = Vec::new();
    let mut next_match = 0;
    let mut plain_start = 0;

    for (char_idx, (byte_idx, ch)) in text.char_indices().enumerate() {
        if indices.get(next_match).copied() == Some(char_idx as u32) {
            if plain_start < byte_idx {
                spans.push(Span::raw(&text[plain_start..byte_idx]));
            }
            let next_byte = byte_idx + ch.len_utf8();
            spans.push(Span::styled(&text[byte_idx..next_byte], highlight_style));
            plain_start = next_byte;
            next_match += 1;
        }
    }
    if plain_start < text.len() {
        spans.push(Span::raw(&text[plain_start..]));
    }
    spans
}

fn parse_delimiter(raw: &str) -> Result<String, String> {
    match raw {
        "\\t" => Ok("\t".into()),
        "" => Err("--delimiter cannot be empty".into()),
        other => Ok(other.to_owned()),
    }
}

fn split_record(line: &str, delimiter: &str) -> Vec<String> {
    line.split(delimiter)
        .map(|cell| cell.trim().to_owned())
        .collect()
}

fn instruction_text<'a>(
    args: &'a TableArgs,
    state: &TableStateModel,
    width: u16,
) -> Option<Cow<'a, str>> {
    if args.no_instructions || matches!(args.instruction_mode, InstructionMode::Hidden) {
        return None;
    }
    if let Some(custom) = args.instruction.as_deref() {
        return (!custom.is_empty()).then_some(Cow::Borrowed(custom));
    }
    let (full, compact, minimal) = if state.multi() && state.filter_enabled() {
        (
            "type: filter  •  ↑/↓: move  •  Tab: mark  •  Enter: accept",
            "type filter  •  Tab mark  •  ↵ accept",
            "Tab  •  ↵",
        )
    } else if state.multi() {
        (MULTI_FULL, MULTI_COMPACT, MULTI_MINIMAL)
    } else if state.filter_enabled() {
        (FILTER_FULL, FILTER_COMPACT, FILTER_MINIMAL)
    } else {
        (TABLE_FULL, TABLE_COMPACT, TABLE_MINIMAL)
    };

    let selected = match args.instruction_mode {
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
    Some(Cow::Borrowed(selected))
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
