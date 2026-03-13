use std::fs;
use std::path::Path;
use std::process::Command;

fn patchix() -> Command {
    Command::new(env!("CARGO_BIN_EXE_patchix"))
}

// --- JSON ---

#[test]
fn json_basic_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");
    let output = dir.path().join("output.json");

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
}

#[test]
fn json_creates_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("new.json");
    let patch = dir.path().join("patch.json");

    fs::write(&patch, r#"{"x": 42}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["x"], 42);
}

#[test]
fn json_creates_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("deep/nested/config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&patch, r#"{"key": "value"}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(existing.exists());
}

#[test]
fn json_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

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
}

#[test]
fn json_no_clobber_null_preserves_key() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"a": 1, "b": 2}"#).unwrap();
    fs::write(&patch, r#"{"b": null}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--no-clobber"])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["a"], 1);
    assert_eq!(result["b"], 2); // b is preserved, null did NOT delete it
}

#[test]
fn json_null_deletes_key() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

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
}

#[test]
fn json_array_append() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

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
}

// --- version flag ---

#[test]
fn version_flag() {
    let output = patchix().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("patchix"));
}

// --- Error paths ---

#[test]
fn error_missing_patch_file() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("nonexistent.json");

    fs::write(&existing, r#"{"a": 1}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn error_malformed_json() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"a": 1}"#).unwrap();
    fs::write(&patch, r#"{ invalid json }"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn error_unknown_extension_without_format() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.txt");
    let patch = dir.path().join("patch.txt");

    fs::write(&existing, "hello").unwrap();
    fs::write(&patch, "world").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(!status.success());
}

// --- --format flag ---

#[test]
fn format_flag_overrides_extension() {
    let dir = tempfile::tempdir().unwrap();
    // Use .txt extension with --format json to verify format override
    let existing = dir.path().join("config.txt");
    let patch = dir.path().join("patch.txt");

    fs::write(&existing, r#"{"a": 1}"#).unwrap();
    fs::write(&patch, r#"{"b": 2}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--format", "json"])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["a"], 1);
    assert_eq!(result["b"], 2);
}

// --- Array strategies ---

#[test]
fn json_default_array_append() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"items": [1, 2]}"#).unwrap();
    fs::write(&patch, r#"{"items": [3, 4]}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--default-array", "append",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["items"], serde_json::json!([1, 2, 3, 4]));
}

#[test]
fn json_default_array_prepend() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"items": [1, 2]}"#).unwrap();
    fs::write(&patch, r#"{"items": [3, 4]}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--default-array", "prepend",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["items"], serde_json::json!([3, 4, 1, 2]));
}

#[test]
fn json_default_array_union() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"items": [1, 2, 3]}"#).unwrap();
    fs::write(&patch, r#"{"items": [2, 3, 4]}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--default-array", "union",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["items"], serde_json::json!([1, 2, 3, 4]));
}

#[test]
fn json_nested_path_array_strategy() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"editor": {"formatters": ["rustfmt"]}}"#).unwrap();
    fs::write(&patch, r#"{"editor": {"formatters": ["prettier"]}}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--array-strategy", "editor.formatters=append",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["editor"]["formatters"], serde_json::json!(["rustfmt", "prettier"]));
}

#[test]
fn json_multiple_array_strategies() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"plugins": ["a"], "keybinds": ["x"]}"#).unwrap();
    fs::write(&patch, r#"{"plugins": ["b"], "keybinds": ["y"]}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--array-strategy", "plugins=append",
            "--array-strategy", "keybinds=prepend",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(result["plugins"], serde_json::json!(["a", "b"]));
    assert_eq!(result["keybinds"], serde_json::json!(["y", "x"]));
}

// --- TOML ---

#[test]
fn toml_basic_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.toml");
    let patch = dir.path().join("patch.toml");

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
}

#[test]
fn toml_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.toml");
    let patch = dir.path().join("patch.toml");

    fs::write(&existing, "name = \"old\"\ncount = 1\n").unwrap();
    fs::write(&patch, "name = \"new\"\nextra = true\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--no-clobber"])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("name = \"old\"")); // preserved
    assert!(content.contains("count = 1"));       // preserved
    assert!(content.contains("extra = true"));    // new key added
}

