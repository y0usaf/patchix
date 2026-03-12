use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod formats;
mod merge;

use formats::Format;
use merge::{ArrayStrategy, MergeConfig};

#[derive(Parser)]
#[command(name = "patchix", about = "Declarative config file patcher for Nix", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Merge a patch into an existing config file
    Merge {
        /// Path to the existing config file (will be created if missing)
        #[arg(short, long)]
        existing: PathBuf,

        /// Path to the patch file (from Nix store)
        #[arg(short, long)]
        patch: PathBuf,

        /// Output path (defaults to --existing, overwriting in place)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Config format (auto-detected from extension if omitted)
        #[arg(short, long)]
        format: Option<FormatArg>,

        /// Default array merge strategy
        #[arg(long, default_value = "replace")]
        default_array: ArrayStrategyArg,

        /// Per-path array strategy overrides (e.g. 'plugins=append')
        #[arg(long = "array-strategy", value_parser = parse_path_strategy)]
        array_strategies: Vec<(String, ArrayStrategy)>,

        /// Don't overwrite existing values — only fill in missing keys
        #[arg(long)]
        no_clobber: bool,
    },
}

#[derive(Clone, ValueEnum)]
enum FormatArg {
    Json,
    Toml,
    Yaml,
    Ini,
}

#[derive(Clone, ValueEnum)]
enum ArrayStrategyArg {
    Replace,
    Append,
    Prepend,
    Union,
}

impl From<ArrayStrategyArg> for ArrayStrategy {
    fn from(a: ArrayStrategyArg) -> Self {
        match a {
            ArrayStrategyArg::Replace => ArrayStrategy::Replace,
            ArrayStrategyArg::Append => ArrayStrategy::Append,
            ArrayStrategyArg::Prepend => ArrayStrategy::Prepend,
            ArrayStrategyArg::Union => ArrayStrategy::Union,
        }
    }
}

fn parse_path_strategy(s: &str) -> Result<(String, ArrayStrategy), String> {
    let (path, strategy) = s
        .split_once('=')
        .ok_or_else(|| format!("expected 'path=strategy', got '{s}'"))?;
    let strategy = match strategy {
        "replace" => ArrayStrategy::Replace,
        "append" => ArrayStrategy::Append,
        "prepend" => ArrayStrategy::Prepend,
        "union" => ArrayStrategy::Union,
        _ => return Err(format!("unknown strategy '{strategy}'")),
    };
    Ok((path.to_string(), strategy))
}

fn detect_format(path: &Path) -> Result<Format> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json" | "jsonc") => Ok(Format::Json),
        Some("toml") => Ok(Format::Toml),
        Some("yaml" | "yml") => Ok(Format::Yaml),
        Some("ini" | "conf" | "cfg") => Ok(Format::Ini),
        Some(ext) => anyhow::bail!("unknown file extension '.{ext}', use --format to specify"),
        None => anyhow::bail!("no file extension, use --format to specify"),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Merge {
            existing,
            patch,
            output,
            format,
            default_array,
            array_strategies,
            no_clobber,
        } => {
            let format = match format {
                Some(FormatArg::Json) => Format::Json,
                Some(FormatArg::Toml) => Format::Toml,
                Some(FormatArg::Yaml) => Format::Yaml,
                Some(FormatArg::Ini) => Format::Ini,
                None => detect_format(&existing)?,
            };

            let config = MergeConfig {
                default_array: default_array.into(),
                path_strategies: array_strategies.into_iter().collect::<HashMap<_, _>>(),
                clobber: !no_clobber,
            };

            // Read existing file (empty object if missing)
            let existing_content = if existing.exists() {
                std::fs::read_to_string(&existing)
                    .with_context(|| format!("reading {}", existing.display()))?
            } else {
                String::new()
            };

            let patch_content = std::fs::read_to_string(&patch)
                .with_context(|| format!("reading {}", patch.display()))?;

            let existing_val = if existing_content.is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                formats::parse(&existing_content, format)
                    .with_context(|| format!("parsing {}", existing.display()))?
            };

            let patch_val = formats::parse(&patch_content, format)
                .with_context(|| format!("parsing {}", patch.display()))?;

            let merged = merge::merge(existing_val, patch_val, &config, "");

            let result = formats::serialize(&merged, format)?;

            // Atomic write: write to temp file then rename
            let output_path = output.as_ref().unwrap_or(&existing);
            if let Some(parent) = output_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating directory {}", parent.display()))?;
                }
            }
            let tmp_name = format!(
                "{}.{}.patchix-tmp",
                output_path.file_name().unwrap_or_default().to_string_lossy(),
                std::process::id()
            );
            let tmp_path = output_path.with_file_name(tmp_name);
            std::fs::write(&tmp_path, &result)
                .with_context(|| format!("writing {}", tmp_path.display()))?;
            std::fs::rename(&tmp_path, output_path)
                .with_context(|| format!("renaming to {}", output_path.display()))?;

            Ok(())
        }
    }
}
