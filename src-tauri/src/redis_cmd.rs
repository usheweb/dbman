use std::collections::HashMap;

use redis::aio::ConnectionManager;
use redis::{ConnectionAddr, ConnectionInfo, RedisConnectionInfo};
use serde_json::{json, Value as JsonValue};
use tauri::State;

use crate::models::{DbConfig, RedisInfo, RedisValue};
use crate::state::AppState;

pub async fn connect(
    cfg: &DbConfig,
    host: &str,
    port: u16,
) -> Result<ConnectionManager, String> {
    let info = ConnectionInfo {
        addr: ConnectionAddr::Tcp(host.to_string(), port),
        redis: RedisConnectionInfo {
            db: cfg.db.unwrap_or(0),
            username: None,
            password: cfg.password.clone().filter(|p| !p.is_empty()),
            ..Default::default()
        },
    };
    let client = redis::Client::open(info).map_err(|e| e.to_string())?;
    let mut cm = ConnectionManager::new(client).await.map_err(|e| e.to_string())?;
    // 对齐 ioredis 连接时会先 ping 校验
    let _: String = redis::cmd("PING")
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;
    Ok(cm)
}

fn opt_string_to_json(value: Option<String>) -> JsonValue {
    match value {
        Some(s) => JsonValue::String(s),
        None => JsonValue::Null,
    }
}

#[tauri::command]
pub async fn redis_info(state: State<'_, AppState>, conn_id: String) -> Result<RedisInfo, String> {
    let mut cm = state.get_redis(&conn_id).await?;
    let info: String = redis::cmd("INFO")
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;
    let dbsize: u64 = redis::cmd("DBSIZE")
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;
    Ok(RedisInfo { info, dbsize })
}

#[tauri::command]
pub async fn redis_keys(
    state: State<'_, AppState>,
    conn_id: String,
    pattern: String,
) -> Result<Vec<String>, String> {
    let mut cm = state.get_redis(&conn_id).await?;
    let pattern = if pattern.is_empty() {
        "*".to_string()
    } else {
        pattern
    };
    redis::cmd("KEYS")
        .arg(pattern)
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn redis_get(
    state: State<'_, AppState>,
    conn_id: String,
    key: String,
) -> Result<RedisValue, String> {
    let mut cm = state.get_redis(&conn_id).await?;

    let r#type: String = redis::cmd("TYPE")
        .arg(&key)
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;

    let value: JsonValue = match r#type.as_str() {
        "string" => {
            let v: Option<String> = redis::cmd("GET")
                .arg(&key)
                .query_async(&mut cm)
                .await
                .map_err(|e| e.to_string())?;
            opt_string_to_json(v)
        }
        "list" => {
            let v: Vec<String> = redis::cmd("LRANGE")
                .arg(&key)
                .arg(0)
                .arg(-1)
                .query_async(&mut cm)
                .await
                .map_err(|e| e.to_string())?;
            json!(v)
        }
        "set" => {
            let v: Vec<String> = redis::cmd("SMEMBERS")
                .arg(&key)
                .query_async(&mut cm)
                .await
                .map_err(|e| e.to_string())?;
            json!(v)
        }
        "zset" => {
            let v: Vec<String> = redis::cmd("ZRANGE")
                .arg(&key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async(&mut cm)
                .await
                .map_err(|e| e.to_string())?;
            json!(v)
        }
        "hash" => {
            let v: HashMap<String, String> = redis::cmd("HGETALL")
                .arg(&key)
                .query_async(&mut cm)
                .await
                .map_err(|e| e.to_string())?;
            json!(v)
        }
        _ => JsonValue::Null,
    };

    let ttl: i64 = redis::cmd("TTL")
        .arg(&key)
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;

    Ok(RedisValue {
        r#type,
        value,
        ttl,
    })
}
