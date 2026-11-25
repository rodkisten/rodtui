# 🦀 rodtui Progress Log

> This is a living implementation record. Do not mark an item complete until the code or workflow is present and validated.

## 2026-11-22 · Inception

**Changed:** Established README, architectural specification, implementation phases, educational Rust comment rule, test/report protocol, and release/installer design.

**Strategy:** Documentation-first bootstrap; preserve the distinction between standalone rodtui and consumer xi. Use Ratatui + an isolated Crossterm backend; Clap for CLI; a shared typed component registry for CLI/schema/SDK.

**Validation:** Documentation, installer source, CI and release workflows committed. No Rust workspace, binary, workflow run, or release has yet been verified. Curl installation is not ready until a release exists.

**How to test today:**
```sh
git clone https://github.com/rodkisten/rodtui.git
cd rodtui
test -f README.md && test -f ARCHITECTURE.md && echo "Documentation present"
```

**Next:** Bootstrap Cargo workspace, implement `rodtui --help` and `rodtui choose`, add terminal restoration tests, and activate CI. Then publish the first checksum-verified release and enable curl installation.

## Phase status

- [x] Product boundaries and documentation
- [ ] Cargo workspace and compiling CLI
- [ ] Native Ratatui adapter
- [ ] First interactive choose demo
- [x] CI workflow scaffold
- [ ] CI run and Rust checks verified
- [x] Installer and release workflow source
- [ ] Release artifacts and end-to-end curl installation verified
- [ ] Gum baseline parity
- [ ] Ratatui widget coverage
- [ ] Declarative runtime
- [ ] Animation and streaming
- [ ] Playground

## Reporting template

### YYYY-MM-DD · Block title

**Changed:** Files and user-visible behavior.

**Strategy:** Why this approach and what was deliberately deferred.

**Tests executed:** Exact commands and results.

**How to test:** Copy-pasteable commands with expected output.

**Known limitations:** Gaps, risks, and failed checks.

**Next:** Highest-value follow-up block.

## 2026-11-22 · Phase 1: first interactive Rust command

**Changed:** Added Cargo package manifest, commented Rust entry point, `choose` widget, selection state tests, and gitignore. Changed the renderer to stderr so stdout carries only the selected value. Updated the release build to tolerate the initial absence of a Cargo.lock.

**Strategy:** Start with one cohesive CLI-to-terminal vertical slice rather than prematurely splitting into many crates. Isolate selection state from terminal rendering and use an RAII guard for cleanup.

**Tests executed:** Not executed: the available container lacks Cargo and could not resolve github.com. GitHub file writes succeeded; compilation and runtime behavior are unverified.

**How to test:**
```sh
git clone https://github.com/rodkisten/rodtui.git
cd rodtui
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo run -- choose Deploy Build Test
cargo run -- choose Production Staging --title Environment
```

**Known limitations:** No checked-in Cargo.lock yet, no PTY integration tests, no fuzzy search, no public SDK, and no published release. The current UI requires interactive stdin and stderr. CI may identify compiler or formatting issues requiring a follow-up patch.

**Next:** Fix any compiler/CI failures, commit a Cargo.lock, add TestBackend snapshots and PTY cleanup tests, then extract reusable public SDK APIs.

## 2026-11-24 · Local baseline validation

**Changed:** Added the generated `Cargo.lock` so local builds resolve a reproducible dependency set. Applied `rustfmt` formatting to `src/main.rs` and `src/choose.rs` after the format check found two layout differences.

**Strategy:** Validate the checked-in command locally without relying on a workflow run. The interactive command was exercised through a pseudo-terminal, including arrow-key navigation, Enter selection, and the selected value on stdout.

**Tests executed:** `cargo fmt --all -- --check` passed; `cargo test` passed (2 tests); `cargo clippy --all-targets -- -D warnings` passed; `cargo build --release` passed; pseudo-terminal interaction selected `Build` successfully. `./target/release/rodtui choose Deploy Build Test` outside a TTY returned the expected `choose requires an interactive terminal` error.

**How to test:** Run `cargo run -- choose Deploy Build Test --title "Pick a task"` in a real terminal, use ↑/↓ or j/k, and press Enter. The selected item is written to stdout; the interface is rendered to stderr.

**Known limitations:** The GitHub CI and release workflow files are present, but no workflow run or release was validated as part of this local check. The command currently requires stdin and stderr to be terminals; `/dev/tty` support and terminal cleanup integration tests remain future work.

**Next:** Add deterministic Ratatui `TestBackend` rendering tests and a terminal cleanup integration test, then extract reusable public SDK APIs.

## 2026-11-24 · Typed choose configuration and compact viewport

**Changed:** Added the first typed component registry in `src/registry.rs` and moved `choose` arguments into it. The component now defaults to a content-sized inline viewport, with explicit `--fullscreen` support. Added configurable dimensions, placement, borders, padding, prompt, instruction visibility, initial selection, highlight spacing, and named/RGB/indexed colors. Advanced namespaced aliases mirror the typed registry.

