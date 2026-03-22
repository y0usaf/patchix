/// Wine/Windows Registry (.reg) format support.
///
/// JSON representation:
/// ```json
/// {
///   "HKEY_CURRENT_USER\\Software\\Wine": {
///     "StringVal": {"type": "sz", "value": "hello"},
///     "DwordVal":  {"type": "dword", "value": 255},
///     "BinaryVal": {"type": "hex", "value": "01,02,03"},
///     "ExpandVal": {"type": "expand_sz", "value": "%SystemRoot%\\foo"},
///     "MultiVal":  {"type": "multi_sz", "value": ["line1", "line2"]},
///     "(default)": {"type": "sz", "value": "default string"}
///   }
/// }
/// ```
///
/// A `null` value for a key in the JSON patch means "delete this value" (emits `-"ValueName"`).
/// A `null` for a section key means "delete this key" (emits `[-HKEY_...]`).
///
/// Value type tags:
/// - `"sz"`        → `"value"` (plain string)
/// - `"expand_sz"` → `"value"` (expandable string, e.g. `%PATH%`)
/// - `"multi_sz"`  → `"value"` (array of strings)
/// - `"dword"`     → `"value"` (integer 0..=0xFFFF_FFFF)
/// - `"qword"`     → `"value"` (integer, serialised as hex:8 bytes LE)
/// - `"hex"`       → `"value"` (raw hex string `"01,02,ff"`)
/// - `"hex(N)"`    → `"value"` (raw hex for REG_* type N, pass-through)
use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Escape a string for emission inside `"…"` in a .reg file.
fn escape_reg_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out
}

/// Strip a BOM (U+FEFF) from the front of a string if present.
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

// ── parse ─────────────────────────────────────────────────────────────────────

pub fn parse(input: &str) -> Result<Value> {
    let input = strip_bom(input);
    let mut lines = input.lines().peekable();

    // First non-empty line must be the header
    let header = lines
        .find(|l| !l.trim().is_empty())
        .context("empty .reg file")?
        .trim();

    if header != "Windows Registry Editor Version 5.00"
        && header != "REGEDIT4"
        && header != "WINE REGISTRY Version 2"
    {
        bail!(
            "unrecognised .reg header: '{}'; expected \
             'Windows Registry Editor Version 5.00', 'REGEDIT4', or 'WINE REGISTRY Version 2'",
            header
        );
    }

    let mut root: Map<String, Value> = Map::new();
    let mut current_key: Option<String> = None;

    // Collect logical lines (backslash-continuation)
    let mut logical = String::new();

    let flush_logical = |logical: &mut String,
                         current_key: &Option<String>,
                         root: &mut Map<String, Value>|
     -> Result<()> {
        let trimmed = logical.trim().to_string();
        *logical = String::new();
        if trimmed.is_empty() {
            return Ok(());
        }
        let key = match current_key {
            Some(k) => k,
            None => return Ok(()), // value line before any section header — skip
        };
        let section = root
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let section_map = section
            .as_object_mut()
            .context("section must be an object")?;
        parse_value_line(&trimmed, section_map)
    };

    for raw_line in lines {
        // Strip inline comments only outside of quoted contexts — but .reg
        // comments are full-line (;), so we just check the start.
        let line = raw_line.trim_end();

        // Comment line or Wine metadata line (#time=..., #arch=..., etc.)
        if line.trim_start().starts_with(';') || line.trim_start().starts_with('#') {
            // flush any pending logical line first
            flush_logical(&mut logical, &current_key, &mut root)?;
            continue;
        }

        // Empty line: flush pending
        if line.trim().is_empty() {
            flush_logical(&mut logical, &current_key, &mut root)?;
            continue;
        }

        // Continuation line
        if logical.ends_with('\\') {
            logical.pop(); // remove trailing backslash
            logical.push_str(line.trim_start());
            continue;
        }

        // Flush any previously accumulated logical line
        flush_logical(&mut logical, &current_key, &mut root)?;

        let trimmed = line.trim();

        // Section header: [KEY] or [-KEY] (delete key)
        // Wine format may append a timestamp: [KEY] 1234567890
        if trimmed.starts_with('[') {
            // Find the closing bracket
            if let Some(close) = trimmed.find(']') {
                let inner = &trimmed[1..close];
                if inner.starts_with('-') {
                    // Key deletion marker — store as null
                    let key = inner[1..].to_string();
                    root.insert(key, Value::Null);
                    current_key = None;
                } else {
                    let key = inner.to_string();
                    root.entry(key.clone())
                        .or_insert_with(|| Value::Object(Map::new()));
                    current_key = Some(key);
                }
            } else {
                bail!("malformed section header: '{}'", trimmed);
            }
            continue;
        }

        // Otherwise accumulate as a logical line
        logical.push_str(trimmed);
    }

    // Flush final logical line
    flush_logical(&mut logical, &current_key, &mut root)?;

    Ok(Value::Object(root))
}

