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
