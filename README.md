# patchix

Nix is great at declaring config files. It's bad at letting apps edit them.

patchix sits in the middle: you declare the keys you care about in Nix, and patchix merges them into the actual file on disk at activation time. The app can still write whatever it wants — your declared keys get enforced on the next rebuild, everything else is left alone.

## Quick start

```nix
# flake.nix
inputs.patchix.url = "github:y0usaf/patchix";
inputs.patchix.inputs.nixpkgs.follows = "nixpkgs";
```

```nix
# configuration.nix
{ inputs, ... }: {
  imports = [ inputs.patchix.nixosModules.default ];

  patchix.enable = true;
  patchix.users.alice.patches = {

    # These values always win on rebuild
    ".config/starship.toml" = {
      format = "toml";
      value.character.success_symbol = "[>](bold green)";
    };

    # These values are defaults — if the app changed them, it sticks
    ".config/Code/User/settings.json" = {
      format = "json";
      clobber = false;
      value = {
        "editor.fontSize" = 14;
        "editor.tabSize" = 2;
      };
    };
  };
}
```

That's it. Each user gets a systemd oneshot that runs `patchix merge` per file.

## `clobber`

The interesting bit.

- **`clobber = true`** (default) — your Nix values overwrite whatever's on disk. Authoritative.
- **`clobber = false`** — your Nix values only fill in keys that don't exist yet. If the app (or user) changed a value at runtime, it's preserved.

Both modes recurse into nested objects, so you can declare defaults deep in a config tree and they'll fill in without flattening anything above them.

## Arrays

When `clobber = true`, arrays use a configurable strategy:

| Strategy | `[a, b]` + `[c]` |
|----------|-------------------|
| `replace` (default) | `[c]` |
| `append` | `[a, b, c]` |
| `prepend` | `[c, a, b]` |
| `union` | `[a, b, c]` (deduplicated) |

Set globally with `defaultArrayStrategy` or per-path:

```nix
arrayStrategies."editor.formatters" = "append";
```

When `clobber = false`, existing arrays are left alone.

## Key deletion

`null` in the patch deletes the key (per RFC 7386).

## Formats

JSON, TOML, YAML, INI. Auto-detected from file extension, or set explicitly with `format`.

TOML datetimes round-trip as strings. INI sections become top-level keys; sectionless keys go under `__global__`.

## CLI

```sh
patchix merge -e config.json -p patch.json                    # basic
patchix merge -e config.json -p patch.json --no-clobber       # defaults only
patchix merge -e config.json -p patch.json --array-strategy 'plugins=append'
patchix merge -e config.toml -p patch.toml -o merged.toml     # explicit output
```

## Module options

`patchix.users.<name>.patches.<path>`:

| Option | Default | |
|--------|---------|---|
| `format` | _(required)_ | `"json"` `"toml"` `"yaml"` `"ini"` |
| `value` | `{}` | Patch content as Nix attrset |
| `clobber` | `true` | Overwrite existing values |
| `defaultArrayStrategy` | `"replace"` | `"replace"` `"append"` `"prepend"` `"union"` |
| `arrayStrategies` | `{}` | Per-path overrides (dot-separated) |
| `enable` | `true` | Toggle this patch |

## License

AGPL-3.0-or-later