/// Parse a single (fully-joined) value assignment line into `section_map`.
fn parse_value_line(line: &str, section_map: &mut Map<String, Value>) -> Result<()> {
    // Value deletion: -"ValueName"
    if let Some(rest) = line.strip_prefix('-') {
        let name = parse_value_name(rest)
            .with_context(|| format!("malformed value deletion: '{line}'"))?;
        section_map.insert(name, Value::Null);
        return Ok(());
    }

    // Normal: "ValueName"=<data>  or  @=<data>  (default value)
    let (name, data) =
        split_name_data(line).with_context(|| format!("malformed value line: '{line}'"))?;

    let entry =
        parse_data(data).with_context(|| format!("malformed data for value '{name}': '{data}'"))?;

    section_map.insert(name, entry);
    Ok(())
}

/// Extract the value name from a quoted string (or `@` for default).
fn parse_value_name(s: &str) -> Result<String> {
    let s = s.trim();
    if s == "@" {
        return Ok("(default)".to_string());
    }
    if !s.starts_with('"') {
        bail!("value name must be quoted or '@', got: '{s}'");
    }
    let (name, _rest) = extract_quoted(s)?;
    Ok(name)
}

/// Split `"ValueName"=data` → `(name, data)`.
fn split_name_data(line: &str) -> Result<(String, &str)> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix('@') {
        // @=data  (default value)
        let data = rest.strip_prefix('=').context("expected '=' after '@'")?;
        return Ok(("(default)".to_string(), data.trim_start()));
    }

    if !line.starts_with('"') {
        bail!("expected quoted value name or '@'");
    }
    let (name, rest) = extract_quoted(line)?;
    let rest = rest.trim_start();
    let data = rest
        .strip_prefix('=')
        .context("expected '=' after value name")?;
    Ok((name, data.trim_start()))
}

/// Extract a `"…"`-quoted string from the start of `s`, returning
/// `(unescaped_content, remainder_after_closing_quote)`.
fn extract_quoted(s: &str) -> Result<(String, &str)> {
    debug_assert!(s.starts_with('"'));
    let s = &s[1..]; // skip opening quote
    let mut out = String::new();
    let mut chars = s.char_indices().peekable();
    loop {
        match chars.next() {
            None => bail!("unterminated quoted string"),
            Some((_, '\\')) => match chars.next() {
                Some((_, '\\')) => out.push('\\'),
                Some((_, '"')) => out.push('"'),
                Some((_, c)) => {
                    out.push('\\');
                    out.push(c);
                }
                None => bail!("unterminated escape sequence"),
            },
            Some((i, '"')) => {
                // closing quote — return remainder
                let remainder = &s[i + 1..];
                return Ok((out, remainder));
            }
            Some((_, c)) => out.push(c),
        }
    }
}

