use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::ssh::Tunnel;

/// 已建立的数据库连接。MySQL 需要 &mut 才能查询，用 Mutex 包一层；
/// Redis 的 ConnectionManager 本身廉价可 Clone，直接存。
pub enum DbConn {
    Mysql(Arc<Mutex<mysql_async::Conn>>),
    Redis(redis::aio::ConnectionManager),
}

pub struct AppState {
    pub connections: Mutex<HashMap<String, DbConn>>,
    pub tunnels: Mutex<HashMap<String, Tunnel>>,
    pub store_dir: PathBuf,
}

impl AppState {
    pub fn new(store_dir: PathBuf) -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            store_dir,
        }
    }

    /// 取出 MySQL 连接；找不到或类型不符统一返回 "Invalid connection"，
    /// 前端依赖这个字符串触发自动重连。
    pub async fn get_mysql(
        &self,
        conn_id: &str,
    ) -> Result<Arc<Mutex<mysql_async::Conn>>, String> {
        let map = self.connections.lock().await;
        match map.get(conn_id) {
            Some(DbConn::Mysql(c)) => Ok(c.clone()),
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

    pub async fn insert(&self, conn_id: String, conn: DbConn) {
        self.connections.lock().await.insert(conn_id, conn);
    }
}
