use anyhow::Result;
use serde_json::Value;

pub fn parse(input: &str) -> Result<Value> {
    Ok(serde_yml::from_str(input)?)
}

pub fn serialize(value: &Value) -> Result<String> {
    Ok(serde_yml::to_string(value)?)
}
