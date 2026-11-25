//! Typed component configuration shared by the CLI and renderer.
//!
//! This module is the first slice of rodtui's component registry. Clap reads
//! these types today; schema and documentation generators can consume the same
//! values later, which keeps the CLI, declarative files, and Rust SDK aligned.

use std::{fmt, path::PathBuf, str::FromStr};

use clap::{Args, ValueEnum};
use ratatui::{
    style::Color,
    widgets::{BorderType, HighlightSpacing},
};

/// Arguments accepted by the `choose` component.
#[derive(Debug, Clone, Args)]
pub struct ChooseArgs {
    /// Candidate values; piped stdin and input files are appended.
    #[arg(value_name = "ITEM", num_args = 0..)]
    pub items: Vec<String>,

    /// Read additional candidates from a text file; repeat for multiple files.
    #[arg(long, short = 'f', value_name = "PATH")]
    pub input_file: Vec<PathBuf>,

    /// Do not read candidates from piped stdin.
    #[arg(long)]
    pub no_stdin: bool,

    /// Split stdin and input files on NUL bytes instead of lines.
    #[arg(long)]
    pub read0: bool,

    /// Remove duplicate candidates while preserving the first occurrence.
    #[arg(long)]
    pub dedupe: bool,

    /// Heading shown above the choices.
    #[arg(long, short, default_value = "Choose")]
    pub title: String,

    /// Selection marker rendered before the active item.
    #[arg(long, default_value = "❯ ")]
    pub prompt: String,

    /// Marker rendered beside selected items in multi-select mode.
    #[arg(long, default_value = "● ", requires = "multi")]
    pub selected_prefix: String,

    /// Marker rendered beside unselected items in multi-select mode.
    #[arg(long, default_value = "○ ", requires = "multi")]
    pub unselected_prefix: String,

    /// Custom instruction line; omitted means responsive built-in text.
    #[arg(long)]
    pub instruction: Option<String>,

    /// Built-in instruction density.
    #[arg(long, default_value = "auto", visible_alias = "instruction.mode")]
    pub instruction_mode: InstructionMode,

    /// Hide the instruction line.
    #[arg(long)]
    pub no_instructions: bool,

    /// Zero-based item selected when the component opens.
    #[arg(long, default_value_t = 0)]
    pub initial: usize,

    /// Enable live fuzzy filtering while typing.
    #[arg(long)]
    pub filter: bool,

    /// Initial fuzzy-filter query; implies --filter.
    #[arg(long, short = 'q')]
    pub query: Option<String>,

    /// Fuzzy matching case behavior.
    #[arg(long, default_value = "smart", visible_alias = "search.case")]
    pub case: CaseMode,

    /// Preserve input order instead of sorting matches by fuzzy score.
    #[arg(long)]
    pub no_sort: bool,

    /// Mark multiple choices with Tab; without filtering, Space also marks.
    #[arg(long, short = 'm')]
    pub multi: bool,

    /// Maximum number of selected items in multi-select mode.
    #[arg(long, requires = "multi")]
    pub limit: Option<usize>,

    /// Preselect an original zero-based item index; repeat as needed.
    #[arg(long, value_name = "INDEX", requires = "multi")]
    pub selected: Vec<usize>,

    /// Accept automatically when filtering leaves exactly one match.
    #[arg(long)]
    pub select_1: bool,

    /// Machine-readable output contract.
    #[arg(long, default_value = "text")]
    pub output: OutputFormat,

    /// Terminate text/index output records with NUL instead of newline.
    #[arg(long)]
    pub print0: bool,

    /// Reusable color preset; explicit color flags override its tokens.
    #[arg(long, default_value = "rod", visible_alias = "style.theme")]
    pub theme: ThemePreset,

    /// Viewport dimensions and placement.
    #[command(flatten)]
    pub viewport: ViewportArgs,

    /// Block and border properties.
    #[command(flatten)]
    pub block: BlockArgs,

    /// List colors and selection behavior.
    #[command(flatten)]
    pub style: ListStyleArgs,
}

/// Terminal viewport behavior. Inline mode is deliberately the default.
#[derive(Debug, Clone, Args)]
pub struct ViewportArgs {
    /// Use the terminal's alternate screen instead of a compact inline viewport.
    #[arg(long)]
    pub fullscreen: bool,

    /// Component width in terminal columns; omitted means content-sized.
    #[arg(long, visible_alias = "viewport.width")]
    pub width: Option<u16>,

    /// Component height in terminal rows; omitted means content-sized.
    #[arg(long, visible_alias = "viewport.height")]
    pub height: Option<u16>,

