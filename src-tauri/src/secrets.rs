// 系统安全存储（macOS 钥匙串 / Windows 凭据管理器 / Linux Secret Service）
//
// 这里只存「秘密」本身：数据库密码、SSH 密码、SSH 私钥、AI apiKey。
// 连接的其余字段（主机/端口/用户名/库名）仍旧落在 data/connections.json，
// 秘密不落盘。钥匙串不可用时命令返回错误，前端自行降级为明文保存，功能不受影响。

use keyring::{Entry, Error};

/// 统一服务名：钥匙串里所有 DBMan 条目都挂在它下面，便于用户自行查看/清理
const SERVICE: &str = "com.tangsanlang.dbman";

/// 是否启用系统钥匙串：只在发布版（release）启用。
///
/// debug 构建每次重编译签名都会变，macOS 会反复弹「允许访问钥匙串」授权框，
/// 开发期无法忍受。debug 下三个命令直接返回错误，前端已有的降级链路会接管：
/// 密码回到本地配置文件明文保存（与 1.3.0 之前的行为一致），并把「清除已保存的密码」按成功处理。
///
/// 注意：这意味着开发期不会走到钥匙串代码路径，发版前必须用一次 release 产物实测。
/// 另外 debug 下新存的明文密码，切到 release 后会优先读钥匙串里的旧值，表现为该连接认证失败，
/// 重新在 release 版里保存一次密码即可。
///
/// debug 下的降级提示：前端收到错误后会自动改走明文存储
const DEBUG_FALLBACK: &str = "debug 构建不读写系统钥匙串（避免重编译后反复弹授权框），已降级为明文保存";

/// 用 OnceLock 包一层，避免 const 折叠后 debug 分支被判为不可达代码而告警
fn use_keyring() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| !cfg!(debug_assertions))
}

fn entry(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|e| format!("钥匙串不可用：{e}"))
}

/// 写入（或覆盖）一条凭据
#[tauri::command]
pub fn secret_set(account: String, value: String) -> Result<(), String> {
    if !use_keyring() {
        return Err(DEBUG_FALLBACK.into());
    }
    entry(&account)?.set_password(&value).map_err(|e| e.to_string())
}

/// 读取一条凭据；没存过返回 None（与「钥匙串读不了」区分开）
#[tauri::command]
pub fn secret_get(account: String) -> Result<Option<String>, String> {
    if !use_keyring() {
        return Err(DEBUG_FALLBACK.into());
    }
    match entry(&account)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// 删除一条凭据；本来就不存在时按成功处理（幂等）
#[tauri::command]
pub fn secret_delete(account: String) -> Result<(), String> {
    if !use_keyring() {
        // debug 下钥匙串根本没写过，按成功返回，避免「清除已保存的密码」误报失败
        return Ok(());
    }
    match entry(&account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
