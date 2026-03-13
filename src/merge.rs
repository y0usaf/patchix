use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArrayStrategy {
    Replace,
    Append,
    Prepend,
    Union,
}

pub struct MergeConfig {
    pub default_array: ArrayStrategy,
    pub path_strategies: HashMap<String, ArrayStrategy>,
    /// When false, existing values are preserved — patch only fills in missing keys.
    pub clobber: bool,
}

impl MergeConfig {
    fn array_strategy_for(&self, path: &str) -> &ArrayStrategy {
        // Try exact match first, then strip leading dot
        let normalized = path.strip_prefix('.').unwrap_or(path);
        self.path_strategies
            .get(normalized)
            .unwrap_or(&self.default_array)
    }
}

/// Deep merge patch into existing value following RFC 7396 semantics
/// with configurable array strategies.
pub fn merge(existing: Value, patch: Value, config: &MergeConfig, path: &str) -> Value {
    match (existing, patch) {
        // Both objects: recursive deep merge
        (Value::Object(mut base), Value::Object(patch)) => {
            for (key, patch_val) in patch {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };

                if patch_val.is_null() {
                    // RFC 7386: null in patch means delete the key (only when clobber is enabled)
                    if config.clobber {
                        base.remove(&key);
                    }
                    // else: no-clobber — preserve existing key, ignore null
                } else if let Some(base_val) = base.remove(&key) {
                    // Both are objects: always recurse deeper regardless of clobber
                    if base_val.is_object() && patch_val.is_object() {
                        base.insert(key, merge(base_val, patch_val, config, &child_path));
                    } else if config.clobber {
                        // Clobber: patch wins — recurse for arrays (strategy handling), direct replace otherwise
                        if base_val.is_array() && patch_val.is_array() {
                            base.insert(key, merge(base_val, patch_val, config, &child_path));
                        } else {
                            base.insert(key, patch_val);
                        }
                    } else {
                        // No clobber: existing value preserved
                        base.insert(key, base_val);
                    }
                } else {
                    // Key doesn't exist yet: always insert
                    base.insert(key, patch_val);
                }
            }
            Value::Object(base)
        }

        // Both arrays: use configured strategy
        (Value::Array(base), Value::Array(patch)) => {
            if !config.clobber {
                // No clobber: keep existing array
                Value::Array(base)
            } else {
                let strategy = config.array_strategy_for(path);
                match strategy {
                    ArrayStrategy::Replace => Value::Array(patch),
                    ArrayStrategy::Append => {
                        let mut result = base;
                        result.extend(patch);
                        Value::Array(result)
                    }
                    ArrayStrategy::Prepend => {
                        let mut result = patch;
                        result.extend(base);
                        Value::Array(result)
                    }
                    ArrayStrategy::Union => {
                        // O(n*m) — acceptable for config file sizes; Value doesn't implement Hash
                        let mut result = base;
                        for item in patch {
                            if !result.contains(&item) {
                                result.push(item);
                            }
                        }
                        Value::Array(result)
                    }
                }
            }
        }

        // Patch is null at top level: treat as no-op (key deletion is handled in the object branch)
        (existing, Value::Null) => existing,

        // Scalar conflict: clobber decides
        (existing, patch) => {
            if config.clobber {
                patch
            } else {
                existing
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_config() -> MergeConfig {
        MergeConfig {
            default_array: ArrayStrategy::Replace,
            path_strategies: HashMap::new(),
            clobber: true,
        }
    }

    #[test]
    fn deep_merge_objects() {
        let existing = json!({"a": {"b": 1, "c": 2}, "d": 3});
        let patch = json!({"a": {"b": 10, "e": 5}});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"a": {"b": 10, "c": 2, "e": 5}, "d": 3}));
    }

    #[test]
    fn null_deletes_key() {
        let existing = json!({"a": 1, "b": 2, "c": 3});
        let patch = json!({"b": null});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"a": 1, "c": 3}));
    }

    #[test]
    fn array_replace() {
        let existing = json!({"items": [1, 2, 3]});
        let patch = json!({"items": [4, 5]});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"items": [4, 5]}));
    }

    #[test]
    fn array_append() {
        let existing = json!({"items": [1, 2, 3]});
        let patch = json!({"items": [4, 5]});
        let config = MergeConfig {
            default_array: ArrayStrategy::Append,
            path_strategies: HashMap::new(),
            clobber: true,
        };
        let result = merge(existing, patch, &config, "");
        assert_eq!(result, json!({"items": [1, 2, 3, 4, 5]}));
    }

    #[test]
    fn array_prepend() {
        let existing = json!({"items": [1, 2, 3]});
        let patch = json!({"items": [4, 5]});
        let config = MergeConfig {
            default_array: ArrayStrategy::Prepend,
            path_strategies: HashMap::new(),
            clobber: true,
        };
        let result = merge(existing, patch, &config, "");
        assert_eq!(result, json!({"items": [4, 5, 1, 2, 3]}));
    }

    #[test]
    fn array_union() {
        let existing = json!({"items": [1, 2, 3]});
        let patch = json!({"items": [2, 3, 4]});
        let config = MergeConfig {
            default_array: ArrayStrategy::Union,
            path_strategies: HashMap::new(),
            clobber: true,
        };
        let result = merge(existing, patch, &config, "");
        assert_eq!(result, json!({"items": [1, 2, 3, 4]}));
    }

    #[test]
    fn per_path_strategy() {
        let existing = json!({
            "plugins": ["a", "b"],
            "keybinds": ["ctrl+a", "ctrl+b"]
        });
        let patch = json!({
            "plugins": ["c"],
            "keybinds": ["ctrl+c"]
        });
        let config = MergeConfig {
            default_array: ArrayStrategy::Replace,
            path_strategies: HashMap::from([
                ("plugins".to_string(), ArrayStrategy::Append),
            ]),
            clobber: true,
        };
        let result = merge(existing, patch, &config, "");
        assert_eq!(
            result,
            json!({
                "plugins": ["a", "b", "c"],
                "keybinds": ["ctrl+c"]
            })
        );
    }

    #[test]
    fn nested_path_strategy() {
        let existing = json!({"editor": {"formatters": ["rustfmt"]}});
        let patch = json!({"editor": {"formatters": ["prettier"]}});
        let config = MergeConfig {
            default_array: ArrayStrategy::Replace,
            path_strategies: HashMap::from([
                ("editor.formatters".to_string(), ArrayStrategy::Append),
            ]),
            clobber: true,
        };
        let result = merge(existing, patch, &config, "");
        assert_eq!(
            result,
            json!({"editor": {"formatters": ["rustfmt", "prettier"]}})
        );
    }

    #[test]
    fn patch_adds_new_keys() {
        let existing = json!({"a": 1});
        let patch = json!({"b": 2});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn scalar_overwrite() {
        let existing = json!({"theme": "light"});
        let patch = json!({"theme": "dark"});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"theme": "dark"}));
    }

    #[test]
    fn type_change_patch_wins() {
        let existing = json!({"val": "string"});
        let patch = json!({"val": {"nested": true}});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"val": {"nested": true}}));
    }

    #[test]
    fn top_level_null_patch_is_noop() {
        let existing = json!({"a": 1});
        let result = merge(existing.clone(), Value::Null, &default_config(), "");
        assert_eq!(result, existing);
    }

    #[test]
    fn empty_existing() {
        let existing = json!({});
        let patch = json!({"a": 1, "b": {"c": 2}});
        let result = merge(existing, patch, &default_config(), "");
        assert_eq!(result, json!({"a": 1, "b": {"c": 2}}));
    }

    fn no_clobber_config() -> MergeConfig {
        MergeConfig {
            default_array: ArrayStrategy::Replace,
            path_strategies: HashMap::new(),
            clobber: false,
        }
    }

    #[test]
    fn no_clobber_preserves_existing_scalars() {
        let existing = json!({"theme": "light", "fontSize": 12});
        let patch = json!({"theme": "dark", "fontSize": 14});
        let result = merge(existing, patch, &no_clobber_config(), "");
        assert_eq!(result, json!({"theme": "light", "fontSize": 12}));
    }

    #[test]
    fn no_clobber_adds_missing_keys() {
        let existing = json!({"theme": "light"});
        let patch = json!({"theme": "dark", "newKey": "value"});
        let result = merge(existing, patch, &no_clobber_config(), "");
        assert_eq!(result, json!({"theme": "light", "newKey": "value"}));
    }

    #[test]
    fn no_clobber_recurses_into_objects() {
        // Even with no-clobber, we recurse into nested objects
        // so that missing keys inside them get filled in
        let existing = json!({"editor": {"fontSize": 12}});
        let patch = json!({"editor": {"fontSize": 14, "tabSize": 2}});
        let result = merge(existing, patch, &no_clobber_config(), "");
        assert_eq!(result, json!({"editor": {"fontSize": 12, "tabSize": 2}}));
    }

    #[test]
    fn no_clobber_preserves_existing_arrays() {
        let existing = json!({"plugins": ["a", "b"]});
        let patch = json!({"plugins": ["c"]});
        let result = merge(existing, patch, &no_clobber_config(), "");
        assert_eq!(result, json!({"plugins": ["a", "b"]}));
    }

    #[test]
    fn no_clobber_plugin_toggle_scenario() {
        // User's Nix config declares plugin as enabled
        // User manually disables it at runtime
        // Next rebuild: no-clobber preserves the manual disable
        let existing = json!({
            "enabledPlugins": {"audio-notify": false, "gh": true},
            "runtimeKey": "preserved"
        });
        let patch = json!({
            "enabledPlugins": {"audio-notify": true, "gh": true, "new-plugin": true},
            "model": "opus"
        });
        let result = merge(existing, patch, &no_clobber_config(), "");
        assert_eq!(result, json!({
            "enabledPlugins": {"audio-notify": false, "gh": true, "new-plugin": true},
            "runtimeKey": "preserved",
            "model": "opus"
        }));
    }

    #[test]
    fn no_clobber_protects_against_null_deletion() {
        let existing = json!({"a": 1, "b": 2});
        let patch = json!({"b": null});
        let result = merge(existing, patch, &no_clobber_config(), "");
        // With no-clobber, null patch should not delete the key
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }
}
