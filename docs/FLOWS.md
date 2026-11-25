# Conditional multi-step flows

`rodtui flow` composes `choose`, `confirm`, and `input` into one validated
interaction. A flow can carry structured values between screens, skip steps,
branch on a negative confirmation, redact sensitive results, and return one
machine-readable report.

This is a complete runtime path, not a separate collection of widgets. Every
step becomes a declarative component document, resolves against the executable
registry, is parsed by Clap, and calls the same implementation as the direct
CLI command.

## Start here

Inspect the primary route without opening a terminal:

```sh
cargo run -- flow --dry-run examples/release-flow.json
```

Run the interactive release assistant:

```sh
cargo run -- flow examples/release-flow.json
```

Run the fully automated fixture, useful as a first local smoke test:

```sh
cargo run -- flow examples/automated-flow.json
```

A flow document may also come from stdin:

```sh
printf '%s\n' '{
  "version": 1,
  "steps": [
    {
      "id": "approval",
      "component": "confirm",
      "args": { "message": "Ship it?", "yes": true }
    }
  ]
}' | cargo run -- flow - --compact
```

When the document itself is read from stdin, that stream is already consumed.
Use embedded values/noninteractive flags in its steps, or place the document in
a file when a component must consume pipeline input.

## Document contract

```json
{
  "version": 1,
  "name": "Release assistant",
  "start": "release",
  "max_steps": 100,
  "variables": {
    "environment": "production"
  },
  "steps": [
    {
      "id": "release",
      "component": "input",
      "save_as": "release",
      "sensitive": false,
      "when": {
        "path": "variables.environment",
        "equals": "production"
      },
      "next": "approval",
      "args": {
        "title": "Release name",
        "required": true,
        "style.theme": "nord"
      }
    },
    {
      "id": "approval",
      "component": "confirm",
      "next_on_false": "reason",
      "args": {
        "message": "Deploy ${variables.release.value}?"
      }
    }
  ]
}
```

Top-level properties:

| Property | Required | Meaning |
| --- | --- | --- |
| `version` | yes | Flow format version. Version 1 is supported. |
| `name` | no | Human-readable name copied into the final report. |
| `start` | no | First step ID. Defaults to the first array entry. |
| `variables` | no | Initial JSON values available to conditions/interpolation. |
| `steps` | yes | Non-empty component graph. IDs must be unique. |
| `max_steps` | no | Transition ceiling; defaults to 100. |

Step properties:

| Property | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Stable ASCII identifier using letters, numbers, `_`, or `-`. |
| `component` | yes | `choose`, `confirm`, or `input`. |
| `args` | no | Normal registry properties for that component. |
| `when` | no | Condition that decides whether the step runs. |
| `save_as` | no | Copies the structured result into `variables.<name>`. |
| `sensitive` | no | Redacts this result and alias from the final report. |
| `next` | no | Explicit transition after success/true; otherwise array order. |
| `next_on_false` | no | Alternate transition for `confirm: false`. |

Unknown fields fail before execution because the format uses strict objects.
Unknown component arguments also fail through the normal registry. This catches
misspellings before an interactive screen opens.

## Results and shared variables

Every completed step writes a structured result under `steps.<id>`.

An input result:

```json
{ "status": "accepted", "value": "v2.0.0" }
```

A confirmation result:

```json
{ "status": "accepted", "confirmed": true }
```

A choose result preserves both convenient values and original indices:

```json
{
  "status": "accepted",
  "query": "dpl",
  "value": "Deploy",
  "values": ["Deploy"],
  "selected": [{ "index": 0, "value": "Deploy" }]
}
```

`save_as: "release"` makes the same object available as
`variables.release`. Initial variables and saved results therefore use one
predictable lookup model.

## Interpolation

Placeholders work recursively inside strings, arrays, and objects in `args`.

```json
{
  "message": "Deploy ${variables.release.value} to ${variables.environment}?"
}
```

