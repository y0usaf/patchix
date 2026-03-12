use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn patchix() -> Command {
    Command::new(env!("CARGO_BIN_EXE_patchix"))
}

fn tmpdir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("patchix-test-{}-{}", std::process::id(), n));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// --- JSON ---

#[test]
fn json_basic_merge() {
    let dir = tmpdir();
    let existing = dir.join("config.json");
    let patch = dir.join("patch.json");
    let output = dir.join("output.json");

    fs::write(&existing, r#"{"a": 1, "b": 2}"#).unwrap();
    fs::write(&patch, r#"{"b": 99, "c": 3}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "-o", s(&output)])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(result["a"], 1);
    assert_eq!(result["b"], 99);
    assert_eq!(result["c"], 3);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_creates_missing_file() {
    let dir = tmpdir();
    let existing = dir.join("new.json");
    let patch = dir.join("patch.json");

    fs::write(&patch, r#"{"x": 42}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["x"], 42);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_creates_parent_directories() {
    let dir = tmpdir();
    let existing = dir.join("deep/nested/config.json");
    let patch = dir.join("patch.json");

    fs::write(&patch, r#"{"key": "value"}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(existing.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_no_clobber() {
    let dir = tmpdir();
    let existing = dir.join("config.json");
    let patch = dir.join("patch.json");

    fs::write(&existing, r#"{"a": 1, "b": 2}"#).unwrap();
    fs::write(&patch, r#"{"a": 99, "c": 3}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--no-clobber"])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["a"], 1);   // preserved
    assert_eq!(result["b"], 2);   // preserved
    assert_eq!(result["c"], 3);   // new key filled in
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_null_deletes_key() {
    let dir = tmpdir();
    let existing = dir.join("config.json");
    let patch = dir.join("patch.json");

    fs::write(&existing, r#"{"a": 1, "b": 2}"#).unwrap();
    fs::write(&patch, r#"{"b": null}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["a"], 1);
    assert!(result.get("b").is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_array_append() {
    let dir = tmpdir();
    let existing = dir.join("config.json");
    let patch = dir.join("patch.json");

    fs::write(&existing, r#"{"plugins": ["a", "b"]}"#).unwrap();
    fs::write(&patch, r#"{"plugins": ["c"]}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--array-strategy", "plugins=append",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["plugins"], serde_json::json!(["a", "b", "c"]));
    fs::remove_dir_all(dir).unwrap();
}

// --- TOML ---

#[test]
fn toml_round_trip() {
    let dir = tmpdir();
    let existing = dir.join("config.toml");
    let patch = dir.join("patch.toml");

    fs::write(&existing, "name = \"old\"\ncount = 1\n").unwrap();
    fs::write(&patch, "name = \"new\"\nextra = true\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("name = \"new\""));
    assert!(content.contains("count = 1"));
    assert!(content.contains("extra = true"));
    fs::remove_dir_all(dir).unwrap();
}

// --- YAML ---

#[test]
fn yaml_round_trip() {
    let dir = tmpdir();
    let existing = dir.join("config.yaml");
    let patch = dir.join("patch.yaml");

    fs::write(&existing, "name: old\ncount: 1\n").unwrap();
    fs::write(&patch, "name: new\nextra: true\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("name: new"));
    assert!(content.contains("count: 1"));
    assert!(content.contains("extra: true"));
    fs::remove_dir_all(dir).unwrap();
}

// --- INI ---

#[test]
fn ini_round_trip() {
    let dir = tmpdir();
    let existing = dir.join("config.ini");
    let patch = dir.join("patch.ini");

    fs::write(&existing, "[section]\nfoo = bar\n").unwrap();
    fs::write(&patch, "[section]\nbaz = qux\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("foo=bar") || content.contains("foo = bar"));
    assert!(content.contains("baz=qux") || content.contains("baz = qux"));
    fs::remove_dir_all(dir).unwrap();
}

// --- version flag ---

#[test]
fn version_flag() {
    let output = patchix().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("patchix"));
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}
