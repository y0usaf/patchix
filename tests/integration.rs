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
        .args([
            "merge",
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "-o",
            s(&output),
        ])
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
    assert_eq!(result["a"], 1); // preserved
    assert_eq!(result["b"], 2); // preserved
    assert_eq!(result["c"], 3); // new key filled in
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
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--array-strategy",
            "plugins=append",
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
        .args([
            "merge",
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--format",
            "json",
        ])
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
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--default-array",
            "append",
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
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--default-array",
            "prepend",
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
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--default-array",
            "union",
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
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--array-strategy",
            "editor.formatters=append",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(
        result["editor"]["formatters"],
        serde_json::json!(["rustfmt", "prettier"])
    );
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
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--array-strategy",
            "plugins=append",
            "--array-strategy",
            "keybinds=prepend",
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
    assert!(content.contains("count = 1")); // preserved
    assert!(content.contains("extra = true")); // new key added
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
    assert!(content.contains("count: 1")); // preserved
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
    assert!(
        !content.starts_with("---"),
        "Output should not start with YAML --- marker, got: {content}"
    );
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
    assert!(
        !content.starts_with("---"),
        "Output should not have --- marker"
    );
    assert!(content.contains("a: 1"));
    assert!(content.contains("b: 2"));
    assert!(content.contains("c: 3"));
}

// --- YAML array strategies ---

#[test]
fn yaml_array_append() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    fs::write(&existing, "plugins:\n  - a\n  - b\n").unwrap();
    fs::write(&patch, "plugins:\n  - c\n").unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--default-array",
            "append",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    let result: serde_json::Value = serde_yml::from_str(&content).unwrap();
    assert_eq!(result["plugins"], serde_json::json!(["a", "b", "c"]));
}

#[test]
fn yaml_array_union() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    fs::write(&existing, "items:\n  - 1\n  - 2\n  - 3\n").unwrap();
    fs::write(&patch, "items:\n  - 2\n  - 3\n  - 4\n").unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--default-array",
            "union",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    let result: serde_json::Value = serde_yml::from_str(&content).unwrap();
    assert_eq!(result["items"], serde_json::json!([1, 2, 3, 4]));
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

// --- REG ---

#[test]
fn reg_basic_merge() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("user.reg");
    let patch = dir.path().join("patch.reg");

    fs::write(
        &existing,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
         \"UseGLSL\"=\"disabled\"\r\n",
    )
    .unwrap();
    fs::write(
        &patch,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
         \"UseGLSL\"=\"enabled\"\r\n\
         \"VideoMemorySize\"=dword:00000200\r\n",
    )
    .unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("Windows Registry Editor Version 5.00"));
    assert!(content.contains("\"UseGLSL\"=\"enabled\""));
    assert!(content.contains("\"VideoMemorySize\"=dword:00000200"));
}

#[test]
fn reg_no_clobber_preserves_existing_values() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("user.reg");
    let patch = dir.path().join("patch.reg");

    fs::write(
        &existing,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
         \"UseGLSL\"=\"disabled\"\r\n",
    )
    .unwrap();
    fs::write(
        &patch,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
         \"UseGLSL\"=\"enabled\"\r\n\
         \"StrictDrawOrdering\"=\"enabled\"\r\n",
    )
    .unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "--no-clobber"])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("\"UseGLSL\"=\"disabled\""));
    assert!(content.contains("\"StrictDrawOrdering\"=\"enabled\""));
}

#[test]
fn reg_accepts_json_patch_format() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("user.reg");
    let patch = dir.path().join("patch.json");

    fs::write(
        &existing,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
         \"UseGLSL\"=\"disabled\"\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Wine\\ObsoleteKey]\r\n\
         \"Keep\"=\"old\"\r\n",
    )
    .unwrap();
    fs::write(
        &patch,
        r#"{
  "HKEY_CURRENT_USER\\Software\\Wine\\Direct3D": {
    "UseGLSL": {"type": "sz", "value": "enabled"},
    "VideoMemorySize": {"type": "dword", "value": 512}
  },
  "HKEY_CURRENT_USER\\Software\\Wine\\ObsoleteKey": null
}"#,
    )
    .unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e",
            s(&existing),
            "-p",
            s(&patch),
            "--format",
            "reg",
            "--patch-format",
            "json",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("\"UseGLSL\"=\"enabled\""));
    assert!(content.contains("\"VideoMemorySize\"=dword:00000200"));
    assert!(!content.contains("ObsoleteKey"));
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

// ────────────────────────────────────────────────────────────────────────────
// Test 1 — --output flag: existing file must be unchanged
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn output_flag_does_not_modify_existing() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");
    let output = dir.path().join("result.json");

    let original = r#"{"a": 1}"#;
    fs::write(&existing, original).unwrap();
    fs::write(&patch, r#"{"b": 2}"#).unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch), "-o", s(&output)])
        .status()
        .unwrap();
    assert!(status.success());

    // Existing file must be byte-for-byte identical to original
    assert_eq!(
        fs::read_to_string(&existing).unwrap(),
        original,
        "existing file was modified even though --output was specified"
    );

    // Output file must contain the merge result
    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(result["a"], 1);
    assert_eq!(result["b"], 2);
}