    /// Horizontal placement within the available terminal area.
    #[arg(long, default_value = "center", visible_alias = "viewport.horizontal")]
    pub position: AxisPosition,

    /// Vertical placement used in fullscreen mode.
    #[arg(long, default_value = "center", visible_alias = "viewport.vertical")]
    pub vertical_position: AxisPosition,
}

/// Block properties map directly to Ratatui's `Block` configuration.
#[derive(Debug, Clone, Args)]
pub struct BlockArgs {
    /// Border glyph family.
    #[arg(long, default_value = "rounded", visible_alias = "block.border")]
    pub border: BorderKind,

    /// Border color: name, #RRGGBB, or ansi:N.
    #[arg(long, visible_alias = "block.border-color")]
    pub border_color: Option<CliColor>,

    /// Title color: name, #RRGGBB, or ansi:N.
    #[arg(long, visible_alias = "block.title-color")]
    pub title_color: Option<CliColor>,

    /// Horizontal title alignment.
    #[arg(long, default_value = "left", visible_alias = "block.title-alignment")]
    pub title_alignment: TextAlignment,

    /// Horizontal padding inside the border.
    #[arg(long, default_value_t = 1, visible_alias = "block.padding")]
    pub padding: u16,
}

/// List styling maps to Ratatui's `List`, `Style`, and `HighlightSpacing`.
#[derive(Debug, Clone, Args)]
pub struct ListStyleArgs {
    /// Normal foreground color.
    #[arg(long, visible_alias = "style.foreground")]
    pub foreground: Option<CliColor>,

    /// Normal background color.
    #[arg(long, visible_alias = "style.background")]
    pub background: Option<CliColor>,

    /// Selected item foreground color.
    #[arg(long, visible_alias = "style.selected-fg")]
    pub selected_fg: Option<CliColor>,

    /// Selected item background color.
    #[arg(long, visible_alias = "style.selected-bg")]
    pub selected_bg: Option<CliColor>,

    /// Instruction foreground color.
    #[arg(long, visible_alias = "style.instruction-fg")]
    pub instruction_fg: Option<CliColor>,

    /// When the prompt column reserves horizontal space.
    #[arg(
        long,
        default_value = "always",
        visible_alias = "list.highlight-spacing"
    )]
    pub highlight_spacing: HighlightSpacingValue,
}

/// Start, center, and end placement shared by both layout axes.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum AxisPosition {
    Start,
    #[default]
    Center,
    End,
}

/// Case matching behavior for fuzzy search.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum CaseMode {
    #[default]
    Smart,
    Sensitive,
    Insensitive,
}

/// Stable output formats intended for scripts and pipelines.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Index,
}

/// Instruction text density selected explicitly or from available width.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum InstructionMode {
    #[default]
    Auto,
    Full,
    Compact,
    Minimal,
    Hidden,
}

/// Reusable palettes shared by every rodtui component.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum ThemePreset {
    #[default]
    Rod,
    Graphite,
    Dracula,
    Nord,
    Gruvbox,
    Monochrome,
}

impl ThemePreset {
    /// Resolve semantic theme tokens into Ratatui colors.
    pub fn palette(self) -> ThemePalette {
        match self {
            Self::Rod => ThemePalette::rgb(
                (214, 211, 224),
                (31, 30, 44),
                (255, 255, 255),
                (124, 92, 255),
                (102, 92, 153),
                (196, 181, 253),
                (143, 138, 168),
            ),
            Self::Graphite => ThemePalette::rgb(
                (228, 228, 231),
                (24, 24, 27),
                (255, 255, 255),
                (113, 113, 122),
                (82, 82, 91),
                (212, 212, 216),
                (113, 113, 122),
            ),
            Self::Dracula => ThemePalette::rgb(
                (248, 248, 242),
                (40, 42, 54),
                (40, 42, 54),
                (189, 147, 249),
                (98, 114, 164),
                (255, 121, 198),
                (98, 114, 164),
            ),
            Self::Nord => ThemePalette::rgb(
                (216, 222, 233),
                (46, 52, 64),
                (46, 52, 64),
                (136, 192, 208),
                (76, 86, 106),
                (129, 161, 193),
                (94, 129, 172),
            ),
            Self::Gruvbox => ThemePalette::rgb(
                (235, 219, 178),
                (40, 40, 40),
                (40, 40, 40),
                (215, 153, 33),
                (102, 92, 84),
                (250, 189, 47),
                (146, 131, 116),
            ),
            Self::Monochrome => ThemePalette {
                foreground: Color::White,
                background: Color::Reset,
                selected_foreground: Color::Black,
                selected_background: Color::White,
                border: Color::DarkGray,
                title: Color::White,
                muted: Color::Gray,
            },
        }
    }
}

