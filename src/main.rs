use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

mod formats;
mod merge;

use formats::Format;
use merge::{ArrayStrategy, MergeConfig};

#[derive(Parser)]
#[command(
    name = "patchix",
    about = "Declarative config file patcher for Nix",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Merge a patch into an existing config file
    Merge(MergeArgs),
}

#[derive(clap::Args)]
struct MergeArgs {
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

    /// Patch file format (defaults to --format / detected existing format)
    #[arg(long)]
    patch_format: Option<FormatArg>,

    /// Default array merge strategy
    #[arg(long, default_value = "replace")]
    default_array: ArrayStrategyArg,

    /// Per-path array strategy overrides (e.g. 'plugins=append')
    #[arg(long = "array-strategy", value_parser = parse_path_strategy)]
    array_strategies: Vec<(String, ArrayStrategy)>,

    /// Don't overwrite existing values — only fill in missing keys. Null patch values are ignored (not deleted)
    #[arg(long)]
    no_clobber: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Json,
    Toml,
    Yaml,
    Ini,
    Reg,
}

impl From<FormatArg> for Format {
    fn from(a: FormatArg) -> Self {
        match a {
            FormatArg::Json => Format::Json,
            FormatArg::Toml => Format::Toml,
            FormatArg::Yaml => Format::Yaml,
            FormatArg::Ini => Format::Ini,
            FormatArg::Reg => Format::Reg,
        }
    }
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
    let strategy_arg = ArrayStrategyArg::from_str(strategy, true).map_err(|_| {
        let valid: Vec<String> = ArrayStrategyArg::value_variants()
            .iter()
            .filter_map(|v| v.to_possible_value().map(|pv| pv.get_name().to_string()))
            .collect();
        format!(
            "unknown strategy '{}'; valid values: {}",
            strategy,
            valid.join(", ")
        )
    })?;
    Ok((path.to_string(), strategy_arg.into()))
}

fn detect_format(path: &Path) -> Result<Format> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Ok(Format::Json),
        Some("toml") => Ok(Format::Toml),
        Some("yaml" | "yml") => Ok(Format::Yaml),
        Some("ini" | "conf" | "cfg") => Ok(Format::Ini),
        Some("reg") => Ok(Format::Reg),
        Some(ext) => anyhow::bail!("unknown file extension '.{ext}', use --format to specify"),
        None => anyhow::bail!("no file extension, use --format to specify"),
    }
}

fn run_merge(args: MergeArgs) -> Result<()> {
    let MergeArgs {
        existing,
        patch,
        output,
        format,
        patch_format,
        default_array,
        array_strategies,
        no_clobber,
    } = args;

    let format = format
        .map(Format::from)
        .map_or_else(|| detect_format(&existing), Ok)?;
    let patch_format = patch_format.map(Format::from).unwrap_or(format);

    let config = MergeConfig {
        default_array: default_array.into(),
        path_strategies: array_strategies.into_iter().collect::<HashMap<_, _>>(),
        clobber: !no_clobber,
    };

    // Read existing file (empty object if missing); avoid TOCTOU from exists()+read pair
    let existing_content = match std::fs::read_to_string(&existing) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", existing.display())),
    };

    let patch_content =
        std::fs::read_to_string(&patch).with_context(|| format!("reading {}", patch.display()))?;

    let existing_val = if existing_content.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        formats::parse(&existing_content, format)
            .with_context(|| format!("parsing {}", existing.display()))?
    };

    let mut patch_val = formats::parse(&patch_content, patch_format)
        .with_context(|| format!("parsing {}", patch.display()))?;

    // Strip internal metadata keys from REG patches — these are patchix internals
    // that must not overwrite the existing file's header/preamble/timestamps.
    // (For other formats these key names are valid user data and must not be touched.)
    if patch_format == Format::Reg {
        if let Some(obj) = patch_val.as_object_mut() {
            obj.remove("__header__");
            obj.remove("__preamble__");
            for section_val in obj.values_mut() {
                if let Some(section) = section_val.as_object_mut() {
                    // Only strip __mtime__/__time__ when they are strings (internal
                    // patchix metadata). Real registry values are objects with
                    // {type, value} structure and must not be dropped.
                    if matches!(section.get("__mtime__"), Some(v) if v.is_string()) {
                        section.remove("__mtime__");
                    }
                    if matches!(section.get("__time__"), Some(v) if v.is_string()) {
                        section.remove("__time__");
                    }
                }
            }
        }
    }

    if patch_val.is_null() {
        anyhow::bail!(
            "patch file {} is empty or null — nothing to merge",
            patch.display()
        );
    }

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
    let parent = output_path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temp file in {}", parent.display()))?;
    // Preserve original file permissions so the atomic rename doesn't change access modes
    if let Ok(meta) = std::fs::metadata(output_path) {
        if let Err(e) = std::fs::set_permissions(tmp.path(), meta.permissions()) {
            eprintln!("patchix: warning: could not preserve file permissions on {}: {e}",
                output_path.display());
        }
    }
    tmp.write_all(result.as_bytes())
        .context("writing temp file")?;
    tmp.persist(output_path).map_err(|e| {
        anyhow::anyhow!(
            "renaming temp file to {}: {}",
            output_path.display(),
            e.error
        )
    })?;

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Merge(args) => run_merge(args),
    }
}
