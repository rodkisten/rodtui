# `rodtui choose`

`choose` is the first complete rodtui vertical feature: typed CLI arguments,
multiple input sources, interactive Ratatui rendering, fuzzy search,
multiselection, themes, stable stdout encoders, documented exit codes, and
end-to-end tests all share one state model.

## Quick start

```sh
rodtui choose Deploy Build Test
```

The default viewport is compact and inline. Add `--fullscreen` when the
component should own the alternate screen.

## Input sources

Positional values, repeatable files, and piped stdin are appended in that
order:

```sh
rodtui choose Deploy Build Test
rodtui choose --input-file environments.txt
printf 'Deploy\nBuild\nTest\n' | rodtui choose
rodtui choose Local --input-file shared.txt < generated.txt
```

Use `--no-stdin` when a parent process supplies a noninteractive stdin that
must be ignored. Use `--read0` for NUL-delimited input and `--dedupe` to retain
only the first occurrence of each value.

## Fuzzy search

Live fuzzy search uses `nucleo-matcher`, the matching engine used by modern
terminal pickers:

```sh
printf 'production\nstaging\npreview\n' |
  rodtui choose --filter --query prod
```

`--query` enables filtering automatically. Case behavior is `smart` by
default and can be changed with `--case sensitive` or `--case insensitive`.
Matches are sorted by score unless `--no-sort` preserves input order.

For a noninteractive pipeline, `--select-1` accepts exactly one match without
opening a terminal UI:

```sh
printf 'Deploy\nBuild\nTest\n' |
  rodtui choose --filter --query bld --select-1
```

## Multiselection

```sh
rodtui choose Deploy Build Test Docs \
  --filter \
  --multi \
  --limit 3 \
  --selected 1
```

Tab toggles and moves forward; Shift-Tab toggles and moves backward. When fuzzy
filtering is disabled, Space also toggles the focused value. During filtering,
Space remains available for queries containing words. Enter emits marked values
in original input order. If nothing is marked, Enter emits the focused value.
`--selected` accepts repeatable original zero-based indices.

## Keyboard model

| Key | Action |
| --- | --- |
| `↑` / `↓` | Move through visible results |
| `Ctrl-P` / `Ctrl-N` | Move up/down |
| typing | Update fuzzy query when `--filter` is active |
| Backspace | Delete one query character |
| `Ctrl-U` | Clear query |
| Space | Type a space while filtering; otherwise toggle in multiselect |
| Tab / Shift-Tab | Toggle and move in multiselect |
| Enter | Accept current or marked items |
| Escape / `Ctrl-C` | Cancel |
| `j` / `k` / `q` | Navigate/cancel when filtering is disabled |

Bracketed paste appends text to the query without executing it as key commands.

## Output contracts

UI cells are rendered on stderr. Accepted data is written only to stdout.

### Text

```sh
rodtui choose Deploy Build Test --output text
```

Each selected value occupies one line.

### Original indices

```sh
rodtui choose Deploy Build Test --output index
```

Indices always refer to the original combined input, even after fuzzy sorting.

### JSON

```sh
printf 'Deploy\nBuild\nTest\n' |
  rodtui choose --filter --multi --output json
```

```json
{
  "query": "bu",
  "selected": [{ "index": 1, "value": "Build" }]
}
```

`--print0` replaces the final newline or text record delimiters with NUL bytes.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | Selection accepted and output written |
| `1` | `--select-1` found no match |
| `2` | Invalid arguments, input, UTF-8, file, terminal, or runtime error |
| `130` | User cancelled with Escape, `q`, or `Ctrl-C` |

Broken stdout pipes terminate cleanly with code `0`.

## Themes and layout

```sh
rodtui choose Deploy Build Test --theme rod
rodtui choose Deploy Build Test --theme dracula --fullscreen
rodtui choose Deploy Build Test --theme nord --width 34
```

Presets: `rod`, `graphite`, `dracula`, `nord`, `gruvbox`, and `monochrome`.
Every semantic token can be overridden individually. Examples:

```sh
rodtui choose Deploy Build Test \
  --theme graphite \
  --selected-bg '#7c5cff' \
  --border-color ansi:99 \
  --title-color light-magenta
```

Instruction copy adapts to terminal width. Force a density with
`--instruction-mode full`, `compact`, `minimal`, or `hidden`.

## Script examples

Choose a Git branch:

```sh
git for-each-ref --format='%(refname:short)' refs/heads |
  rodtui choose --filter --title Branch
```

Choose several changed files as JSON:

```sh
git status --short | cut -c4- |
  rodtui choose --filter --multi --output json --title 'Changed files'
```

Resolve a unique deployment target without opening a UI:

```sh
printf 'production\nstaging\npreview\n' |
  rodtui choose --filter --query production --select-1
```