/// Parse the data portion of a value assignment (everything after `=`).
fn parse_data(data: &str) -> Result<Value> {
    let data = data.trim();

    if data.starts_with('"') {
        // REG_SZ
        let (s, _) = extract_quoted(data)?;
        return Ok(make_entry("sz", Value::String(s)));
    }

    if let Some(hex) = data.strip_prefix("dword:") {
        let hex = hex.trim();
        let n = u32::from_str_radix(hex, 16)
            .with_context(|| format!("invalid dword value: '{hex}'"))?;
        return Ok(make_entry("dword", Value::Number(n.into())));
    }

    if data.starts_with("qword:") || data.starts_with("hex(b):") {
        // QWORD stored as 8-byte little-endian hex
        let hex_data = if let Some(hex_data) = data.strip_prefix("qword:") {
            hex_data.trim()
        } else {
            data["hex(b):".len()..].trim()
        };
        // Store as raw hex string — round-trip safe
        return Ok(make_entry("qword", Value::String(hex_data.to_string())));
    }

    if let Some(hex_data) = data.strip_prefix("hex(2):") {
        // REG_EXPAND_SZ — NUL-terminated UTF-16LE
        let hex_data = hex_data.trim();
        let s = decode_utf16le_hex(hex_data).context("decoding REG_EXPAND_SZ (hex(2))")?;
        return Ok(make_entry("expand_sz", Value::String(s)));
    }

    if let Some(hex_data) = data.strip_prefix("hex(7):") {
        // REG_MULTI_SZ — double-NUL-terminated UTF-16LE
        let hex_data = hex_data.trim();
        let s = decode_utf16le_hex(hex_data).context("decoding REG_MULTI_SZ (hex(7))")?;
        // Split on inner NUL; drop trailing empty strings
        let parts: Vec<Value> = s
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(|p| Value::String(p.to_string()))
            .collect();
        return Ok(make_entry("multi_sz", Value::Array(parts)));
    }

    // Generic hex(N) or plain hex:
    if data.starts_with("hex") {
        let colon = data.find(':').context("expected ':' after hex type")?;
        let type_tag = &data[..colon]; // "hex" or "hex(N)"
        let hex_data = data[colon + 1..].trim();
        let tag = match type_tag {
            "hex" => "hex".to_string(),
            other => other.to_string(), // "hex(3)", "hex(4)", …
        };
        return Ok(make_typed_entry(&tag, Value::String(hex_data.to_string())));
    }

    // Wine-specific: str(N):"..." — human-readable encoding for string types
    // str(2) = REG_EXPAND_SZ, str(7) = REG_MULTI_SZ, etc.
    if let Some(rest) = data.strip_prefix("str(") {
        if let Some(close) = rest.find("):") {
            let n: u32 = rest[..close]
                .parse()
                .with_context(|| format!("invalid str(N) type: '{data}'"))?;
            let quoted = &rest[close + 2..];
            let (s, _) = extract_quoted(quoted)
                .with_context(|| format!("malformed str(N) value: '{data}'"))?;
            let type_tag = match n {
                1 => "sz",
                2 => "expand_sz",
                7 => "multi_sz",
                _ => "sz", // fallback: treat unknown str(N) as plain string
            };
            if n == 7 {
                // Multi-string: split on \0
                let parts: Vec<Value> = s
                    .split('\0')
                    .filter(|p| !p.is_empty())
                    .map(|p| Value::String(p.to_string()))
                    .collect();
                return Ok(make_entry(type_tag, Value::Array(parts)));
            }
            return Ok(make_entry(type_tag, Value::String(s)));
        }
    }

    bail!("unrecognised data format: '{data}'");
}

fn make_entry(type_tag: &str, value: Value) -> Value {
    let mut m = Map::new();
    m.insert("type".to_string(), Value::String(type_tag.to_string()));
    m.insert("value".to_string(), value);
    Value::Object(m)
}

fn make_typed_entry(type_tag: &str, value: Value) -> Value {
    make_entry(type_tag, value)
}

/// Decode a hex string like `"68,00,65,00,6c,00,6c,00,6f,00,00,00"` as UTF-16LE.
fn decode_utf16le_hex(hex: &str) -> Result<String> {
    let bytes = parse_hex_bytes(hex)?;
    if bytes.len() % 2 != 0 {
        bail!("UTF-16LE hex byte count must be even, got {}", bytes.len());
    }
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    String::from_utf16(&u16s).context("invalid UTF-16LE sequence")
}

/// Parse a comma-separated hex byte string (with optional line-continuation whitespace).
fn parse_hex_bytes(hex: &str) -> Result<Vec<u8>> {
    hex.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(i, s)| {
            u8::from_str_radix(s, 16).with_context(|| format!("invalid hex byte #{i}: '{s}'"))
        })
        .collect()
}

// ── serialize ─────────────────────────────────────────────────────────────────

pub fn serialize(value: &Value) -> Result<String> {
    let root = value.as_object().context("REG root must be an object")?;

    let mut out = String::from("Windows Registry Editor Version 5.00\r\n");

    for (section, section_val) in root {
        out.push_str("\r\n");

        if section_val.is_null() {
            // Key deletion
            out.push('[');
            out.push('-');
            out.push_str(section);
            out.push(']');
            out.push_str("\r\n");
            continue;
        }

        let props = section_val
            .as_object()
            .with_context(|| format!("REG section '{section}' must be an object or null"))?;

        out.push('[');
        out.push_str(section);
        out.push(']');
        out.push_str("\r\n");

        for (name, entry) in props {
            if entry.is_null() {
                // Value deletion
                out.push('-');
                out.push_str(&quote_value_name(name));
                out.push_str("\r\n");
                continue;
            }

            let entry_obj = entry.as_object().with_context(|| {
                format!("entry '{name}' must be an object {{type, value}} or null")
            })?;

            let type_tag = entry_obj
                .get("type")
                .and_then(Value::as_str)
                .with_context(|| format!("entry '{name}' missing 'type' field"))?;

            let val = entry_obj
                .get("value")
                .with_context(|| format!("entry '{name}' missing 'value' field"))?;

            let name_part = quote_value_name(name);
            let data_part = serialize_data(type_tag, val, name)
                .with_context(|| format!("serialising entry '{name}'"))?;

            out.push_str(&name_part);
            out.push('=');
            out.push_str(&data_part);
            out.push_str("\r\n");
        }
    }

    // Ensure trailing newline
    if !out.ends_with("\r\n") {
        out.push_str("\r\n");
    }

    Ok(out)
}