An exact placeholder preserves its JSON type:

```json
{
  "some_future_boolean_property": "${steps.approval.confirmed}"
}
```

In that example the resolved value is the boolean `true`, not the string
`"true"`. A placeholder embedded in surrounding text accepts string, number,
boolean, or null and renders text. Embedding an array or object is rejected;
use an exact placeholder when a component property accepts structured data.

Missing paths and unterminated placeholders are errors. Paths may traverse
objects and use numeric array indices:

```text
variables.release.value
steps.action.selected.0.index
```

## Conditions

`when` evaluates immediately before argument interpolation. A false condition
records `{ "status": "skipped" }`, then follows `next` or normal array order.

Equality:

```json
{ "path": "variables.environment", "equals": "production" }
```

Inequality:

```json
{ "path": "steps.action.value", "not_equals": "Build only" }
```

Existence:

```json
{ "path": "variables.token", "exists": true }
```

With no comparator, the path uses JSON truthiness: null, false, zero, empty
text, empty arrays, and empty objects are false; other values are true.

## Branches and loops

Without `next`, execution advances to the next array item. `next` targets any
known step. A confirmation that returns false follows `next_on_false`; without
that transition the flow stops with status `rejected` and exit code 1.

Explicit transitions may form loops. `max_steps` is a mandatory safety model,
not a graph restriction. The CLI can override the document for one run:

```sh
rodtui flow workflow.json --max-steps 20
```

Exceeding the limit is an error with exit code 2. `--dry-run` reports the
validated primary route and stops when it reaches a terminal node or revisits a
node; it cannot predict runtime conditions or negative branches.

## Sensitive values

Mark a step when its accepted value must not appear in the report:

```json
{
  "id": "token",
  "component": "input",
  "save_as": "token",
  "sensitive": true,
  "args": { "password": true, "required": true }
}
```

The real result remains available internally for later interpolation. Both
`steps.token` and `variables.token` become this in stdout:

```json
{ "status": "accepted", "redacted": true }
```

Redaction protects the final report. If a flow author interpolates the secret
into a later visible message, that message can still expose it on the terminal.
Password mode masks typing but cannot protect shell history or downstream
systems.

## Final report

The runtime writes no partial values to stdout. After terminal cleanup it emits
one report:

```json
{
  "version": 1,
  "name": "Automated release validation",
  "status": "completed",
  "exit_code": 0,
  "order": ["release", "action", "approval"],
  "variables": {
    "environment": "staging",
    "release": { "status": "accepted", "value": "v2.0.0" }
  },
  "steps": {
    "release": { "status": "accepted", "value": "v2.0.0" },
    "action": {
      "status": "accepted",
      "query": "dpl",
      "value": "Deploy",
      "values": ["Deploy"],
      "selected": [{ "index": 0, "value": "Deploy" }]
    },
    "approval": { "status": "accepted", "confirmed": true }
  }
}
```

Use `--compact` for JSONL-friendly single-line output.

## Exit codes

| Code | Report status | Meaning |
| --- | --- | --- |
| `0` | `completed` or `planned` | Flow completed or dry-run validation passed. |
| `1` | `rejected` or `no_match` | Unhandled false confirmation or choose no-match. |
| `2` | no report required | Invalid JSON/graph/argument, exceeded limit, or runtime error. |
| `130` | `cancelled` | User cancelled an interactive component. |

`status`, `exit_code`, and the process code agree. This allows shell scripts to
branch on the process while logs and higher-level callers inspect the report.

## Validation and testing

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked
python3 tests/flow_pty.py target/debug/rodtui
```

The compiled-binary suite covers interpolation, shared variables, conditions,
redaction, negative branching, rejection, dry-run planning, graph errors, and
cycle limits. The PTY suite enters a value into a real input screen, verifies
that it appears in the following confirmation, and asserts terminal mode,
alternate-screen, and bracketed-paste restoration.
