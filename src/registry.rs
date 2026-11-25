//! Typed component configuration shared by the CLI and renderer.
//!
//! This module is the first slice of rodtui's component registry. Clap reads
//! these types today; schema and documentation generators can consume the same
//! values later, which keeps the CLI, declarative files, and Rust SDK aligned.

use std::{fmt, str::FromStr};

use clap::{Args, ValueEnum};
use ratatui::{
    style::Color,
    widgets::{BorderType, HighlightSpacing},
};

/// Arguments accepted by the `choose` component.
#[derive(Debug, Clone, Args)]
pub struct ChooseArgs {
    /// Candidate values; quote an item if it contains spaces.
    #[arg(required = true)]
    pub items: Vec<String>,

    /// Heading shown above the choices.
    #[arg(long, short, default_value = "Choose")]
    pub title: String,

    /// Selection marker rendered before the active item.
    #[arg(long, default_value = "❯ ")]
    pub prompt: String,

    /// Instruction line rendered below the list.
    #[arg(
        long,
        default_value = "↑/↓ or j/k: navigate  •  Enter: select  •  Esc/q: cancel"
    )]
    pub instruction: String,

    /// Hide the instruction line.
    #[arg(long)]
    pub no_instructions: bool,

    /// Zero-based item selected when the component opens.
    #[arg(long, default_value_t = 0)]
    pub initial: usize,

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
    #[arg(long, default_value = "#665c99", visible_alias = "block.border-color")]
    pub border_color: CliColor,

    /// Title color: name, #RRGGBB, or ansi:N.
    #[arg(long, default_value = "#c4b5fd", visible_alias = "block.title-color")]
    pub title_color: CliColor,

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
    #[arg(long, default_value = "#d6d3e0", visible_alias = "style.foreground")]
    pub foreground: CliColor,

    /// Normal background color.
    #[arg(long, default_value = "#1f1e2c", visible_alias = "style.background")]
    pub background: CliColor,

    /// Selected item foreground color.
    #[arg(long, default_value = "white", visible_alias = "style.selected-fg")]
    pub selected_fg: CliColor,

    /// Selected item background color.
    #[arg(long, default_value = "#7c5cff", visible_alias = "style.selected-bg")]
    pub selected_bg: CliColor,

    /// Instruction foreground color.
    #[arg(
        long,
        default_value = "#8f8aa8",
        visible_alias = "style.instruction-fg"
    )]
    pub instruction_fg: CliColor,

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
