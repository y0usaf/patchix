# patchix

Merge declarative Nix patches into mutable config files at activation time.

## Setup

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

    ".config/starship.toml" = {
      format = "toml";
      value.character.success_symbol = "[>](bold green)";
    };

    # clobber = false: only write keys not already present on disk
    ".config/Code/User/settings.json" = {
      format = "json";
      clobber = false;
      value = {
        "editor.fontSize" = 14;
        "editor.tabSize" = 2;
      };
    };

    # __global__ holds sectionless keys.
    ".config/foot/config.ini" = {
      format = "ini";
      value = {
        __global__ = { font = "monospace:size=12"; };
        colors-dark = { alpha = 0.85; };
      };
    };
  };
}
```

Each user gets a systemd oneshot that runs `patchix merge` per file on
activation.

## Options

`patchix.users.<name>.patches.<path>`:

| Option                 | Default      |                                                                       |
| ---------------------- | ------------ | --------------------------------------------------------------------- |
| `format`               | _(required)_ | `"json"` `"toml"` `"yaml"` `"ini"`                                    |
| `value`                | `{}`         | Patch content as Nix attrset                                          |
| `clobber`              | `true`       | `true`: overwrite existing values. `false`: only fill in missing keys |
| `defaultArrayStrategy` | `"replace"`  | `"replace"` `"append"` `"prepend"` `"union"`                          |
| `arrayStrategies`      | `{}`         | Per-path overrides (dot-separated)                                    |
| `enable`               | `true`       | Toggle this patch                                                     |

Both modes recurse into nested objects. Setting a key to `null` deletes it (RFC
7396). Under `--no-clobber`, null patch values are ignored — they do not delete
the key.

### Array strategies (when `clobber = true`)

| Strategy            | `[a, b]` + `[c]`           |
| ------------------- | -------------------------- |
| `replace` (default) | `[c]`                      |
| `append`            | `[a, b, c]`                |
| `prepend`           | `[c, a, b]`                |
| `union`             | `[a, b, c]` (deduplicated) |

Per-path: `arrayStrategies."editor.formatters" = "append";`

## Formats

Auto-detected from file extension. Supported: `json`, `toml`, `yaml`/`yml`,
`ini`/`conf`/`cfg`.

TOML datetimes round-trip as strings. INI sections map to top-level keys;
sectionless (global) keys are grouped under `__global__`. YAML uses implicit
typing: unquoted `yes`/`no`/`true`/`false` become booleans and bare numbers
become numeric — quote values to preserve them as strings.

## CLI

```sh
patchix merge -e config.json -p patch.json
patchix merge -e config.json -p patch.json --no-clobber
patchix merge -e config.json -p patch.json --array-strategy 'plugins=append'
patchix merge -e config.toml -p patch.toml -o merged.toml
patchix merge -e config.yml -p patch.yml --default-array append
patchix merge -e config -p patch.json --format json
```

## License

AGPL-3.0-or-later
