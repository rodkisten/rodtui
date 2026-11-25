# Confirm and input

`rodtui confirm` and `rodtui input` form one complete shell interaction slice.
They reuse the same semantic themes, compact inline viewport, explicit
fullscreen mode, border model, responsive instructions, terminal cleanup, and
stdout isolation as `choose`.

## Confirm

Ask a question and use the process status directly in a shell condition:

```sh
if rodtui confirm 'Deploy production?'; then
  ./deploy.sh
fi
```

The default focused choice is yes. Change labels, default, and theme without
changing the shell contract:

```sh
rodtui confirm 'Remove preview?' \
  --affirmative Remove \
  --negative Keep \
  --default no \
  --theme gruvbox
```

Keys:

- Left, Right, Tab, or Shift-Tab changes the focused choice.
- Enter accepts the focused choice.
- `y` and `n` answer directly.
- Escape or Ctrl-C cancels.

Exit codes deliberately distinguish all outcomes:

| Code | Meaning |
|---:|---|
| `0` | affirmative |
| `1` | negative |
| `2` | invalid arguments or runtime error |
| `130` | cancelled |

By default `confirm` writes nothing, which makes it safe in ordinary shell
conditions. Output can be requested explicitly:

```sh
rodtui confirm 'Continue?' --output text
# true or false

rodtui confirm 'Continue?' --output json
# {"confirmed":true}
```

Automation can bypass the terminal while retaining output and exit semantics:

```sh
rodtui confirm 'Approved by policy?' --yes --output json
rodtui confirm 'Blocked by policy?' --no --output text
```

## Input

Edit a single Unicode line:

```sh
name=$(rodtui input \
  --title 'Release name' \
  --placeholder 'v1.0.0' \
  --required \
  --theme dracula)
```

The editor supports:

- Unicode-aware cursor movement with Left, Right, Home, and End.
- Backspace and Delete at the cursor.
- Ctrl-A / Ctrl-E for start/end.
- Ctrl-U to clear the line.
- Ctrl-W to delete the previous word.
- Bracketed paste with control characters removed.
- Escape or Ctrl-C cancellation.

### Validation

```sh
rodtui input \
  --title Username \
  --required \
  --min-length 3 \
  --max-length 24 \
  --char-limit 24
```

Invalid Enter presses keep the component open and display an inline error.
Lengths count Unicode characters rather than UTF-8 bytes. `--char-limit`
prevents further editing, while `--max-length` is acceptance validation.

Leading and trailing whitespace is trimmed by default. Use `--no-trim` when it
is meaningful data.

### Password entry

```sh
token=$(rodtui input \
  --title Token \
  --password \
  --password-character '*' \
  --required)
```

The display is masked; stdout still contains the accepted value. Shell history,
process inspection, and downstream commands remain the caller's responsibility.

### Pipeline and noninteractive validation

Seed an editable field from stdin:

```sh
printf 'release/next\n' |
  rodtui input --read-stdin --title Branch --required
```

Validate and emit without opening a terminal:

```sh
rodtui input \
  --value '  Rod  ' \
  --accept \
  --required \
  --output json
# {"value":"Rod"}

printf 'pipeline value\n' |
  rodtui input --read-stdin --accept --required
```

Input exit codes are `0` for an accepted value, `2` for invalid input/errors,
and `130` for cancellation.

## Shared Ratatui configuration

Both components accept the same visual flags as `choose`:

```sh
rodtui confirm 'Continue?' \
  --theme nord \
  --width 42 \
  --position center \
  --border rounded \
  --block.padding 2

rodtui input \
  --theme monochrome \
  --fullscreen \
  --vertical-position center \
  --instruction-mode compact
```

Available presets are `rod`, `graphite`, `dracula`, `nord`, `gruvbox`, and
`monochrome`. Explicit semantic colors override only their corresponding theme
tokens:

```sh
rodtui input \
  --theme graphite \
  --selected-bg '#7c5cff' \
  --border-color ansi:99
```

Namespaced aliases such as `--viewport.width`, `--block.border`, and
`--style.selected-bg` remain available alongside the shorter flags.

## Terminal contract

- UI escape sequences are rendered to stderr.
- Accepted data is written only to stdout.
- Inline mode is the default and preserves surrounding shell context.
- `--fullscreen` explicitly enters the alternate screen.
- Raw mode, cursor visibility, bracketed paste, and alternate-screen state are
  restored on success, negative confirmation, cancellation, and errors.
- Downstream broken pipes are treated as clean completion.