**Strategy:** Use one Rust type model as the source for Clap today and for schemas, documentation, and SDK types later. Map declarative values directly to Ratatui types while keeping terminal lifecycle and event handling inside the renderer. Preserve list state between frames so long lists retain their scroll offset.

**Checks executed:** `cargo fmt --all -- --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo build`, and CLI help generation completed successfully. A pseudo-terminal smoke selected `Build` in both modes and confirmed that compact mode stays inline while `--fullscreen` enters the alternate screen. Automated unit tests were not run in this block.

**How to test:** Run `cargo run -- choose Deploy Build Test --title "Choose a task"` for the compact themed viewport. Add `--fullscreen` for the alternate screen. Run `cargo run -- choose --help` to inspect all supported flags and aliases.

**Known limitations:** This is the first registry slice rather than the complete Ratatui coverage matrix. The inline renderer still reads keyboard events from stdin instead of a separate controlling TTY. Declarative YAML/JSON generation and Gum compatibility aliases remain pending.

**Next:** Publish the branch for mobile terminal review, then add registry metadata/schema generation and extend the same model to `confirm` and `input`.

## 2026-11-24 · Shared themes and responsive instructions

**Changed:** Added reusable `rod`, `graphite`, `dracula`, `nord`, `gruvbox`, and `monochrome` theme presets to the typed registry. Explicit color flags now override semantic theme tokens individually. Added full, compact, minimal, hidden, and automatic instruction modes; automatic mode chooses copy from the resolved terminal width.

**Strategy:** Keep theme values semantic so future components share a coherent palette without duplicating color constants. Preserve complete CLI control by resolving the preset first and applying explicit color flags afterward. Keep instruction density separate from viewport sizing so mobile terminals receive readable shortcuts automatically.

**Checks executed:** `cargo fmt --all -- --check`, `cargo check`, strict Clippy, and CLI help generation completed successfully. A Clippy warning for a lazy boolean closure was corrected. A pseudo-terminal smoke confirmed Dracula RGB tokens, automatic minimal instructions at width 30, inline viewport behavior, and cancellation exit code 130.

**How to test:** Run `cargo run -- choose Deploy Build Test --theme dracula`, then compare `--theme nord`, `--theme gruvbox`, and `--theme monochrome`. Use `--width 30` to see compact instruction copy and `--instruction-mode full` to force the complete line.

**Known limitations:** Presets currently live in compiled registry data; user-defined theme files and configuration precedence are pending. Color capability detection and `NO_COLOR` support remain future work.

**Next:** Add registry metadata and schema generation, then reuse theme and viewport contracts for `confirm` and `input`.

## 2026-11-24 · Complete choose vertical slice

**Changed:** Completed `choose` as an end-to-end shell feature. Candidates now combine positional arguments, repeatable files, and piped stdin with line or NUL delimiters and optional stable deduplication. Added live fuzzy search through `nucleo-matcher`, smart/sensitive/insensitive case modes, score sorting or preserved input order, initial queries, multiselection, selection limits, preselected original indices, responsive search UI, bracketed paste, resize handling, and empty-result rendering. Added text, original-index, and structured JSON stdout contracts, NUL output, noninteractive unique-match selection, clean broken-pipe behavior, and distinct success/no-match/error/cancel exit codes.

**Strategy:** One pure `ChoiceState` owns original indices, visible fuzzy order, cursor, query, and stable selections. Terminal code translates Crossterm events into state transitions and renders Ratatui widgets; input collection and output encoding remain independent modules. This keeps CLI, future SDK, and declarative runtime behavior aligned while allowing deterministic tests without a terminal.

On Unix, the Ratatui backend sends inline cursor-position queries through
the controlling terminal. The PTY suite keeps both stdin and stdout piped,
so it catches terminal escape bytes leaking into stdout as well as raw-mode
or alternate-screen cleanup failures.

**Tests executed:** Six state/input/output/terminal unit tests, five compiled-binary pipeline tests, and three pseudo-terminal tests passed. Coverage exercises arguments, files, line and NUL stdin, fuzzy unique matching, deduplication, JSON structure, original index output, no-match code `1`, invalid-input code `2`, inline multiselect, spaces inside fuzzy queries, fullscreen cancellation, terminal restoration, bracketed-paste cleanup, and clean stdout. Formatting, strict Clippy, release build, and the complete local suite passed before publication; CI runs the same locked checks and PTY harness.

**How to test:** Run `printf 'Deploy\nBuild\nTest\n' | cargo run -- choose --filter --multi --output json`. Type `bu`, press Tab to mark, then Enter. For a noninteractive contract, run `printf 'Deploy\nBuild\nTest\n' | cargo run -- choose --filter --query bld --select-1 --output json`. See `docs/CHOOSE.md` for complete examples.

**Known limitations:** Mouse selection, preview panes, disabled/grouped structured items, user theme files, shell completion generation, and Windows piped-stdin controlling-terminal behavior remain outside this `choose` baseline. The current JSON input surface is line-oriented strings; structured item metadata belongs to the registry/schema block.

**Next:** Build `confirm` and `input` as one complete form-interaction vertical slice reusing terminal lifecycle, themes, viewports, stdout contracts, and integration harnesses.
