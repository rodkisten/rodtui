//! Pure state and fuzzy matching for the `choose` component.
//!
//! Terminal rendering and keyboard decoding live elsewhere. Keeping this model
//! free of terminal handles makes filtering, navigation, and multiselect
//! deterministic and reusable by the future SDK and declarative runtime.

use std::collections::BTreeSet;

use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};

use crate::registry::{CaseMode, ChooseArgs};

/// A candidate retains its original input position through filtering and sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub original_index: usize,
    pub value: String,
}

/// Result returned by the interactive chooser and consumed by output encoders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOutcome {
    pub query: String,
    pub selected: Vec<Choice>,
}

/// Complete interactive state for filtering, navigation, and selection.
#[derive(Debug)]
pub struct ChoiceState {
    items: Vec<String>,
    visible: Vec<usize>,
    match_indices: Vec<Vec<u32>>,
    cursor: usize,
    selected: BTreeSet<usize>,
    query: String,
    filter_enabled: bool,
    sort_matches: bool,
    case: CaseMode,
    multi: bool,
    limit: Option<usize>,
}

impl ChoiceState {
    /// Build state from resolved input and validate preselected indices.
    pub fn new(items: Vec<String>, args: &ChooseArgs) -> Result<Self, String> {
        if items.is_empty() {
            return Err("choose requires at least one item from arguments, files, or stdin".into());
        }
        if matches!(args.limit, Some(0)) {
            return Err("--limit must be greater than zero".into());
        }
        if args.selected.iter().any(|index| *index >= items.len()) {
            return Err(format!(
                "--selected index is outside the input range 0..{}",
                items.len().saturating_sub(1)
            ));
        }
        let selected: BTreeSet<_> = args.selected.iter().copied().collect();
        if let Some(limit) = args.limit {
            if selected.len() > limit {
                return Err("preselected items exceed --limit".into());
            }
        }

        let filter_enabled = args.filter || args.query.is_some();
        let mut state = Self {
            visible: (0..items.len()).collect(),
            match_indices: Vec::new(),
            cursor: args.initial.min(items.len().saturating_sub(1)),
            selected,
            query: args.query.clone().unwrap_or_default(),
            filter_enabled,
            sort_matches: !args.no_sort,
            case: args.case,
            multi: args.multi,
            limit: args.limit,
            items,
        };
        state.rebuild_visible();
        Ok(state)
    }

    /// Return all original candidates.
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Return original indices in their current visible order.
    pub fn visible_indices(&self) -> &[usize] {
        &self.visible
    }