// ────────────────────────────────────────────────────────────────────────────
// Test 2 — Error paths produce useful stderr
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn error_missing_patch_file_has_useful_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("nonexistent_patch.json");

    fs::write(&existing, r#"{"a": 1}"#).unwrap();

    let output = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .output()
        .unwrap();

    assert!(!output.status.success());
    // anyhow writes errors to stderr via the default main() handler
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "stderr should not be empty on error"
    );
    assert!(
        stderr.contains("nonexistent_patch.json"),
        "stderr should name the missing file, got: {stderr}"
    );
}

#[test]
fn error_malformed_json_has_useful_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"a": 1}"#).unwrap();
    fs::write(&patch, r#"{ not valid json }"#).unwrap();

    let output = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "stderr should not be empty on parse error");
    assert!(
        stderr.contains("parsing") || stderr.contains("patch.json") || stderr.contains("error"),
        "stderr should contain useful parse error context, got: {stderr}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 3 — --no-clobber + --array-strategy (document that strategy is ignored)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn no_clobber_ignores_array_strategy() {
    // Under --no-clobber, existing arrays are preserved regardless of any
    // --array-strategy setting. This documents the current behavior.
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
            "--no-clobber",
            "--array-strategy", "plugins=append",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let result: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&existing).unwrap()).unwrap();
    // Array is preserved (no-clobber wins over append strategy)
    assert_eq!(result["plugins"], serde_json::json!(["a", "b"]));
}

// ────────────────────────────────────────────────────────────────────────────
// Test 4 — YAML implicit type coercion (document actual behavior)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn yaml_implicit_bool_coercion_documented() {
    // serde_yml (0.0.12) does NOT coerce bare 'yes'/'no' to booleans — it
    // preserves them as quoted strings ('yes', 'no') in the output.
    // This differs from older serde_yaml (YAML 1.1) behavior.
    // This test documents the actual behavior of the serde_yml version in use.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    // Start with a file that has no yes/no keys
    fs::write(&existing, "name: test\n").unwrap();
    // Patch adds a bare 'yes' value
    fs::write(&patch, "enabled: yes\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // With serde_yml 0.0.12, bare 'yes' is preserved as the string "yes",
    // NOT coerced to boolean true. If this test starts failing after a
    // serde_yml upgrade, the library's YAML 1.1 boolean coercion may have changed.
    assert!(
        content.contains("enabled: 'yes'") || content.contains("enabled: yes"),
        "expected bare 'yes' to be preserved as a string (not coerced to boolean), got: {content}"
    );
}

