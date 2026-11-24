//! Validation for the public CLI result encoding. Values stay as inert JSON.

use serde_json::Value;

pub fn valid(value: &Value) -> bool {
    valid_at(value, 0)
}

fn valid_at(value: &Value, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    let child = |item: &Value| valid_at(item, depth + 1);
    match value["type"].as_str() {
        Some("unit" | "none" | "resource" | "memory") => true,
        Some("bool") => value["value"].is_boolean(),
        Some("int") => value["decimal"].as_str().is_some_and(|value| {
            let digits = value.strip_prefix('-').unwrap_or(value);
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        }),
        Some("float") => match (value.get("value"), value.get("special")) {
            (Some(value), None) => value.as_f64().is_some_and(f64::is_finite),
            (None, Some(special)) => {
                matches!(special.as_str(), Some("nan" | "infinity" | "-infinity"))
            }
            _ => false,
        },
        Some("text") => value["value"].is_string(),
        Some("list") => value["items"]
            .as_array()
            .is_some_and(|items| items.iter().all(child)),
        Some("map") => value["entries"]
            .as_object()
            .is_some_and(|entries| entries.values().all(child)),
        Some("table") => value["header"].as_array().is_some_and(|header| {
            header.iter().all(Value::is_string)
                && value["rows"].as_array().is_some_and(|rows| {
                    rows.iter().all(|row| {
                        row.as_array().is_some_and(|cells| {
                            cells.len() == header.len() && cells.iter().all(child)
                        })
                    })
                })
        }),
        Some("formula") => {
            value["latex"].is_string() && optional(value, "display", Value::is_boolean)
        }
        Some("code") => {
            value["source"].is_string()
                && optional(value, "language", Value::is_string)
                && optional(value, "content_hash", Value::is_string)
        }
        Some("function_ref") => value["module"].is_string() && value["name"].is_string(),
        Some("plugin_handle") => {
            value["plugin"].is_string() && value["export"].is_string() && value["id"].is_u64()
        }
        _ => false,
    }
}

fn optional(value: &Value, name: &str, check: fn(&Value) -> bool) -> bool {
    value.get(name).is_none_or(check)
}

/// Archive terminal values that can be represented as data without handle IDs.
pub fn transferable(value: &Value) -> bool {
    match value["type"].as_str() {
        Some("memory" | "resource" | "plugin_handle" | "function_ref") => false,
        Some("list") => value["items"]
            .as_array()
            .is_some_and(|items| items.iter().all(transferable)),
        Some("map") => value["entries"]
            .as_object()
            .is_some_and(|items| items.values().all(transferable)),
        Some("table") => value["rows"].as_array().is_some_and(|rows| {
            rows.iter().all(|row| {
                row.as_array()
                    .is_some_and(|values| values.iter().all(transferable))
            })
        }),
        _ => true,
    }
}
