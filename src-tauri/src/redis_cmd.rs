use std::collections::HashMap;

use redis::aio::ConnectionManager;
use redis::{ConnectionAddr, ConnectionInfo, RedisConnectionInfo};
use serde_json::{json, Value as JsonValue};
use tauri::State;

use crate::models::{DbConfig, RedisInfo, RedisKey, RedisKeyPage, RedisValue};
use crate::state::AppState;

// 每页返回的 key 数量：SCAN 的 COUNT 只是提示，可能多返回，累计到该数量即停止
const KEY_PAGE_SIZE: u64 = 200;
// 单次请求内最多发起多少次 SCAN：MATCH 命中率极低时（大库搜一个几乎不存在的关键词），
// 若一直循环到凑满一页，等于一次请求把整个 keyspace 扫一遍。这里设上限，
// 到上限就返回已收集的结果与游标，由用户继续翻页。COUNT=200 时约等于最多访问 3200 个哈希桶。
const KEY_MAX_SCANS: usize = 16;

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

// 前端提交的 JSON 值转成 Redis 的文本参数
fn json_to_text(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

// list / set / zset 的值需要是数组
fn json_to_items(v: &JsonValue, type_name: &str) -> Result<Vec<String>, String> {
    let arr = v
        .as_array()
        .ok_or_else(|| format!("{} 内容格式不正确", type_name))?;
    Ok(arr.iter().map(json_to_text).collect())
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
    cursor: u64,
) -> Result<RedisKeyPage, String> {
    let mut cm = state.get_redis(&conn_id).await?;
    let pattern = if pattern.is_empty() {
        "*".to_string()
    } else {
        pattern
    };

    // 用 SCAN 游标迭代取一页：KEYS 会在单条命令里扫完全库、长时间占住事件循环（这才是「卡死」的元凶）；
    // SCAN 每次只遍历少量哈希桶就返回，单次调用是短命令，不会阻塞实例。
    let mut cur = cursor;
    let mut names: Vec<String> = Vec::new();
    let mut scans = 0usize;
    loop {
        let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cur)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(KEY_PAGE_SIZE)
            .query_async(&mut cm)
            .await
            .map_err(|e| e.to_string())?;
        scans += 1;
        names.extend(batch);
        cur = next;
        // 凑够一页、扫完整个 keyspace（游标归零），或达到单请求扫描上限就停下
        if cur == 0 || names.len() >= KEY_PAGE_SIZE as usize || scans >= KEY_MAX_SCANS {
            break;
        }
    }

    // DBSIZE 是 O(1)，带回来给界面显示库内规模（与 pattern 无关）
    let total: u64 = redis::cmd("DBSIZE")
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;

    if names.is_empty() {
        return Ok(RedisKeyPage {
            cursor: cur,
            total,
            keys: Vec::new(),
        });
    }
    // 用 pipeline 一次性取回本页所有 key 的类型，避免逐键往返造成的延迟
    let mut pipe = redis::pipe();
    for k in &names {
        pipe.cmd("TYPE").arg(k);
    }
    let types: Vec<String> = pipe.query_async(&mut cm).await.map_err(|e| e.to_string())?;
    Ok(RedisKeyPage {
        cursor: cur,
        total,
        keys: names
            .into_iter()
            .zip(types)
            .map(|(name, r#type)| RedisKey { name, r#type })
            .collect(),
    })
}

#[tauri::command]
pub async fn redis_select_db(
    state: State<'_, AppState>,
    conn_id: String,
    db: i64,
) -> Result<(), String> {
    let mut cm = state.get_redis(&conn_id).await?;
    let _: String = redis::cmd("SELECT")
        .arg(db)
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn redis_databases(state: State<'_, AppState>, conn_id: String) -> Result<u32, String> {
    let mut cm = state.get_redis(&conn_id).await?;
    // CONFIG GET databases 返回 [name, value]；托管实例可能禁用 CONFIG，失败时回退默认 16
    let res: Result<Vec<String>, redis::RedisError> = redis::cmd("CONFIG")
        .arg("GET")
        .arg("databases")
        .query_async(&mut cm)
        .await;
    let count = res
        .ok()
        .and_then(|v| v.get(1).and_then(|s| s.parse::<u32>().ok()));
    Ok(count.unwrap_or(16))
}

#[tauri::command]
pub async fn redis_set_ttl(
    state: State<'_, AppState>,
    conn_id: String,
    key: String,
    ttl: i64,
) -> Result<(), String> {
    let mut cm = state.get_redis(&conn_id).await?;
    if ttl < 0 {
        // -1（或更小）表示永久：去掉过期时间
        let _: i64 = redis::cmd("PERSIST")
            .arg(&key)
            .query_async(&mut cm)
            .await
            .map_err(|e| e.to_string())?;
    } else if ttl == 0 {
        // EXPIRE 0 会直接删除 key，这里拦下来避免误删
        return Err("TTL 需大于 0；如需永久保存请填 -1".to_string());
    } else {
        let _: i64 = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(ttl)
            .query_async(&mut cm)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// 整体重写 key 的内容：按服务端实际类型原子地重建，并保留原有 TTL
#[tauri::command]
pub async fn redis_set_value(
    state: State<'_, AppState>,
    conn_id: String,
    key: String,
    value: JsonValue,
) -> Result<(), String> {
    let mut cm = state.get_redis(&conn_id).await?;

    // 以服务端实际类型为准，避免前后端类型不一致写坏数据
    let r#type: String = redis::cmd("TYPE")
        .arg(&key)
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;
    if r#type == "none" {
        return Err("Key 不存在，无法保存".to_string());
    }

    // 先记下原 TTL：重建会清掉过期时间，保存后按原值恢复
    let ttl: i64 = redis::cmd("TTL")
        .arg(&key)
        .query_async(&mut cm)
        .await
        .map_err(|e| e.to_string())?;

    let mut pipe = redis::pipe();
    pipe.atomic();
    match r#type.as_str() {
        "string" => {
            pipe.cmd("SET").arg(&key).arg(json_to_text(&value));
        }
        "list" => {
            let items = json_to_items(&value, &r#type)?;
            pipe.cmd("DEL").arg(&key);
            if !items.is_empty() {
                let c = pipe.cmd("RPUSH");
                c.arg(&key);
                for it in &items {
                    c.arg(it);
                }
            }
        }
        "set" => {
            let items = json_to_items(&value, &r#type)?;
            pipe.cmd("DEL").arg(&key);
            if !items.is_empty() {
                let c = pipe.cmd("SADD");
                c.arg(&key);
                for it in &items {
                    c.arg(it);
                }
            }
        }
        "zset" => {
            // 前端按 [member, score, member, score, ...] 提交
            let items = json_to_items(&value, &r#type)?;
            if items.len() % 2 != 0 {
                return Err("有序集合需要成对的成员与分值".to_string());
            }
            pipe.cmd("DEL").arg(&key);
            if !items.is_empty() {
                let c = pipe.cmd("ZADD");
                c.arg(&key);
                for pair in items.chunks(2) {
                    c.arg(&pair[1]).arg(&pair[0]);
                }
            }
        }
        "hash" => {
            let obj = value
                .as_object()
                .ok_or_else(|| "哈希内容格式不正确".to_string())?;
            pipe.cmd("DEL").arg(&key);
            if !obj.is_empty() {
                let c = pipe.cmd("HSET");
                c.arg(&key);
                for (field, val) in obj {
                    c.arg(field).arg(json_to_text(val));
                }
            }
        }
        other => return Err(format!("暂不支持修改 {} 类型", other)),
    }
    if ttl > 0 {
        pipe.cmd("EXPIRE").arg(&key).arg(ttl);
    }
    let _: () = pipe.query_async(&mut cm).await.map_err(|e| e.to_string())?;
    Ok(())
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
