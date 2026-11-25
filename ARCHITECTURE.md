# 🦀 rodtui Architecture

**Status:** proposed specification · **Language:** English · **Owner:** rodtui · **Consumer:** xi

## 1. Product boundaries

rodtui is an independently installable CLI and Rust SDK built on Ratatui. It owns terminal rendering, reusable interactive components, layout, animations, data transformations, schemas, and runtime. xi consumes its SDK; xi must not fork widgets or duplicate terminal/event infrastructure.

Three supported surfaces: (1) direct commands (`rodtui choose`), (2) declarative files (`rodtui run screen.yaml`), and (3) Rust SDK (`rodtui::prelude`). All must normalize to a shared typed command/component model.

## 2. Dependency and crate boundaries

Proposed workspace:

```text
crates/
  rodtui-cli/         # executable and clap entrypoints
  rodtui-core/        # component IDs, state, styles, typed contracts
  rodtui-terminal/    # backend abstraction, Crossterm and terminal guards
  rodtui-widgets/     # native Ratatui adapters
  rodtui-components/  # interactive menus, forms, palette, tree, overlays
  rodtui-layout/      # constraints, responsive composition, split panes
  rodtui-data/        # JSON, JSONL, CSV, jq-like query engine, streams
  rodtui-effects/     # animation scheduler and effect adapters
  rodtui-runtime/     # state, actions, events, reconciliation, async tasks
  rodtui-schema/      # YAML/JSON input, JSON Schema and validation
  rodtui-ipc/         # authenticated local sessions and Unix sockets
  rodtui-testing/     # test backend, fixtures, snapshot harness
```

Begin with fewer crates if necessary; split only at proven API boundaries. The public umbrella crate `rodtui` should re-export stable APIs and offer feature-gated modules. Core must not depend on Tokio or Crossterm. The terminal adapter may depend on Crossterm; tests may use Ratatui TestBackend.

## 3. Terminal strategy

Use Ratatui as rendering abstraction and Crossterm as default terminal/event backend. The backend adapter is isolated. Terminal lifecycle uses RAII guards and explicit restoration on normal exits, recoverable errors, and panic hooks where safe. Render to controlling TTY, not stdout, when stdout carries results. No TTY: return a clear error or use an explicit noninteractive mode. Detect capabilities and honor NO_COLOR and reduced-motion configuration.

Interactive component commands default to a compact inline viewport sized from
their content. Fullscreen mode and the alternate screen are explicit. This
keeps short selections usable in mobile terminals and preserves surrounding
shell context while still allowing dashboards to own the complete screen.
Instruction copy and other secondary content use terminal-width breakpoints so
components retain their meaning without overflowing narrow mobile viewports.

Backends: Crossterm (default), TestBackend (tests), Termwiz (possible future feature). Support inline, fullscreen and fixed viewports. Avoid mixing incompatible Crossterm versions.

## 4. CLI parsing

Use Clap 4: derive for stable infrastructure commands (`run`, `serve`, `schema`, `inspect`, `completions`, `playground`), builder for widget subcommands generated from the registry. Namespace advanced flags (e.g. `--style.border`, `--search.mode`) while maintaining short common aliases. Parsing results in a typed AST, then merged configuration, validated schema, and runtime execution.

Precedence: defaults < user config < project config < environment < CLI flags. Secrets must not appear in diagnostics. Provide `--dry-run`, `--output json`, contextual help, typo suggestions, and generated shell completions.

## 5. Component registry and coverage

A versioned registry is the source of truth for widget IDs, arguments, property types, defaults, validation, help text, schema, and documentation. Maintain a machine-readable coverage matrix pinned to an exact Ratatui version. Classify each capability as supported, partial, or pending, with a test link.

CLI names have two layers: short familiar flags inspired by fzf and Gum for
common operations, and namespaced aliases matching the typed registry for
advanced properties. For example, `--width` aliases `--viewport.width`, while
`--selected-bg` aliases `--style.selected-bg`. Every declarative Ratatui
property must either map to the registry or appear as pending in the coverage
matrix; runtime callbacks and Rust generic types remain SDK concerns.

Themes expose semantic tokens such as foreground, background, selection,
border, title, and muted text. Components consume tokens rather than embedding
colors. A preset supplies defaults, and explicit component flags override
individual tokens without discarding the rest of the preset.

Native coverage targets: Paragraph/Text/Line/Span, Block, List, Table, BarChart, Chart/Dataset, Sparkline, Gauge/LineGauge, Canvas, Calendar, Scrollbar, Clear, Layout/Constraint/Flex/Rect, Style/Modifier/Color, viewports, and stateful rendering. Distinguish native Ratatui widgets from rodtui extensions.

Literal Rust API parity cannot be expressed entirely through flags; functional parity uses schemas and SDK extension points.

## 6. Interaction model

Input events normalize to key, mouse, paste, resize, tick, and lifecycle messages. Components receive typed events, produce state changes and optional actions, and render from state. Stable IDs preserve focus and selection across rerenders.

Menus support fuzzy search, descriptions, disabled entries, groups, nested submenus, breadcrumbs, back navigation, preview panes, keyboard shortcuts, and optional multiselect. Complex navigation must always expose a reliable Back/Cancel path.

The implemented `choose` state keeps original input indices separate from the
visible fuzzy-sorted order. Filtering uses `nucleo-matcher`; selected values
remain stable across query changes and are emitted in original input order.
The pure state owns navigation, query editing, limits, and acceptance while the
terminal adapter only translates events and renders the current state.