#[test]
fn yaml_quoted_yes_preserved_as_string() {
    // Quoted 'yes' is NOT coerced — it stays as a string.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.yaml");
    let patch = dir.path().join("patch.yaml");

    fs::write(&existing, "name: test\n").unwrap();
    fs::write(&patch, "restart: \"no\"\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(
        content.contains("restart: 'no'") || content.contains("restart: \"no\"") || content.contains("restart: no"),
        "quoted 'no' should be preserved as a string, got: {content}"
    );
    // Verify it was NOT coerced to boolean false
    assert!(
        !content.contains("restart: false"),
        "quoted 'no' should not be coerced to boolean false, got: {content}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 5 — TOML datetime behavior (document the known limitation)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn toml_datetime_becomes_string_after_merge_known_limitation() {
    // KNOWN LIMITATION: TOML datetimes are converted to strings during the
    // JSON intermediate representation and cannot be round-tripped as native
    // TOML datetime types. After a merge, a datetime like 2024-01-15T12:00:00Z
    // becomes the string "2024-01-15T12:00:00Z" in TOML output.
    // See: src/formats/toml.rs toml_to_json (Datetime → String conversion)
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.toml");
    let patch = dir.path().join("patch.toml");

    fs::write(
        &existing,
        "name = \"old\"\ntimestamp = 2024-01-15T12:00:00Z\n",
    )
    .unwrap();
    fs::write(&patch, "name = \"new\"\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // name was updated
    assert!(content.contains("name = \"new\""), "patch was not applied: {content}");
    // timestamp is present but may have been converted to a quoted string
    // (the datetime type is lost after round-trip through JSON)
    assert!(
        content.contains("2024-01-15"),
        "timestamp value was lost entirely: {content}"
    );
    // The actual limitation: the datetime is now a quoted TOML string, not a native datetime
    assert!(
        content.contains("timestamp = \"2024-01-15"),
        "expected datetime to become a quoted string (known limitation), \
         but it may still be a native TOML datetime: {content}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 6 — INI global (sectionless) keys
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn ini_global_section_keys_preserved() {
    // Keys before any [section] header in INI files are "global" (sectionless).
    // patchix maps them to the __global__ section in its JSON representation.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.ini");
    let patch = dir.path().join("patch.ini");

    fs::write(&existing, "global_key = global_val\n[section]\nfoo = bar\n").unwrap();
    fs::write(&patch, "[section]\nbaz = qux\n").unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // Global key must be preserved
    assert!(
        content.contains("global_key") && content.contains("global_val"),
        "global (sectionless) key was lost: {content}"
    );
    // Section key was added
    assert!(
        content.contains("baz") && content.contains("qux"),
        "patched section key missing: {content}"
    );
}

#[test]
fn ini_patch_global_section() {
    // The Nix module uses __global__ to represent sectionless keys in patches.
    // Verify that patching a file with a __global__ JSON key works correctly.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.ini");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, "[colors]\nred = ff0000\n").unwrap();
    // JSON patch using __global__ to add a sectionless key
    fs::write(&patch, r#"{"__global__": {"font": "monospace"}}"#).unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
            "--format", "ini",
            "--patch-format", "json",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(
        content.contains("font") && content.contains("monospace"),
        "patched global key missing from INI output: {content}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Test 7 — REG empty existing file creates new registry file
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn reg_creates_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("new.reg");
    let patch = dir.path().join("patch.reg");

    fs::write(
        &patch,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\NewApp]\r\n\
         \"Setting\"=\"value\"\r\n",
    )
    .unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(existing.exists());

    let content = fs::read_to_string(&existing).unwrap();
    assert!(content.contains("Windows Registry Editor Version 5.00"));
    assert!(content.contains("\"Setting\"=\"value\""));
}

// ────────────────────────────────────────────────────────────────────────────
// Test 8 — Null JSON patch is rejected
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn error_null_patch_rejected() {
    // A JSON patch containing literal `null` (not an empty object `{}`) is
    // rejected by the CLI because there is nothing to merge. An empty object
    // `{}` is valid and produces a no-op merge.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("config.json");
    let patch = dir.path().join("patch.json");

    fs::write(&existing, r#"{"a": 1}"#).unwrap();
    fs::write(&patch, "null").unwrap();

    let output = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "should produce an error message for null patch"
    );
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr_str.contains("null") || stderr_str.contains("nothing to merge") || stderr_str.contains("empty"),
        "error should mention null/empty patch, got: {stderr_str}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// REG format-sensitive integration tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn reg_wine_header_preserved_after_cli_merge() {
    // Wine header must survive a merge — the output must still be a valid
    // Wine registry file, not downgraded to Windows Registry Editor v5.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("user.reg");
    let patch = dir.path().join("patch.reg");

    fs::write(
        &existing,
        "WINE REGISTRY Version 2\n\n\
         [Software\\\\Wine]\n\
         \"Version\"=\"6.0\"\n",
    )
    .unwrap();
    fs::write(
        &patch,
        "WINE REGISTRY Version 2\n\n\
         [Software\\\\Wine\\\\Direct3D]\n\
         \"UseGLSL\"=\"enabled\"\n",
    )
    .unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // Header must be Wine, not v5
    assert!(
        content.starts_with("WINE REGISTRY Version 2"),
        "Wine header was not preserved after merge, got: {}",
        &content[..60.min(content.len())]
    );
    // Internal __header__ key must not leak into output
    assert!(
        !content.contains("__header__"),
        "internal __header__ key leaked into output"
    );
    // Original value preserved
    assert!(content.contains("\"Version\"=\"6.0\""));
    // Patch applied
    assert!(content.contains("\"UseGLSL\"=\"enabled\""));
}

#[test]
fn reg_v5_uses_double_backslash_headers() {
    // Standard v5 .reg files must use double-backslash in section headers.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("test.reg");
    let patch = dir.path().join("patch.reg");

    fs::write(
        &existing,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Test]\r\n\
         \"Old\"=\"value\"\r\n",
    )
    .unwrap();
    fs::write(
        &patch,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Test]\r\n\
         \"New\"=\"value\"\r\n",
    )
    .unwrap();

    let status = patchix()
        .args(["merge", "-e", s(&existing), "-p", s(&patch)])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // v5 format: section header must have double backslashes
    assert!(
        content.contains("[HKEY_CURRENT_USER\\\\Software\\\\Test]"),
        "v5 section header should use double backslashes, got: {content}"
    );
}

#[test]
fn reg_cross_format_json_patch_strips_metadata() {
    // When patching a .reg file with a REG-format patch, internal metadata
    // keys (__header__, __preamble__) are stripped so they don't overwrite
    // the existing file's header.  Use --patch-format reg to trigger this.
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("user.reg");
    let patch = dir.path().join("patch.reg");

    fs::write(
        &existing,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Test]\r\n\
         \"Old\"=\"value\"\r\n",
    )
    .unwrap();
    // Patch is also a v5 reg file — its __header__ is stripped during merge
    // so the existing file's header is preserved.
    fs::write(
        &patch,
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\Test]\r\n\
         \"New\"=\"value\"\r\n",
    )
    .unwrap();

    let status = patchix()
        .args([
            "merge",
            "-e", s(&existing),
            "-p", s(&patch),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let content = fs::read_to_string(&existing).unwrap();
    // Original v5 header preserved
    assert!(content.starts_with("Windows Registry Editor Version 5.00"));
    assert!(!content.contains("__header__"));
    // Patch value applied
    assert!(content.contains("\"New\"=\"value\""));
    // Original value preserved
    assert!(content.contains("\"Old\"=\"value\""));
}