#[test]
fn toml_creates_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("new.toml");
    let patch = dir.path().join("patch.toml");

    fs::write(&patch, "key = \"value\"\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(existing.exists());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("key = \"value\""));
}

#[test]
fn toml_nested_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.toml");
    let patch = dir.path().join("patch.toml");

    fs::write(&existing, "[editor]\nfont_size = 12\n").unwrap();
    fs::write(&patch, "[editor]\ntheme = \"dark\"\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("font_size = 12"));
    assert!(content.contains("theme = \"dark\""));
}

#[test]
fn toml_error_malformed() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.toml");
    let patch = dir.path().join("patch.toml");

    fs::write(&existing, "name = \"ok\"\n").unwrap();
    fs::write(&patch, "this is [not valid toml !!!\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(!status.success());
}

// --- YAML ---

#[test]
fn yaml_basic_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

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
    // Output must not start with YAML document marker
    assert!(!content.starts_with("---"));
}

#[test]
fn yaml_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    fs::write(&existing, "name: old\ncount: 1\n").unwrap();
    fs::write(&patch, "name: new\nextra: true\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--no-clobber"])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("name: old")); // preserved
    assert!(content.contains("count: 1"));  // preserved
    assert!(content.contains("extra: true")); // new key added
}

#[test]
fn yaml_nested_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    fs::write(&existing, "editor:\n  font_size: 12\n").unwrap();
    fs::write(&patch, "editor:\n  theme: dark\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("font_size: 12"));
    assert!(content.contains("theme: dark"));
}

#[test]
fn yaml_creates_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("new.yaml");
    let patch = dir.path().join("patch.yaml");

    fs::write(&patch, "key: value\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(existing.exists());
}

#[test]
fn yaml_no_document_marker_in_output() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    // Input file does not have --- marker
    fs::write(&existing, "a: 1\n").unwrap();
    fs::write(&patch, "b: 2\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(!content.starts_with("---"), "Output should not start with YAML --- marker, got: {content}");
}

#[test]
fn yaml_input_with_document_marker() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    // Input has --- document marker
    fs::write(&existing, "---\na: 1\nb: 2\n").unwrap();
    fs::write(&patch, "c: 3\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(!content.starts_with("---"), "Output should not have --- marker");
    assert!(content.contains("a: 1"));
    assert!(content.contains("b: 2"));
    assert!(content.contains("c: 3"));
}

// --- INI ---

#[test]
fn ini_basic_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.ini");
    let patch = dir.path().join("patch.ini");

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
}

#[test]
fn ini_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.ini");
    let patch = dir.path().join("patch.ini");

    fs::write(&existing, "[section]\nfoo = bar\n").unwrap();
    fs::write(&patch, "[section]\nfoo = changed\nnew_key = value\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--no-clobber"])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // foo should be preserved
    assert!(content.contains("foo=bar") || content.contains("foo = bar"));
    // new key should be added
    assert!(content.contains("new_key=value") || content.contains("new_key = value"));
}

#[test]
fn ini_creates_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("new.ini");
    let patch = dir.path().join("patch.ini");

    fs::write(&patch, "[section]\nkey = value\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(existing.exists());
}

#[test]
fn ini_multiple_sections() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.ini");
    let patch = dir.path().join("patch.ini");

    fs::write(&existing, "[a]\nkey1 = v1\n\n[b]\nkey2 = v2\n").unwrap();
    fs::write(&patch, "[a]\nnew = added\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("key1=v1") || content.contains("key1 = v1"));
    assert!(content.contains("key2=v2") || content.contains("key2 = v2"));
    assert!(content.contains("new=added") || content.contains("new = added"));
}

#[test]
fn ini_error_malformed() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.ini");
    let patch = dir.path().join("patch.ini");

    fs::write(&existing, "[section]\nkey = value\n").unwrap();
    fs::write(&patch, "this is not = valid\n[[broken\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    // Should fail gracefully
    assert!(!status.success());
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}
