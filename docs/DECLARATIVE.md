# Registry, JSON Schema, and declarative execution

rodtui exposes the same component contract to humans, shell scripts, editors,
and generators. The executable Clap graph is inspected at runtime, so metadata
and schemas stay synchronized with the flags that the binary actually accepts.

## Inspect the registry

Print every interactive component:

```sh
rodtui registry
```

Limit the response or remove whitespace:

```sh
rodtui registry --component choose
rodtui registry --component input --compact
```

The response is versioned and contains:

- component names and descriptions;
- argument IDs and canonical flags;
- semantic aliases such as `viewport.width`, `block.border`, and `style.theme`;
- positional and repeated-value behavior;
- JSON primitive types;
- possible enum values;
- executable defaults;
- help text from the CLI definition.

Example metadata fragment:

```json
{
  "id": "theme",
  "flag": "theme",
  "aliases": ["style.theme"],
  "positional": false,
  "required": false,
  "multiple": false,
  "value_type": "string",
  "possible_values": [
    "rod",
    "graphite",
    "dracula",
    "nord",
    "gruvbox",
    "monochrome"
  ],
  "defaults": ["rod"]
}
```

This is intended for completion engines, form builders, documentation,
workflow editors, and future SDK generation.

## Generate JSON Schema

Generate Draft 2020-12 JSON Schema for every component:

```sh
rodtui schema > rodtui.schema.json
```

Or generate a smaller component-specific schema:

```sh
rodtui schema --component confirm > confirm.schema.json
rodtui schema --component input --compact
```

The schema uses one versioned variant per component. `args` rejects unknown
properties, carries enum/default/help metadata, and accepts canonical argument
IDs plus semantic dotted aliases.

Editors can associate the schema URL used by the generated document:

```json
{
  "$schema": "https://rodtui.dev/schema/v1/component.json",
  "version": 1,
  "component": "input",
  "args": {}
}
```

The hosted URL is an identifier in this phase; use `rodtui schema` to write the
current local schema until release publishing hosts the artifact.

## Declarative documents

A document has three fields:

```json
{
  "version": 1,
  "component": "choose",
  "args": {
    "items": ["Deploy", "Build", "Test"],
    "filter": true,
    "multi": true,
    "style.theme": "dracula",
    "output": "json"
  }
}
```

- `version` must currently be `1`.
- `component` is `choose`, `confirm`, or `input`.
- `args` maps directly to that component's typed CLI properties.

Run a checked-in document:

```sh
rodtui run examples/choose.json
rodtui run examples/confirm.json
rodtui run examples/input.json
```

Or provide it through stdin:

```sh
printf '%s\n' '{
  "version": 1,
  "component": "confirm",
  "args": {"message": "Deploy?", "output": "json"}
}' | rodtui run -
```

The resolved component runs through the normal CLI implementation. It does not
have a second renderer or a weaker execution path: themes, validation, terminal
cleanup, stdout, and exit codes remain identical.

## Property mapping

Canonical Rust/registry IDs use snake case:

```json
{
  "min_length": 3,
  "char_limit": 32,
  "instruction_mode": "compact"
}
```

Semantic aliases can make layout intent clearer:

```json
{
  "viewport.width": 48,
  "viewport.position": "center",
  "block.border": "rounded",
  "block.padding": 2,
  "style.theme": "nord",
  "style.selected-bg": "#7c5cff"
}
```

Booleans map to flags, scalar strings/numbers map to option values, and arrays
map to repeated flags or positional values:

```json
{
  "items": ["one", "two", "three"],
  "selected": [0, 2],
  "multi": true
}
```

Unknown keys, nested objects inside `args`, invalid enum values, incompatible
flags, and unsupported versions fail before terminal initialization with exit
code `2`.

## Dry run and debugging

Inspect the exact CLI arguments without opening a terminal:

```sh
rodtui run --dry-run examples/input.json
```

Example result:

```json
[
  "rodtui",
  "input",
  "--title",
  "Release name",
  "--required",
  "--min-length",
  "3"
]
```

The resolved argv is parsed again by Clap. This intentionally preserves its
conflict, requirement, enum, range, and help behavior rather than duplicating
validation in the JSON loader.

## Pipeline considerations

When the document itself comes from stdin, that stream is consumed before the
component starts. Put candidates and initial values in the document:

```json
{
  "version": 1,
  "component": "input",
  "args": {"value": "release/next", "required": true}
}
```

For interactive stdin-backed component data, save the document to a file and
use the component's ordinary CLI input contracts where appropriate.

## Compatibility contract

- Registry and document format versioning is independent from the binary
  package version.
- Version `1` never silently accepts unknown properties.
- New optional component arguments may appear without changing the version.
- Removing or changing a property requires a new document version or an
  explicit migration path.
- Metadata is generated from the CLI command graph, keeping executable flags
  and registry output in the same test surface.
