# Changelog

All notable changes to patchix are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
patchix uses [Semantic Versioning](https://semver.org/).

---

## [0.1.1] — 2025-03-22

### Added
- `reg` format: Wine Registry (`WINE REGISTRY Version 2`) support including:
  - Short-path normalization (Wine omits `HKEY_CURRENT_USER\` prefix in section headers)
  - Preamble, mtime, and `#time=` metadata preservation on round-trip
  - `str(N):"…"` Wine human-readable encoding for `expand_sz` and `multi_sz`
  - `qword:` and `hex(b):` QWORD value support
- `--patch-format` flag: allows the patch file to use a different format than the existing config (e.g., patching a `.reg` file with a JSON patch)
- `defaultArrayStrategy` and `arrayStrategies` per-path overrides in the Nix module

### Fixed
- Wine format: section headers now correctly use single backslashes (Wine requirement)
- `expand_sz`: trailing NUL byte is now properly stripped after decoding UTF-16LE
- Internal metadata keys (`__header__`, `__preamble__`, `__mtime__`, `__time__`) are now only stripped from `.reg` format patches, not from JSON/TOML/YAML/INI patches

### Changed
- Nix module: path traversal assertion now correctly rejects absolute paths and uses component-level `..` detection
- Nix module: systemd services now include security hardening (`NoNewPrivileges`, `ProtectSystem`, etc.)
- Nix module: services are no longer created for users whose patches are all `enable = false`

---

## [0.1.0] — 2025-03-12

### Added
- Initial release
- Formats: `json`, `toml`, `yaml`, `ini`, `reg`
- RFC 7396 deep merge with null-as-delete semantics
- `--no-clobber` mode: fills in missing keys without overwriting existing values
- Array merge strategies: `replace` (default), `append`, `prepend`, `union`
- Per-path array strategy overrides via `--array-strategy path=strategy`
- Atomic write via tempfile + rename (preserves original file permissions)
- NixOS module with per-user patch declarations and systemd oneshot services
- Auto-detection of config format from file extension