Forms support validation, errors, conditional fields, keyboard accessibility, and output typed by field ID. Avoid executing user actions during rendering.

## 7. Layout and responsive design

Use Ratatui constraints and layout engine for base geometry. Add a higher-level declarative solver for nested layouts, breakpoints based on terminal rows/columns, collapsible panels, overlays, scroll containers, and resizable split panes. Never infer pixel-accurate animation or transparency from a cell terminal.

## 8. Data engine

Accept text, CSV, TSV, whitespace-delimited, JSON, and JSONL. A query option uses jaq with a documented compatibility subset of jq, not a promise of full jq equivalence. Native operators cover select/project, filter, sort, group, aggregate, join (later), windows, and bounded streams.

Streaming requirements: bounded queues, backpressure, cancellation, stable row IDs, incremental updates, and memory limits. Expose transformation errors with source positions where available. Preserve Unix pipelines: awk, sed, jq and external tools can preprocess stdin. Never parse shell scripts implicitly.

## 9. Reactive runtime and IPC

Runtime manages component tree, state store, derived values, event routing, action queue, subscriptions, and render scheduling. Tokio is limited to async tasks, subprocess management, and IPC. Idle apps do not redraw continuously; active animations request timed frames.

IPC: local Unix socket initially, restricted permissions, versioned messages, per-session IDs, update batching, and cancellation. Authentication/authorization and remote transports require explicit design before exposure.

## 10. Effects and animation

Use TachyonFX as an adapter candidate, not an unremovable core dependency. Provide timelines, easing, stagger, progress/spinners, reveal, slide, expand, and simulated fades. Reduced-motion mode replaces transitions with instant state changes. Schedule frames only while active, with configurable FPS ceilings and a graceful slow-terminal fallback.

## 11. CLI I/O contract

- stdin: source data, not interactive keyboard input.
- stdout: selected values, JSON, CSV or other machine-readable results.
- stderr: diagnostics and errors.
- /dev/tty or equivalent controlling terminal: interactive display and input.
- Exit 0: success; nonzero: cancel/error with distinct documented codes.
- Broken pipe: clean termination; cancellation propagates to workers.
- Explicit noninteractive flags support CI and scripts.

The `choose` implementation establishes exit `0` for accepted output, `1` for
a no-match noninteractive selection, `2` for usage/runtime errors, and `130`
for user cancellation. Its text, original-index, and JSON encoders are tested
through the compiled binary with real stdin/stdout pipes.

Gum compatibility is tracked separately from Ratatui widget coverage, including argument aliases and exit behavior.

## 12. Security

Declarative actions are inert unless explicitly enabled. Prefer executable + argument arrays, never implicit `sh -c`. Validate external inputs, cap payload sizes and query resources, redact secrets, and sandbox/deny dangerous extensions by default. Verify downloaded release binaries with published SHA-256 checksums. Do not treat checksums alone as provenance or signing.

## 13. Testing and quality

- Unit tests: state reducers, parsers, query operators, layout resolution.
- Ratatui TestBackend snapshots: deterministic widget rendering.
- Property tests: Unicode, resize, invalid schema, random event sequences.
- PTY integration tests: terminal restoration, input, SIGINT, cancellation.
- CLI contract tests: stdout/stderr/exit codes and Gum-compatible flags.
- Release smoke tests: download, checksum, executable launch, help/version.
- Benchmarks: startup, frame time, search latency, JSONL throughput, memory.

Required checks: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release`. Add supply-chain checks when the workspace is bootstrapped.

## 14. Release and installer

GitHub Actions CI on pull requests and main; tag-driven release workflow builds platform binaries, uploads SHA-256 manifest, and publishes a GitHub Release. Installer chooses supported target, downloads binary and checksum manifest, validates the archive, and atomically installs to `~/.local/bin` or configured prefix. Never fall back to executing arbitrary downloaded source. Verify release availability before advertising a curl install as ready.

Targets to validate: Linux x86_64/aarch64 (glibc or musl explicitly), macOS x86_64/aarch64, Windows x86_64 (PowerShell install path separately). Release only targets actually tested.

## 15. Educational Rust rule (mandatory)

All Rust source files must contain meaningful **English** comments. Public APIs require Rustdoc (`///`), modules should explain their role (`//!`), and non-obvious code must explain ownership, borrowing, lifetimes, traits, event loops, concurrency, errors, and cleanup. Comments should teach *why*, not paraphrase every token. Keep examples runnable and comments updated. This applies to every future Rust project as a requested working convention.

## 16. Delivery protocol

For every implementation block, publish a concise report: **Changed**, **Strategy**, **Tests executed**, **How to test** (copy-paste commands), **Known limitations**, **Next**. Update `docs/PROGRESS.md` and the README phase table. Never claim tests passed without actually running them. Deliver complete vertical features in large coherent chunks: input, state, UI, output, documentation, and integration tests belong in the same reviewable block whenever they form one user workflow. Avoid micro-commits and partial feature fragments. Intermediate commits may use `[skip ci]`; final delivery commits should trigger CI. Always provide relevant links to commits, checks, and previews when available.

## 17. Open design questions

- Exact crate versions and minimum supported Rust version.
- Stable YAML schema version and migration policy.
- jaq compatibility surface and supported jq built-ins.
- Windows terminal backend and installer.
- Whether plugin extension uses native Rust, WASM, or IPC.
- Event ordering and transactional update semantics.
- Reproducibility, artifact signing, and provenance.

All unresolved questions remain explicitly open until tested and documented.
