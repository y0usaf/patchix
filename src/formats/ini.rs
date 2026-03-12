use anyhow::{Context, Result};
use ini::Ini;
use serde_json::{Map, Value};

/// Parse INI into JSON structure:
/// { "section": { "key": "value" }, "__global__": { "key": "value" } }
pub fn parse(input: &str) -> Result<Value> {
    let ini = Ini::load_from_str(input).context("invalid INI")?;
    let mut root = Map::new();

    for (section, props) in &ini {
        let section_name = section.unwrap_or("__global__");
        let mut section_map = Map::new();
        for (key, value) in props.iter() {
            section_map.insert(key.to_string(), Value::String(value.to_string()));
        }
        root.insert(section_name.to_string(), Value::Object(section_map));
    }

    Ok(Value::Object(root))
}

pub fn serialize(value: &Value) -> Result<String> {
    let obj = value
        .as_object()
        .context("INI root must be an object")?;

    let mut ini = Ini::new();

    for (section, props) in obj {
        let props = props
            .as_object()
            .with_context(|| format!("INI section '{section}' must be an object"))?;

        let section_name = if section == "__global__" {
            None
        } else {
            Some(section.as_str())
        };

        for (key, value) in props {
            let str_val = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => continue,
                _ => anyhow::bail!("INI values must be scalars, got nested structure at [{section}].{key}"),
            };
            ini.with_section(section_name).set(key, str_val);
        }
    }

    let mut buf = Vec::new();
    ini.write_to(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}
