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