fn quote_value_name(name: &str) -> String {
    if name == "(default)" {
        return "@".to_string();
    }
    format!("\"{}\"", escape_reg_string(name))
}

fn serialize_data(type_tag: &str, val: &Value, name: &str) -> Result<String> {
    match type_tag {
        "sz" => {
            let s = val
                .as_str()
                .with_context(|| format!("sz value for '{name}' must be a string"))?;
            Ok(format!("\"{}\"", escape_reg_string(s)))
        }

        "expand_sz" => {
            let s = val
                .as_str()
                .with_context(|| format!("expand_sz value for '{name}' must be a string"))?;
            // Encode as UTF-16LE hex
            let hex = encode_utf16le_hex(s);
            Ok(format!("hex(2):{}", fold_hex(&hex)))
        }

        "multi_sz" => {
            let arr = val
                .as_array()
                .with_context(|| format!("multi_sz value for '{name}' must be an array"))?;
            let mut combined = String::new();
            for item in arr {
                let s = item
                    .as_str()
                    .with_context(|| format!("multi_sz items for '{name}' must be strings"))?;
                combined.push_str(s);
                combined.push('\0');
            }
            combined.push('\0'); // double-NUL terminator
            let hex = encode_utf16le_hex(&combined);
            Ok(format!("hex(7):{}", fold_hex(&hex)))
        }

        "dword" => {
            let n = val
                .as_u64()
                .with_context(|| format!("dword value for '{name}' must be an integer"))?;
            if n > u32::MAX as u64 {
                bail!("dword value for '{name}' is out of range: {n}");
            }
            Ok(format!("dword:{:08x}", n as u32))
        }

        "qword" => {
            // Stored as raw hex string from parse, or as u64 integer
            match val {
                Value::String(s) => Ok(format!("hex(b):{}", fold_hex(s))),
                Value::Number(n) => {
                    let v = n
                        .as_u64()
                        .with_context(|| format!("qword value for '{name}' must fit in u64"))?;
                    let bytes = v.to_le_bytes();
                    let hex = bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    Ok(format!("hex(b):{}", fold_hex(&hex)))
                }
                _ => bail!("qword value for '{name}' must be a string or number"),
            }
        }

        "hex" => {
            let s = val
                .as_str()
                .with_context(|| format!("hex value for '{name}' must be a string"))?;
            Ok(format!("hex:{}", fold_hex(s)))
        }

        other if other.starts_with("hex(") => {
            let s = val
                .as_str()
                .with_context(|| format!("{other} value for '{name}' must be a string"))?;
            Ok(format!("{}:{}", other, fold_hex(s)))
        }

        other => bail!("unknown value type '{other}' for '{name}'"),
    }
}

