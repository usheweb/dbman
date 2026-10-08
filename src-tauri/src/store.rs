use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::State;

use crate::state::AppState;

/// 白名单 key → 固定文件名，杜绝路径穿越。
const STORE_FILES: &[(&str, &str)] = &[
    ("dbman_connections", "connections.json"),
    ("dbman_connection_databases", "connection_databases.json"),
    ("dbman_page_size", "page_size.json"),
    ("dbman_global_display", "global_display.json"),
    ("dbman_panel_state", "panel_state.json"),
    ("dbman_export_opts", "export_opts.json"),
    ("dbman_sql_log", "sql_log.jsonl"),
    ("dbman_sql_favorites", "sql_favorites.jsonl"),
    ("dbman_ai_config", "ai_config.json"),
];

fn store_file(dir: &Path, key: &str) -> Option<PathBuf> {
    STORE_FILES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, name)| dir.join(name))
}

fn is_jsonl(path: &Path) -> bool {
    path.extension().map(|e| e == "jsonl").unwrap_or(false)
}

/// 普通 JSON 整体 parse；JSONL 逐行 parse 后组装成数组；不存在/损坏返回 None。
fn read_store_file(file: &Path) -> Option<Value> {
    if !file.exists() {
        return None;
    }
    let text = std::fs::read_to_string(file).ok()?;
    if is_jsonl(file) {
        let mut items = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            items.push(serde_json::from_str::<Value>(line).ok()?);
        }
        Some(Value::Array(items))
    } else {
        serde_json::from_str(&text).ok()
    }
}

/// 先写临时文件再 rename，避免半截内容。
fn write_store_file(file: &Path, value: &Value) -> Result<(), String> {
    let text = if is_jsonl(file) {
        match value {
            Value::Array(items) => items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
                .join("\n"),
            other => serde_json::to_string(other).map_err(|e| e.to_string())?,
        }
    } else {
        serde_json::to_string(value).map_err(|e| e.to_string())?
    };

    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())?;
    Ok(())
}

/// 启动时把早期的 .json 旧文件合并进 .jsonl（按整条内容去重后追加），再删除旧文件。
pub fn migrate_legacy(dir: &Path) {
    for (_, name) in STORE_FILES {
        if !name.ends_with(".jsonl") {
            continue;
        }
        let file = dir.join(name);
        let legacy = dir.join(name.replace(".jsonl", ".json"));
        if !legacy.exists() {
            continue;
        }
        let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
            let old_text = std::fs::read_to_string(&legacy)?;
            let old: Value = serde_json::from_str(&old_text)?;
            let old_items = match old {
                Value::Array(items) => items,
                _ => Vec::new(),
            };
            let cur = if file.exists() {
                match read_store_file(&file) {
                    Some(Value::Array(items)) => items,
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            let mut seen: HashSet<String> = cur
                .iter()
                .map(|item| serde_json::to_string(item).unwrap_or_default())
                .collect();
            let mut merged = cur;
            for item in old_items {
                let key = serde_json::to_string(&item).unwrap_or_default();
                if seen.insert(key) {
                    merged.push(item);
                }
            }
            write_store_file(&file, &Value::Array(merged))?;
            std::fs::remove_file(&legacy)?;
            Ok(())
        })();
    }
}

#[tauri::command]
pub async fn store_load(state: State<'_, AppState>) -> Result<Value, String> {
    let mut map = serde_json::Map::new();
    for (key, _) in STORE_FILES {
        let value = store_file(&state.store_dir, key)
            .and_then(|f| read_store_file(&f))
            .unwrap_or(Value::Null);
        map.insert((*key).to_string(), value);
    }
    Ok(Value::Object(map))
}

#[tauri::command]
pub async fn store_set(
    state: State<'_, AppState>,
    key: String,
    value: Value,
) -> Result<bool, String> {
    let file = store_file(&state.store_dir, &key).ok_or_else(|| "Unknown key".to_string())?;
    write_store_file(&file, &value)?;
    Ok(true)
}
