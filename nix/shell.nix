{
  mkShell,
  cargo,
  rustc,
  rust-analyzer,
  clippy,
  rustfmt,
}:
mkShell {
  buildInputs = [
    cargo
    rustc
    rust-analyzer
    clippy
    rustfmt
  ];
}
