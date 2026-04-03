# Contributing to patchix

Thank you for your interest in contributing!

## Development Setup

```sh
# Using Nix (recommended)
nix develop

# Without Nix: install Rust toolchain via rustup
rustup toolchain install stable
```

## Running Tests

```sh
cargo test              # all tests (unit + integration)
cargo test --lib        # unit tests only
cargo test --test integration  # integration tests only (requires built binary)
```

Integration tests build and run the `patchix` binary, so they require a successful `cargo build` first.

## Code Style

```sh
cargo fmt               # format code
cargo clippy            # lint
```

The project uses `rustfmt` defaults. Please run `cargo fmt` before submitting a PR.

## Project Structure

```
src/
  main.rs          # CLI entry point (clap), file I/O, atomic write
  merge.rs         # RFC 7396 deep merge algorithm with array strategies
  formats/
    mod.rs         # Format enum + parse/serialize dispatch
    json.rs        # JSON format
    toml.rs        # TOML format (via toml crate, JSON intermediate)
    yaml.rs        # YAML format (via serde_yml)
    ini.rs         # INI format (via rust-ini, __global__ sentinel)
    reg.rs         # Windows Registry .reg format (custom parser/serializer)
tests/
  integration.rs   # CLI integration tests (spawn patchix binary)
nix/
  module.nix       # NixOS module
  package.nix      # Nix package build
  shell.nix        # Dev shell
```

## Submitting Changes

1. Fork the repository
2. Create a branch: `git checkout -b fix/description`
3. Make your changes with tests
4. Run `cargo test` and `cargo clippy` — both must pass
5. Submit a pull request with a clear description of the change

## Reporting Bugs

Please open a GitHub issue with:
- patchix version (`patchix --version`)
- The command you ran
- The existing config file contents (sanitized if sensitive)
- The patch file contents
- The actual vs. expected output

## License

By contributing, you agree that your contributions will be licensed under AGPL-3.0-or-later.