/// Encode a Rust `&str` as a UTF-16LE comma-separated hex byte string.
fn encode_utf16le_hex(s: &str) -> String {
    s.encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Fold a long hex string to 76-char line width (regedit style).
/// Lines beyond the first are indented with two spaces.
fn fold_hex(hex: &str) -> String {
    // Each byte is "xx," = 3 chars; 76 chars per line ≈ 25 bytes/line
    // We split on commas to avoid breaking a byte in the middle.
    if hex.is_empty() {
        return String::new();
    }
    let bytes: Vec<&str> = hex.split(',').collect();
    // ~25 bytes per line keeps lines under 80 chars when indented
    const BYTES_PER_LINE: usize = 25;
    let mut lines: Vec<String> = Vec::new();
    for chunk in bytes.chunks(BYTES_PER_LINE) {
        lines.push(chunk.join(","));
    }
    if lines.len() == 1 {
        return lines.remove(0);
    }
    // Join with ",\\\r\n  " continuation
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push_str(",\\\r\n  ");
        }
        out.push_str(line);
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Round-trip helper: parse → serialize → parse, check both intermediate
    /// and final values equal the expected JSON.
    fn round_trip(reg: &str) -> Result<Value> {
        let parsed = parse(reg)?;
        let serialised = serialize(&parsed)?;
        let reparsed = parse(&serialised)?;
        assert_eq!(parsed, reparsed, "round-trip mismatch");
        Ok(parsed)
    }

    fn make_sz(v: &str) -> Value {
        json!({"type": "sz", "value": v})
    }
    fn make_dword(v: u32) -> Value {
        json!({"type": "dword", "value": v})
    }
    fn make_hex(v: &str) -> Value {
        json!({"type": "hex", "value": v})
    }
    fn make_multi(v: Vec<&str>) -> Value {
        json!({"type": "multi_sz", "value": v})
    }

    // ── header ────────────────────────────────────────────────────────────────

    #[test]
    fn accepts_v5_header() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Test]\r\n";
        let v = parse(reg).unwrap();
        assert!(v["HKEY_CURRENT_USER\\Test"].is_object());
    }

    #[test]
    fn accepts_regedit4_header() {
        let reg = "REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Test]\r\n";
        let v = parse(reg).unwrap();
        assert!(v["HKEY_CURRENT_USER\\Test"].is_object());
    }

    #[test]
    fn rejects_unknown_header() {
        let reg = "Some Random Header\r\n\r\n[HKEY_CURRENT_USER\\Test]\r\n";
        assert!(parse(reg).is_err());
    }

    #[test]
    fn rejects_empty_file() {
        assert!(parse("").is_err());
        assert!(parse("   \r\n  ").is_err());
    }

    // ── string values ─────────────────────────────────────────────────────────

    #[test]
    fn parses_sz_value() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Wine]\r\n\
                   \"UseGLSL\"=\"enabled\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Wine"]["UseGLSL"], make_sz("enabled"));
    }

    #[test]
    fn parses_sz_with_escaped_backslash() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Path\"=\"C:\\\\Windows\\\\System32\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(
            v["HKEY_CURRENT_USER\\Test"]["Path"],
            make_sz("C:\\Windows\\System32")
        );
    }

    #[test]
    fn parses_sz_with_escaped_quote() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Msg\"=\"say \\\"hello\\\"\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(
            v["HKEY_CURRENT_USER\\Test"]["Msg"],
            make_sz("say \"hello\"")
        );
    }

    #[test]
    fn parses_empty_sz() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Empty\"=\"\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Test"]["Empty"], make_sz(""));
    }

    #[test]
    fn parses_default_value() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   @=\"default string\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(
            v["HKEY_CURRENT_USER\\Test"]["(default)"],
            make_sz("default string")
        );
    }

    // ── dword ─────────────────────────────────────────────────────────────────

    #[test]
    fn parses_dword() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Version\"=dword:00000001\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Test"]["Version"], make_dword(1));
    }

    #[test]
    fn parses_dword_max() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Max\"=dword:ffffffff\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Test"]["Max"], make_dword(0xFFFF_FFFF));
    }

    #[test]
    fn parses_dword_zero() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Flag\"=dword:00000000\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Test"]["Flag"], make_dword(0));
    }

    #[test]
    fn serializes_dword_zero_padded() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "Flag": {"type": "dword", "value": 255}
            }
        });
        let s = serialize(&v).unwrap();
        assert!(s.contains("\"Flag\"=dword:000000ff"), "got: {s}");
    }

    // ── hex ───────────────────────────────────────────────────────────────────

    #[test]
    fn parses_hex_binary() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Data\"=hex:01,02,03,ff\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(
            v["HKEY_CURRENT_USER\\Test"]["Data"],
            make_hex("01,02,03,ff")
        );
    }

    #[test]
    fn parses_hex_empty() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   \"Empty\"=hex:\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Test"]["Empty"], make_hex(""));
    }

    // ── expand_sz ─────────────────────────────────────────────────────────────

    #[test]
    fn parses_expand_sz() {
        // "%SystemRoot%\foo" encoded as UTF-16LE
        let s = "%SystemRoot%\\foo";
        let hex = encode_utf16le_hex(&format!("{s}\0")); // NUL-terminated
        let reg = format!(
            "Windows Registry Editor Version 5.00\r\n\r\n\
             [HKEY_CURRENT_USER\\Test]\r\n\
             \"ExpandPath\"=hex(2):{hex}\r\n"
        );
        let v = parse(&reg).unwrap();
        // The parsed value strips trailing NUL because decode_utf16le_hex leaves it in the string;
        // we verify it at least contains the path.
        let parsed_val = v["HKEY_CURRENT_USER\\Test"]["ExpandPath"]["value"]
            .as_str()
            .unwrap();
        assert!(parsed_val.contains(s), "got: {parsed_val:?}");
    }

    #[test]
    fn round_trips_expand_sz_via_serialize() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "EP": {"type": "expand_sz", "value": "%TEMP%\\wine"}
            }
        });
        let s = serialize(&v).unwrap();
        assert!(s.contains("hex(2):"), "expected hex(2) in output: {s}");
        let reparsed = parse(&s).unwrap();
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Test"]["EP"]["value"],
            "%TEMP%\\wine"
        );
    }

    // ── multi_sz ──────────────────────────────────────────────────────────────

    #[test]
    fn round_trips_multi_sz() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "Fonts": {"type": "multi_sz", "value": ["Arial", "Times New Roman", "Courier"]}
            }
        });
        let s = serialize(&v).unwrap();
        assert!(s.contains("hex(7):"), "expected hex(7) in output: {s}");
        let reparsed = parse(&s).unwrap();
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Test"]["Fonts"],
            make_multi(vec!["Arial", "Times New Roman", "Courier"])
        );
    }

    #[test]
    fn round_trips_multi_sz_empty() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "Empty": {"type": "multi_sz", "value": []}
            }
        });
        let s = serialize(&v).unwrap();
        let reparsed = parse(&s).unwrap();
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Test"]["Empty"]["value"],
            json!([])
        );
    }

    // ── qword ─────────────────────────────────────────────────────────────────

    #[test]
    fn round_trips_qword_integer() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "Big": {"type": "qword", "value": 0x0102030405060708u64}
            }
        });
        let s = serialize(&v).unwrap();
        assert!(s.contains("hex(b):"), "expected hex(b): {s}");
        // Parse back — comes back as string
        let reparsed = parse(&s).unwrap();
        let hex_str = reparsed["HKEY_CURRENT_USER\\Test"]["Big"]["value"]
            .as_str()
            .unwrap();
        // LE bytes of 0x0102030405060708
        assert!(hex_str.contains("08"), "LE byte check: {hex_str}");
    }

    #[test]
    fn round_trips_qword_string() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "Q": {"type": "qword", "value": "08,07,06,05,04,03,02,01"}
            }
        });
        let s = serialize(&v).unwrap();
        let reparsed = parse(&s).unwrap();
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Test"]["Q"]["value"],
            "08,07,06,05,04,03,02,01"
        );
    }

    // ── deletions ─────────────────────────────────────────────────────────────

    #[test]
    fn parses_value_deletion() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Test]\r\n\
                   -\"OldValue\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\Test"]["OldValue"], Value::Null);
    }

    #[test]
    fn parses_key_deletion() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [-HKEY_CURRENT_USER\\ObsoleteKey]\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKEY_CURRENT_USER\\ObsoleteKey"], Value::Null);
    }

    #[test]
    fn serializes_value_deletion() {
        let v = json!({
            "HKEY_CURRENT_USER\\Test": {
                "OldValue": null
            }
        });
        let s = serialize(&v).unwrap();
        assert!(s.contains("-\"OldValue\""), "got: {s}");
    }

    #[test]
    fn serializes_key_deletion() {
        let v = json!({
            "HKEY_CURRENT_USER\\ObsoleteKey": null
        });
        let s = serialize(&v).unwrap();
        assert!(s.contains("[-HKEY_CURRENT_USER\\ObsoleteKey]"), "got: {s}");
    }

    // ── multiple sections / keys ───────────────────────────────────────────────

    #[test]
    fn parses_multiple_sections() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\A]\r\n\
                   \"x\"=\"1\"\r\n\
                   \r\n\
                   [HKCU\\B]\r\n\
                   \"y\"=dword:00000002\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\A"]["x"], make_sz("1"));
        assert_eq!(v["HKCU\\B"]["y"], make_dword(2));
    }

    #[test]
    fn parses_multiple_values_in_section() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"a\"=\"alpha\"\r\n\
                   \"b\"=dword:0000000a\r\n\
                   \"c\"=hex:de,ad,be,ef\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["a"], make_sz("alpha"));
        assert_eq!(v["HKCU\\Test"]["b"], make_dword(10));
        assert_eq!(v["HKCU\\Test"]["c"], make_hex("de,ad,be,ef"));
    }

    // ── line continuation ─────────────────────────────────────────────────────

    #[test]
    fn parses_hex_with_line_continuation() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Long\"=hex:01,02,03,\\\r\n  04,05,06\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["Long"], make_hex("01,02,03,04,05,06"));
    }

    // ── comments ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_comments() {
        let reg = "Windows Registry Editor Version 5.00\r\n\
                   ; This is a comment\r\n\
                   \r\n\
                   [HKCU\\Test]\r\n\
                   ; another comment\r\n\
                   \"val\"=\"ok\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["val"], make_sz("ok"));
    }

    // ── BOM ───────────────────────────────────────────────────────────────────

    #[test]
    fn strips_utf8_bom() {
        let reg = "\u{FEFF}Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"x\"=\"1\"\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["x"], make_sz("1"));
    }

    // ── round-trips ───────────────────────────────────────────────────────────

    #[test]
    fn round_trip_sz() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Hello\"=\"world\"\r\n";
        round_trip(reg).unwrap();
    }

    #[test]
    fn round_trip_dword() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Num\"=dword:0000002a\r\n";
        round_trip(reg).unwrap();
    }

    #[test]
    fn round_trip_hex() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Bin\"=hex:ca,fe,ba,be\r\n";
        round_trip(reg).unwrap();
    }

    #[test]
    fn round_trip_deletions() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   -\"Dead\"\r\n\
                   \"Alive\"=\"yes\"\r\n";
        round_trip(reg).unwrap();
    }

    #[test]
    fn round_trip_key_deletion() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [-HKCU\\Gone]\r\n";
        round_trip(reg).unwrap();
    }

    // ── Wine-specific real-world scenarios ────────────────────────────────────

    #[test]
    fn wine_direct3d_section() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
                   \"MaxVersionGL\"=\"opengl32,3.3\"\r\n\
                   \"UseGLSL\"=\"enabled\"\r\n\
                   \"VideoMemorySize\"=\"512\"\r\n";
        let v = parse(reg).unwrap();
        let sec = &v["HKEY_CURRENT_USER\\Software\\Wine\\Direct3D"];
        assert_eq!(sec["MaxVersionGL"], make_sz("opengl32,3.3"));
        assert_eq!(sec["UseGLSL"], make_sz("enabled"));
        assert_eq!(sec["VideoMemorySize"], make_sz("512"));
    }

    #[test]
    fn wine_drives_section_with_dword() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_LOCAL_MACHINE\\System\\CurrentControlSet\\Services\\WineD3D]\r\n\
                   \"Version\"=dword:00000001\r\n\
                   \"Flags\"=dword:00000003\r\n";
        let v = parse(reg).unwrap();
        let sec = &v["HKEY_LOCAL_MACHINE\\System\\CurrentControlSet\\Services\\WineD3D"];
        assert_eq!(sec["Version"], make_dword(1));
        assert_eq!(sec["Flags"], make_dword(3));
    }

    #[test]
    fn wine_font_substitutes() {
        // Font substitutions are common in Wine prefix setups
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows NT\\CurrentVersion\\FontSubstitutes]\r\n\
                   \"MS Shell Dlg\"=\"Tahoma\"\r\n\
                   \"MS Shell Dlg 2\"=\"Tahoma\"\r\n";
        let v = parse(reg).unwrap();
        let sec = &v["HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows NT\\CurrentVersion\\FontSubstitutes"];
        assert_eq!(sec["MS Shell Dlg"], make_sz("Tahoma"));
        assert_eq!(sec["MS Shell Dlg 2"], make_sz("Tahoma"));
    }

    #[test]
    fn wine_full_patch_scenario() {
        // Simulate patching a Wine prefix: add D3D settings to existing minimal reg
        use crate::merge::{merge, ArrayStrategy, MergeConfig};
        use std::collections::HashMap;

        let existing_reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                            [HKEY_CURRENT_USER\\Software\\Wine]\r\n\
                            \"Version\"=\"6.0\"\r\n";

        let patch_reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
                         \"UseGLSL\"=\"enabled\"\r\n\
                         \"MaxVersionGL\"=\"opengl32,4.6\"\r\n";

        let existing_val = parse(existing_reg).unwrap();
        let patch_val = parse(patch_reg).unwrap();

        let config = MergeConfig {
            default_array: ArrayStrategy::Replace,
            path_strategies: HashMap::new(),
            clobber: true,
        };

        let merged = merge(existing_val, patch_val, &config, "");
        let output = serialize(&merged).unwrap();
        let reparsed = parse(&output).unwrap();

        // Original key preserved
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Software\\Wine"]["Version"],
            make_sz("6.0")
        );
        // Patch applied
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Software\\Wine\\Direct3D"]["UseGLSL"],
            make_sz("enabled")
        );
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Software\\Wine\\Direct3D"]["MaxVersionGL"],
            make_sz("opengl32,4.6")
        );
    }

    #[test]
    fn wine_no_clobber_preserves_user_override() {
        // If a user has manually tweaked a wine setting, no-clobber must preserve it
        use crate::merge::{merge, ArrayStrategy, MergeConfig};
        use std::collections::HashMap;

        let existing_reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                            [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
                            \"UseGLSL\"=\"disabled\"\r\n";

        let patch_reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                         [HKEY_CURRENT_USER\\Software\\Wine\\Direct3D]\r\n\
                         \"UseGLSL\"=\"enabled\"\r\n\
                         \"NewSetting\"=\"value\"\r\n";

        let existing_val = parse(existing_reg).unwrap();
        let patch_val = parse(patch_reg).unwrap();

        let config = MergeConfig {
            default_array: ArrayStrategy::Replace,
            path_strategies: HashMap::new(),
            clobber: false,
        };

        let merged = merge(existing_val, patch_val, &config, "");
        let output = serialize(&merged).unwrap();
        let reparsed = parse(&output).unwrap();

        // User's override preserved
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Software\\Wine\\Direct3D"]["UseGLSL"],
            make_sz("disabled")
        );
        // New key from patch still added
        assert_eq!(
            reparsed["HKEY_CURRENT_USER\\Software\\Wine\\Direct3D"]["NewSetting"],
            make_sz("value")
        );
    }

    // ── error cases ───────────────────────────────────────────────────────────

    #[test]
    fn rejects_malformed_dword() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Bad\"=dword:ZZZZZZZZ\r\n";
        assert!(parse(reg).is_err());
    }

    #[test]
    fn hex_stored_as_raw_passthrough() {
        // hex: values are stored verbatim — we don't validate the byte content
        // (they are opaque binary blobs). Round-trip must preserve the raw string.
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Good\"=hex:01,ab,ff\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["Good"], make_hex("01,ab,ff"));
    }

    #[test]
    fn rejects_dword_out_of_range_on_serialize() {
        let v = json!({
            "HKCU\\Test": {
                "Bad": {"type": "dword", "value": 0x1_0000_0000u64}
            }
        });
        assert!(serialize(&v).is_err());
    }

    #[test]
    fn rejects_non_object_root_on_serialize() {
        assert!(serialize(&Value::String("nope".into())).is_err());
    }

    #[test]
    fn rejects_non_object_section_on_serialize() {
        let v = json!({ "HKCU\\Test": "not an object" });
        assert!(serialize(&v).is_err());
    }

    #[test]
    fn rejects_missing_type_field() {
        let v = json!({ "HKCU\\Test": { "val": {"value": "x"} } });
        assert!(serialize(&v).is_err());
    }

    #[test]
    fn rejects_missing_value_field() {
        let v = json!({ "HKCU\\Test": { "val": {"type": "sz"} } });
        assert!(serialize(&v).is_err());
    }

    #[test]
    fn rejects_unknown_value_type() {
        let v = json!({ "HKCU\\Test": { "val": {"type": "bogus", "value": "x"} } });
        assert!(serialize(&v).is_err());
    }

    // ── edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn empty_section_serializes_and_reparses() {
        let v = json!({ "HKCU\\Empty": {} });
        let s = serialize(&v).unwrap();
        assert!(s.contains("[HKCU\\Empty]"), "got: {s}");
        let reparsed = parse(&s).unwrap();
        assert_eq!(reparsed["HKCU\\Empty"], json!({}));
    }

    #[test]
    fn value_name_with_spaces_and_backslash() {
        let v = json!({
            "HKCU\\Test": {
                "My Value\\Path": {"type": "sz", "value": "test"}
            }
        });
        let s = serialize(&v).unwrap();
        let reparsed = parse(&s).unwrap();
        assert_eq!(reparsed["HKCU\\Test"]["My Value\\Path"], make_sz("test"));
    }

    #[test]
    fn value_with_unicode_string() {
        let v = json!({
            "HKCU\\Test": {
                "Unicode": {"type": "sz", "value": "日本語テスト"}
            }
        });
        let s = serialize(&v).unwrap();
        let reparsed = parse(&s).unwrap();
        assert_eq!(reparsed["HKCU\\Test"]["Unicode"], make_sz("日本語テスト"));
    }

    #[test]
    fn lf_only_line_endings_accepted() {
        // Some tools write \n instead of \r\n; we should handle both
        let reg = "Windows Registry Editor Version 5.00\n\n[HKCU\\Test]\n\"x\"=\"ok\"\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["x"], make_sz("ok"));
    }

    #[test]
    fn hex_passthrough_type() {
        // hex(3) = REG_BINARY variant — should pass through
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
                   [HKCU\\Test]\r\n\
                   \"Custom\"=hex(3):01,02\r\n";
        let v = parse(reg).unwrap();
        assert_eq!(v["HKCU\\Test"]["Custom"]["type"], "hex(3)");
        assert_eq!(v["HKCU\\Test"]["Custom"]["value"], "01,02");
        // Round-trip
        let s = serialize(&v).unwrap();
        let reparsed = parse(&s).unwrap();
        assert_eq!(reparsed["HKCU\\Test"]["Custom"]["type"], "hex(3)");
    }
}
