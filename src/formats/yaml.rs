use anyhow::Result;
use serde_json::Value;

pub fn parse(input: &str) -> Result<Value> {
    Ok(serde_yml::from_str(input)?)
}

pub fn serialize(value: &Value) -> Result<String> {
    let s = serde_yml::to_string(value)?;
    Ok(s.strip_prefix("---\n").unwrap_or(&s).to_string())
}
