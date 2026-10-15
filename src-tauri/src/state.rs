use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::db::RelationalDriver;
use crate::ssh::Tunnel;

/// 已建立的数据库连接。关系型库统一以 `Arc<dyn RelationalDriver>` 存储，
/// 驱动内部自行持有并加锁连接；Redis 的 ConnectionManager 与 MongoDB 的
/// Client 本身都廉价可 Clone（内部连接池），直接存。
pub enum DbConn {
    Relational(Arc<dyn RelationalDriver>),
    Redis(redis::aio::ConnectionManager),
    Mongo(mongodb::Client),
}

/// AI 生成会话：保留上一次生成的完整 messages，支持「继续微调」在其基础上追加。
/// 只存内存，进程重启即失效。
pub struct AiSession {
    pub messages: Vec<serde_json::Value>,
    pub turn: u32,
    pub touched: Instant,
}

pub struct AppState {
    pub connections: Mutex<HashMap<String, DbConn>>,
    pub tunnels: Mutex<HashMap<String, Tunnel>>,
    pub store_dir: PathBuf,
    /// AI 会话，key = "{conn_id}::{database}"
    pub ai_sessions: Mutex<HashMap<String, AiSession>>,
}

impl AppState {
    pub fn new(store_dir: PathBuf) -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            store_dir,
            ai_sessions: Mutex::new(HashMap::new()),
        }
    }

    /// 取出关系型库驱动；找不到或类型不符统一返回 "Invalid connection"，
    /// 前端依赖这个字符串触发自动重连。
    pub async fn get_driver(
        &self,
        conn_id: &str,
    ) -> Result<Arc<dyn RelationalDriver>, String> {
        let map = self.connections.lock().await;
        match map.get(conn_id) {
            Some(DbConn::Relational(c)) => Ok(c.clone()),
            _ => Err("Invalid connection".to_string()),
        }
    }

    pub async fn get_redis(
        &self,
        conn_id: &str,
    ) -> Result<redis::aio::ConnectionManager, String> {
        let map = self.connections.lock().await;
        match map.get(conn_id) {
            Some(DbConn::Redis(c)) => Ok(c.clone()),
            _ => Err("Invalid connection".to_string()),
        }
    }

    pub async fn get_mongo(&self, conn_id: &str) -> Result<mongodb::Client, String> {
        let map = self.connections.lock().await;
        match map.get(conn_id) {
            Some(DbConn::Mongo(c)) => Ok(c.clone()),
            _ => Err("Invalid connection".to_string()),
        }
    }

    pub async fn insert(&self, conn_id: String, conn: DbConn) {
        self.connections.lock().await.insert(conn_id, conn);
    }
}
