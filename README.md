# patchix

Declarative config patching for Nix. Deep-merge Nix-declared values into mutable application config files (JSON, TOML, YAML, INI) without clobbering runtime changes.

## Problem

Nix config management tools (hjem, Home Manager) write config files as read-only symlinks into `/nix/store`. Applications that need to write to their own configs (VS Code settings, Claude Code plugin toggles, etc.) can't — the files are immutable.

The workaround is making files writable copies, but then `nixos-rebuild` overwrites everything the application changed.

patchix solves this: declare the keys you care about in Nix, and patchix deep-merges them into the existing mutable file at activation time. Keys you didn't declare are left alone.

## Usage

### CLI

```sh
# Basic merge — patch values overwrite existing values
patchix merge --existing config.json --patch patch.json

# No-clobber — only fill in missing keys, preserve runtime changes
patchix merge --existing config.json --patch patch.json --no-clobber

# Per-path array strategies
patchix merge --existing config.json --patch patch.json \
  --default-array replace \
  --array-strategy 'plugins=append' \
  --array-strategy 'keybinds=union'

# Explicit format (auto-detected from extension by default)
patchix merge --existing config --patch patch --format toml

# Output to a different file
patchix merge --existing config.json --patch patch.json --output merged.json
```

### NixOS Module

```nix
# flake.nix
{
  inputs.patchix.url = "github:y0usaf/patchix";
  inputs.patchix.inputs.nixpkgs.follows = "nixpkgs";
}
```

```nix
# configuration.nix
{ inputs, ... }: {
  imports = [ inputs.patchix.nixosModules.default ];

  patchix.enable = true;
  patchix.users.alice.patches = {
    # Clobber mode (default): declared values always win
    ".config/starship.toml" = {
      format = "toml";
      value.character.success_symbol = "[>](bold green)";
    };

    # No-clobber mode: declared values are defaults, runtime changes preserved
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

## Merge behavior

### Objects

Recursive deep merge. Patch keys are inserted into the existing object. Nested objects are always recursed into regardless of `clobber`.

### Scalars

| Mode | Existing | Patch | Result |
|------|----------|-------|--------|
| `clobber = true` | `"light"` | `"dark"` | `"dark"` |
| `clobber = false` | `"light"` | `"dark"` | `"light"` |
| either | _(missing)_ | `"dark"` | `"dark"` |

### Arrays

With `clobber = true`, the array strategy applies:

| Strategy | Existing | Patch | Result |
|----------|----------|-------|--------|
| `replace` | `[a, b]` | `[c]` | `[c]` |
| `append` | `[a, b]` | `[c]` | `[a, b, c]` |
| `prepend` | `[a, b]` | `[c]` | `[c, a, b]` |
| `union` | `[a, b]` | `[b, c]` | `[a, b, c]` |

With `clobber = false`, existing arrays are preserved.

### Key deletion

Set a key to `null` in the patch to delete it from the target (RFC 7386).

## NixOS module options

### `patchix.enable`

Enable the patchix systemd services.

### `patchix.users.<name>.patches.<path>`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | bool | `true` | Whether this patch is active |
| `format` | enum | _(required)_ | `"json"`, `"toml"`, `"yaml"`, or `"ini"` |
| `value` | attrs | `{}` | Patch content as a Nix attrset |
| `clobber` | bool | `true` | Overwrite existing values. `false` = only set missing keys |
| `defaultArrayStrategy` | enum | `"replace"` | `"replace"`, `"append"`, `"prepend"`, or `"union"` |
| `arrayStrategies` | attrsOf enum | `{}` | Per-path overrides (dot-separated keys) |

Patch targets are relative to the user's home directory. Each user gets a systemd oneshot service (`patchix-<username>`) that runs at activation.

## Formats

All formats are parsed into a common intermediate representation for merging, then serialized back. Format-specific notes:

- **JSON**: Parsed/serialized with `serde_json`. Output is pretty-printed.
- **TOML**: Datetimes are round-tripped as strings. Null values are not supported (TOML has no null).
- **YAML**: Parsed/serialized with `serde_yml`.
- **INI**: Sections become top-level object keys. Sectionless keys go under `__global__`. All values are strings.

## Building

```sh
nix build            # via flake
cargo build --release  # directly
cargo test             # run tests
```

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
