# 🦀 rodtui

**Build delightful terminal interfaces. Compose them from the command line.**

> A standalone Rust CLI, SDK, and declarative runtime powered by [Ratatui](https://ratatui.rs/). Designed for shell pipelines, interactive workflows, dashboards, and reusable terminal components.

[![CI](https://github.com/rodkisten/rodtui/actions/workflows/ci.yml/badge.svg)](https://github.com/rodkisten/rodtui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/rodkisten/rodtui?display_name=tag)](https://github.com/rodkisten/rodtui/releases)

```text
    __
   / _)   🦀  rodtui
  / /     Tiny claws. Big terminal ambitions.
 / /__
(_____)
```

> **Status: early alpha.** `rodtui choose` is implemented for local Rust builds. Other commands shown below describe the intended public API; no release binary has shipped yet.

## Why rodtui?

Ratatui is an excellent Rust rendering toolkit. Gum is an excellent model for composable shell interactions. rodtui aims to connect those worlds: a complete standalone CLI and embeddable SDK, with declarative screens, interactive widgets, data pipelines, transitions, and terminal-safe lifecycle management.

**rodtui owns its CLI and UI engine.** The separate `xi` project is a consumer of rodtui's public Rust API; it must not reimplement rodtui widgets or parsing.

## Planned experience

```sh
rodtui choose Deploy Build Test
rodtui confirm "Deploy to production?"
rodtui input --placeholder "Repository"
rodtui menu --title "Workspace" --search fuzzy --preview --file menu.yaml
rodtui table --file users.json --query '.users[]' --columns name,email,status
rodtui run dashboard.yaml
rodtui playground
```

The default viewport is compact and inline, so a component occupies only the
rows it needs and returns control to the surrounding shell. Fullscreen mode is
explicit:

```sh
rodtui choose Deploy Build Test \
  --title "Choose a task" \
  --width 48 \
  --border rounded \
  --border-color '#665c99' \
  --selected-bg '#7c5cff'

rodtui choose Deploy Build Test --fullscreen
```

Common flags use familiar fzf/Gum-style names. Advanced aliases mirror the
typed component model, such as `--viewport.width`, `--block.border`, and
`--style.selected-bg`. Colors accept names, `#RRGGBB`, and `ansi:N` values.
The versioned component registry is intended to generate CLI arguments,
schemas, SDK types, documentation, and a Ratatui coverage matrix from the same
definitions.

The CLI will support interactive TTY usage, stdin pipelines, structured stdout, script-friendly exit codes, and machine-readable schemas. Output to stdout is reserved for selected values/data; UI rendering uses a controlling TTY.

### Advanced data pipelines

```sh
gh pr list --json number,title,state |
  rodtui table --query '.[] | select(.state == "OPEN")' --columns number,title,state

ps aux | awk 'NR > 1 {print $1, $2, $3, $11}' |
  rodtui table --input whitespace
```

The query engine will evaluate a documented **jq-compatible subset** using `jaq`, with explicit compatibility tests. Native streaming operators will support filter, sort, group, aggregate, windows, and backpressure. Unix tools such as `awk`, `sed`, and `jq` remain first-class upstream processors.

### Composition

```yaml
type: layout
direction: horizontal
children:
  - id: sidebar
    widget: menu
    width: 25%
  - id: main
    widget: table
    flex: 1
  - id: inspector
    widget: paragraph
    width: 30%
```

Planned: nested layouts, responsive breakpoints, focus routing, menus with nested actions and fuzzy search, command palettes, multi-step forms, dialogs, overlays, charts, virtualized lists, file explorers, and live log viewers.

### Motion

Planned: timelines, easing, spinner/progress animation, slide/reveal/expand transitions, reduced-motion support, and redraw-on-demand. Terminal opacity is simulated with color interpolation, not actual alpha blending.

## Installation (planned, not yet published)

After the first verified release, the supported install command will be:

```sh
curl -fsSL https://raw.githubusercontent.com/rodkisten/rodtui/main/install.sh | sh
```

The installer will select the matching OS/architecture artifact, verify its SHA-256 digest against release checksums, and install into `~/.local/bin` by default. **Do not use the command until a release and verified installer are published.** For review-first installation, download and inspect `install.sh` before running it. No Rust toolchain will be needed for release binaries.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full technical specification: crate boundaries, backend isolation, CLI parsing, schema registry, widget coverage, runtime, animation, data queries, IPC, security, testing, and distribution.

## Implementation phases

| Phase | Scope | Exit criteria | Status |
| --- | --- | --- | --- |
| 0 · Foundation | Architecture, contributor rules, CI, release/install design | Reviewed docs and bootstrap checks | 🟡 In progress |
| 1 · Kernel | Cargo workspace, Ratatui, Crossterm backend, terminal guards, test backend | Runnable CLI and terminal restoration tests | 🟡 In progress |
| 2 · Gum baseline | choose, filter, input, confirm, write, table, style, spin, pager | Scriptable commands with contract tests | ⬜ Planned |
| 3 · Ratatui coverage | Native widgets, styles, constraints, viewports, state adapters | Versioned coverage matrix and snapshots | ⬜ Planned |
| 4 · Interactive composition | Menus, tree, palettes, forms, focus, keyboard/mouse | Complex navigation demos | ⬜ Planned |
| 5 · Data engine | JSON/JSONL/CSV, jq-like queries, streaming/windowing | Streaming correctness and bounded-memory tests | ⬜ Planned |
| 6 · Declarative runtime | YAML/JSON screens, bindings, actions, hot reload | Dashboard example and schema validation | ⬜ Planned |
| 7 · Effects & IPC | Animations, async jobs, Unix sockets, live updates | Smooth and cancellable live dashboards | ⬜ Planned |
| 8 · Playground | Component gallery, property inspector, event/state debug | Usable end-to-end demo | ⬜ Planned |
| 9 · Stable distribution | Cross-platform artifacts, checksum verification, changelog | curl installer and reproducible releases | ⬜ Planned |

**Coverage is a goal, not a current claim.** Literal 1:1 parity with every imperative Ratatui Rust API is not possible through command flags alone; the goal is documented functional parity through CLI, schemas, and SDK extension points.

## Proposed Rust stack

- `ratatui` for rendering, `crossterm` as the default isolated backend, `TestBackend` for deterministic tests.
- `clap` derive for stable structural commands; clap builder generated from a typed component registry for widget commands.
- `serde`, `serde_json`, `schemars`, and YAML support for declarative documents.
- `tokio` only in asynchronous runtime and IPC crates.
- `jaq` for structured queries, `nucleo` for fuzzy search, `tachyonfx` for effects (subject to compatibility verification).
- `tracing`, `insta`, and property tests for observability and correctness.

## Development workflow

Every implementation block must include:

1. **What changed**: files, behavior, and design choices.
2. **Why**: strategy, tradeoffs, and alternatives.
3. **How to test**: exact copy-pasteable commands and expected outcomes.
4. **Evidence**: tests/builds actually executed, never invented.
5. **Progress**: update the roadmap and remaining work.

### Mandatory Rust learning comments

**All newly written or modified Rust code must be educationally commented in English.** Explain non-obvious control flow, types, ownership, borrowing, lifetimes, traits, async behavior, terminal safety, and error handling. Prefer useful explanations over repeating the syntax. Add Rustdoc (`///`) to public APIs and module-level (`//!`) guides. Keep comments correct as code changes.

### Planned local validation

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

These commands become executable after the Cargo workspace is introduced in Phase 1.

## Compatibility & accessibility

Linux/macOS first, with Windows as a release target. Test SSH, tmux, narrow terminals, mobile terminal clients, keyboard/mouse navigation, no-color mode, Unicode width, resize handling, and reduced motion.

## Security principles

Never silently execute arbitrary shell commands from untrusted YAML/JSON. Actions require explicit opt-in, arguments are passed without implicit shell interpolation, IPC sockets are permission-restricted, and installers verify artifacts. Protect stdin/stdout contracts and restore terminal state on exit or recoverable failure.

## Project links

- [Architecture](ARCHITECTURE.md)
- [Progress](docs/PROGRESS.md)
- [GitHub repository](https://github.com/rodkisten/rodtui)
- [Releases](https://github.com/rodkisten/rodtui/releases)

---

**🦀 Small claws. Serious terminal power.**
