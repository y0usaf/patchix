use anyhow::Result;
use serde_json::Value;

pub fn parse(input: &str) -> Result<Value> {
    Ok(serde_yml::from_str(input)?)
}

pub fn serialize(value: &Value) -> Result<String> {
    let s = serde_yml::to_string(value)?;
    let s = s.strip_prefix("---\n")
        .or_else(|| s.strip_prefix("---\r\n"))
        .unwrap_or(&s);
    let mut s = s.to_string();
    if !s.ends_with('\n') {
        s.push('\n');
    }
    Ok(s)
}