    /// Return the matched character indices for a visible row (for `hl` / `hl+` styling).
    pub fn match_indices(&self, visible_index: usize) -> &[u32] {
        self.match_indices
            .get(visible_index)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Return the current search query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether keyboard characters edit a fuzzy query.
    pub fn filter_enabled(&self) -> bool {
        self.filter_enabled
    }

    /// Whether the component accepts multiple selected items.
    pub fn multi(&self) -> bool {
        self.multi
    }

    /// Current cursor position within the visible result list.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Number of selected original items.
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    /// Check whether an original item index is selected.
    pub fn is_selected(&self, original_index: usize) -> bool {
        self.selected.contains(&original_index)
    }

    /// Return the original index under the cursor.
    pub fn current_original_index(&self) -> Option<usize> {
        self.visible.get(self.cursor).copied()
    }

    /// Move to the next visible result with wrap-around navigation.
    pub fn next(&mut self) {
        if !self.visible.is_empty() {
            self.cursor = (self.cursor + 1) % self.visible.len();
        }
    }

    /// Move to the previous visible result with wrap-around navigation.
    pub fn previous(&mut self) {
        if !self.visible.is_empty() {
            self.cursor = (self.cursor + self.visible.len() - 1) % self.visible.len();
        }
    }

    /// Toggle the current item while respecting the optional selection limit.
    pub fn toggle_current(&mut self) -> bool {
        let Some(index) = self.current_original_index() else {
            return false;
        };
        if self.selected.remove(&index) {
            return true;
        }
        if self.limit.is_some_and(|limit| self.selected.len() >= limit) {
            return false;
        }
        self.selected.insert(index);
        true
    }

    /// Append one character to the fuzzy query and rebuild visible results.
    pub fn push_query(&mut self, character: char) {
        if self.filter_enabled {
            self.query.push(character);
            self.rebuild_visible();
        }
    }

    /// Remove the final Unicode scalar from the fuzzy query.
    pub fn pop_query(&mut self) {
        if self.filter_enabled && self.query.pop().is_some() {
            self.rebuild_visible();
        }
    }

    /// Clear the current fuzzy query.
    pub fn clear_query(&mut self) {
        if self.filter_enabled && !self.query.is_empty() {
            self.query.clear();
            self.rebuild_visible();
        }
    }

    /// Append sanitized paste text to the query.
    pub fn append_query(&mut self, text: &str) {
        if self.filter_enabled {
            self.query.extend(text.chars().map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            }));
            self.rebuild_visible();
        }
    }

    /// Resolve selected values in stable original-input order.
    pub fn accept(&self) -> Option<ChoiceOutcome> {
        let indices: Vec<usize> = if self.multi && !self.selected.is_empty() {
            self.selected.iter().copied().collect()
        } else {
            vec![self.current_original_index()?]
        };
        let selected = indices
            .into_iter()
            .map(|original_index| Choice {
                original_index,
                value: self.items[original_index].clone(),
            })
            .collect();
        Some(ChoiceOutcome {
            query: self.query.clone(),
            selected,
        })
    }

    /// Rebuild matches while preserving the currently focused original item.
    fn rebuild_visible(&mut self) {
        let focused = self.current_original_index();
        if !self.filter_enabled || self.query.is_empty() {
            self.visible = (0..self.items.len()).collect();
            self.match_indices.clear();
        } else {
            let case = match self.case {
                CaseMode::Smart => CaseMatching::Smart,
                CaseMode::Sensitive => CaseMatching::Respect,
                CaseMode::Insensitive => CaseMatching::Ignore,
            };
            let pattern = Pattern::parse(&self.query, case, Normalization::Smart);
            let mut matcher = Matcher::new(Config::DEFAULT);
            let mut scored = Vec::new();
            let mut buffer = Vec::new();
            for (index, item) in self.items.iter().enumerate() {
                let mut indices = Vec::new();
                if let Some(score) = pattern.indices(
                    Utf32Str::new(item.as_str(), &mut buffer),
                    &mut matcher,
                    &mut indices,
                ) {
                    indices.sort_unstable();
                    indices.dedup();
                    scored.push((index, score, indices));
                }
            }
            if self.sort_matches {
                scored.sort_by(
                    |(left_index, left_score, _), (right_index, right_score, _)| {
                        right_score
                            .cmp(left_score)
                            .then_with(|| left_index.cmp(right_index))
                    },
                );
            }
            self.visible = Vec::with_capacity(scored.len());
            self.match_indices = Vec::with_capacity(scored.len());
            for (index, _, indices) in scored {
                self.visible.push(index);
                self.match_indices.push(indices);
            }
        }

        self.cursor = focused
            .and_then(|index| {
                self.visible
                    .iter()
                    .position(|candidate| *candidate == index)
            })
            .unwrap_or(0)
            .min(self.visible.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::ChoiceState;
    use crate::{Cli, Commands};

    fn args(arguments: &[&str]) -> crate::registry::ChooseArgs {
        let cli = Cli::try_parse_from(arguments).expect("valid test arguments");
        let Commands::Choose(args) = cli.command else {
            panic!("expected choose command")
        };
        args
    }

    #[test]
    fn fuzzy_filter_preserves_original_indices() {
        let args = args(&["rodtui", "choose", "--filter", "--query", "bld"]);
        let state = ChoiceState::new(vec!["Deploy".into(), "Build".into(), "Test".into()], &args)
            .expect("state");
        assert_eq!(state.visible_indices(), &[1]);
        assert_eq!(state.match_indices(0), &[0, 3, 4]);
        assert_eq!(
            state.accept().expect("selection").selected[0].original_index,
            1
        );
    }

    #[test]
    fn multiselect_enforces_limit_and_returns_input_order() {
        let args = args(&[
            "rodtui",
            "choose",
            "--multi",
            "--limit",
            "2",
            "--selected",
            "2",
            "--selected",
            "0",
        ]);
        let mut state =
            ChoiceState::new(vec!["Deploy".into(), "Build".into(), "Test".into()], &args)
                .expect("state");
        state.next();
        assert!(!state.toggle_current());
        let outcome = state.accept().expect("selection");
        let indices: Vec<_> = outcome
            .selected
            .into_iter()
            .map(|choice| choice.original_index)
            .collect();
        assert_eq!(indices, vec![0, 2]);
    }
}
