// 系统安全存储（macOS 钥匙串 / Windows 凭据管理器 / Linux Secret Service）
//
// 这里只存「秘密」本身：数据库密码、SSH 密码、SSH 私钥、AI apiKey。
// 连接的其余字段（主机/端口/用户名/库名）仍旧落在 data/connections.json，
// 秘密不落盘。钥匙串不可用时命令返回错误，前端自行降级为明文保存，功能不受影响。

use keyring::{Entry, Error};

/// 统一服务名：钥匙串里所有 DBMan 条目都挂在它下面，便于用户自行查看/清理
const SERVICE: &str = "com.tangsanlang.dbman";

fn entry(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|e| format!("钥匙串不可用：{e}"))
}

/// 写入（或覆盖）一条凭据
#[tauri::command]
pub fn secret_set(account: String, value: String) -> Result<(), String> {
    entry(&account)?.set_password(&value).map_err(|e| e.to_string())
}

/// 读取一条凭据；没存过返回 None（与「钥匙串读不了」区分开）
#[tauri::command]
pub fn secret_get(account: String) -> Result<Option<String>, String> {
    match entry(&account)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// 删除一条凭据；本来就不存在时按成功处理（幂等）
#[tauri::command]
pub fn secret_delete(account: String) -> Result<(), String> {
    match entry(&account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