/// Semantic colors consumed by components after applying CLI overrides.
#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub foreground: Color,
    pub background: Color,
    pub selected_foreground: Color,
    pub selected_background: Color,
    pub border: Color,
    pub title: Color,
    pub muted: Color,
}

impl ThemePalette {
    /// Construct a palette from readable RGB tuples.
    fn rgb(
        foreground: (u8, u8, u8),
        background: (u8, u8, u8),
        selected_foreground: (u8, u8, u8),
        selected_background: (u8, u8, u8),
        border: (u8, u8, u8),
        title: (u8, u8, u8),
        muted: (u8, u8, u8),
    ) -> Self {
        let color = |(red, green, blue)| Color::Rgb(red, green, blue);
        Self {
            foreground: color(foreground),
            background: color(background),
            selected_foreground: color(selected_foreground),
            selected_background: color(selected_background),
            border: color(border),
            title: color(title),
            muted: color(muted),
        }
    }
}

/// CLI representation of Ratatui's border types plus a borderless option.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum BorderKind {
    None,
    Plain,
    #[default]
    Rounded,
    Double,
    Thick,
    QuadrantInside,
    QuadrantOutside,
}

impl BorderKind {
    /// Convert the stable CLI value into Ratatui's native border type.
    pub fn ratatui(self) -> Option<BorderType> {
        match self {
            Self::None => None,
            Self::Plain => Some(BorderType::Plain),
            Self::Rounded => Some(BorderType::Rounded),
            Self::Double => Some(BorderType::Double),
            Self::Thick => Some(BorderType::Thick),
            Self::QuadrantInside => Some(BorderType::QuadrantInside),
            Self::QuadrantOutside => Some(BorderType::QuadrantOutside),
        }
    }

    /// Return the rows and columns occupied by a complete border.
    pub fn inset(self) -> u16 {
        if matches!(self, Self::None) {
            0
        } else {
            2
        }
    }
}

/// CLI representation of Ratatui text alignment.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// CLI representation of Ratatui's highlight spacing policy.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum HighlightSpacingValue {
    #[default]
    Always,
    WhenSelected,
    Never,
}

impl From<HighlightSpacingValue> for HighlightSpacing {
    fn from(value: HighlightSpacingValue) -> Self {
        match value {
            HighlightSpacingValue::Always => Self::Always,
            HighlightSpacingValue::WhenSelected => Self::WhenSelected,
            HighlightSpacingValue::Never => Self::Never,
        }
    }
}

/// Color accepted by CLI flags while preserving Ratatui's complete color model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliColor(pub Color);

impl FromStr for CliColor {
    type Err = String;

    /// Parse named colors, six-digit RGB colors, and indexed ANSI colors.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.to_ascii_lowercase();
        let named = match normalized.as_str() {
            "reset" | "default" => Some(Color::Reset),
            "black" => Some(Color::Black),
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "yellow" => Some(Color::Yellow),
            "blue" => Some(Color::Blue),
            "magenta" | "purple" => Some(Color::Magenta),
            "cyan" => Some(Color::Cyan),
            "gray" | "grey" => Some(Color::Gray),
            "dark-gray" | "dark-grey" => Some(Color::DarkGray),
            "light-red" => Some(Color::LightRed),
            "light-green" => Some(Color::LightGreen),
            "light-yellow" => Some(Color::LightYellow),
            "light-blue" => Some(Color::LightBlue),
            "light-magenta" | "light-purple" => Some(Color::LightMagenta),
            "light-cyan" => Some(Color::LightCyan),
            "white" => Some(Color::White),
            _ => None,
        };

        if let Some(color) = named {
            return Ok(Self(color));
        }

        if let Some(rgb) = normalized.strip_prefix('#') {
            if rgb.len() != 6 {
                return Err("RGB colors must use #RRGGBB".into());
            }
            let channel = |range: std::ops::Range<usize>| {
                u8::from_str_radix(&rgb[range], 16)
                    .map_err(|_| "RGB colors must use hexadecimal digits".to_string())
            };
            return Ok(Self(Color::Rgb(
                channel(0..2)?,
                channel(2..4)?,
                channel(4..6)?,
            )));
        }

        if let Some(index) = normalized.strip_prefix("ansi:") {
            let index = index
                .parse::<u8>()
                .map_err(|_| "ANSI colors must use ansi:0 through ansi:255".to_string())?;
            return Ok(Self(Color::Indexed(index)));
        }

        Err(format!(
            "unknown color {input:?}; use a named color, #RRGGBB, or ansi:N"
        ))
    }
}

impl fmt::Display for CliColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.0)
    }
}
