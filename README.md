# patchix

Merge declarative Nix patches into mutable config files at activation time.

> **Version:** 0.1.1 — [Changelog](CHANGELOG.md)

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

    ".wine/user.reg" = {
      format = "reg";
      value = {
        "HKEY_CURRENT_USER\\Software\\Wine\\Direct3D" = {
          "UseGLSL" = { type = "sz"; value = "enabled"; };
          "VideoMemorySize" = { type = "dword"; value = 512; };
        };
      };
    };
  };
}
```

Each user with at least one enabled patch gets a systemd oneshot that runs `patchix merge` per file on
activation.

## Options

`patchix.users.<name>.patches.<path>`:

| Option                 | Default      |                                                                       |
| ---------------------- | ------------ | --------------------------------------------------------------------- |
| `format`               | _(required)_ | `"json"` `"toml"` `"yaml"` `"ini"` `"reg"`                            |
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
`ini`/`conf`/`cfg`, `reg`.

TOML datetimes round-trip as strings. INI sections map to top-level keys;
sectionless (global) keys are grouped under `__global__`. YAML uses the `serde_yml` library (v0.0.12) which preserves bare `yes`/`no` as strings rather than coercing them to booleans. However, bare numbers are parsed as numeric types. Be aware that upgrading `serde_yml` or switching YAML libraries may change this behavior — quoting values (`"yes"`, `"123"`) is the safest way to guarantee string preservation.

**INI `__global__` section**: Keys that appear before any `[section]` header in an INI file (sometimes called "global" or "default" keys) are represented under the special key `__global__`. In a Nix patch:

```nix
".config/foot/foot.ini" = {
  format = "ini";
  value = {
    __global__ = { font = "monospace:size=12"; };   # sectionless keys
    main = { dpi-aware = "yes"; };
  };
};
```

The name `__global__` is case-sensitive and is reserved — do not use it as an actual INI section name.

Registry patches use typed values because `.reg` entries are not plain JSON
scalars:

```nix
{
  "HKEY_CURRENT_USER\\Software\\Wine\\Direct3D" = {
    "UseGLSL" = { type = "sz"; value = "enabled"; };
    "VideoMemorySize" = { type = "dword"; value = 512; };
    "BinaryBlob" = { type = "hex"; value = "01,02,ff"; };
    "(default)" = { type = "sz"; value = "default text"; };
    "DeprecatedValue" = null;
  };

  "HKEY_CURRENT_USER\\Software\\Wine\\ObsoleteKey" = null;
}
```

## Known Limitations

**INI**: Comments (`; …` and `# …`) in existing INI files are **not preserved** — they are permanently removed on the first merge. This is a limitation of the underlying `rust-ini` parser. If your INI config contains important comments, consider using a different format or keeping a separate reference copy.

**INI**: Values are always treated as strings. There is no integer or boolean type in INI; a numeric value in a Nix patch (`alpha = 0.85`) will be stored as the string `"0.85"`.

**TOML datetimes**: TOML `datetime` values are round-tripped as strings after a merge (they lose their native TOML datetime type). Use quoted strings in your Nix patch to match: `value = "2024-01-15T12:00:00Z"`.

**YAML implicit typing**: The current `serde_yml` library (v0.0.12) preserves bare `yes`/`no`/`on`/`off` as strings rather than coercing them to booleans, but this behavior is library-version-dependent. Bare numbers like `123` are parsed as integers. For maximum safety, quote values in your YAML patches to guarantee they remain strings: use `"yes"` instead of `yes`.

**YAML `~`**: In YAML, `~` is equivalent to `null`. Under clobber mode, a patch with `key: ~` will **delete** the key from the config. This may be surprising — use explicit `null` in your Nix config only when you intend deletion.

**Multi-document YAML**: YAML files with multiple `---` document separators are not supported. Only single-document YAML files can be patched.

**Systemd re-apply**: The generated systemd service runs on every boot as a oneshot (`RemainAfterExit = true` keeps it marked "active" until next reboot). Patches are re-merged each activation. Because the merge is idempotent under `clobber = true`, this is safe — but under `clobber = false`, new patch keys are still added while existing values are preserved. To manually re-apply mid-session: `sudo systemctl restart patchix-<username>.service`.

## CLI

```sh
patchix merge -e config.json -p patch.json
patchix merge -e config.json -p patch.json --no-clobber
patchix merge -e config.json -p patch.json --array-strategy 'plugins=append'
patchix merge -e config.toml -p patch.toml -o merged.toml
patchix merge -e config.yml -p patch.yml --default-array append
patchix merge -e config -p patch.json --format json
patchix merge -e user.reg -p patch.json --format reg --patch-format json
```

> **Note:** There is currently no `--dry-run` mode. To test a merge safely, use `-o /tmp/preview.json` to write the result to a separate file without modifying the original.

### Cross-format patching

When the existing config and the patch file use different formats, specify both:

```sh
# Existing file is .reg, patch is JSON (as generated by the Nix module)
patchix merge -e user.reg -p patch.json --format reg --patch-format json
```

`--format` sets the format for the existing config file. `--patch-format` overrides the format for the patch file (defaults to the same as `--format`).

## License

AGPL-3.0-or-later — see [LICENSE](LICENSE) and [CONTRIBUTING.md](CONTRIBUTING.md).
