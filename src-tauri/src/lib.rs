mod csv_import;
mod db;
mod llm;
mod models;
mod mongo_cmd;
mod redis_cmd;
mod secrets;
mod ssh;
mod state;
mod store;

use std::time::Duration;

use tauri::{Manager, State};
use tokio::time::timeout;

use models::ConnectRequest;
use state::{AppState, DbConn};

/// 建连超时。安全组/防火墙丢包时 TCP 会一直重传（macOS 默认约 75 秒才报错），
/// 这里主动兜底，避免界面长时间没有任何反应。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// 建连超时的统一提示：明确告诉用户可能的原因，而不是抛一句系统级超时。
fn connect_timeout_error(host: &str, port: u16) -> String {
    format!(
        "连接超时（{} 秒）：{}:{} 无响应。请检查地址端口是否正确，以及服务器防火墙或云安全组是否放行本机 IP",
        CONNECT_TIMEOUT.as_secs(),
        host,
        port
    )
}

fn base36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// 对齐 JS generateId()：时间戳 base36 + 随机数 base36。
fn generate_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let rnd: u64 = rand::random();
    format!("{}{}", base36(ts), base36(rnd))
}

#[tauri::command]
async fn connect(state: State<'_, AppState>, payload: ConnectRequest) -> Result<String, String> {
    let db_type = payload.r#type.clone();
    let is_redis = db_type == "redis";
    let is_mongo = db_type == "mongodb" || db_type == "mongo";
    if !is_redis && !is_mongo && !db::is_relational(&db_type) {
        return Err("Unsupported database type".to_string());
    }

    let conn_id = generate_id();
    let ssh_cfg = payload.ssh.clone().unwrap_or_default();
    // 本地文件型数据库（如 SQLite）无主机/端口，也不需要 SSH 隧道。
    let is_file_based = db::is_file_based(&db_type);

    // 用户填写的真实目标：SSH 模式下 host/port 会被换成本地转发地址，报错时仍展示真实目标
    let target_host = payload.config.host.clone();
    let target_port = payload
        .config
        .port
        .unwrap_or_else(|| db::default_port(&db_type));

    let mut host = target_host.clone();
    let mut port = target_port;

    // 主机为空时各驱动的报错五花八门（MongoDB 驱动抛 "the hostname cannot be empty"），
    // 在这里统一给一句中文提示，前端漏校验时也不会露出驱动的原始文案
    if !is_file_based && host.trim().is_empty() {
        return Err("请填写主机地址（本机数据库可填 localhost）".to_string());
    }

    if ssh_cfg.enabled && !is_file_based {
        let ssh_host = ssh_cfg.host.clone().unwrap_or_default();
        let ssh_port = ssh_cfg.port.unwrap_or(22);
        let tunnel = timeout(
            CONNECT_TIMEOUT,
            ssh::connect_tunnel(&ssh_cfg, &target_host, target_port),
        )
        .await
        .map_err(|_| connect_timeout_error(&ssh_host, ssh_port))??;
        let local_port = tunnel.local_port;
        state.tunnels.lock().await.insert(conn_id.clone(), tunnel);
        host = "127.0.0.1".to_string();
        port = local_port;
    }

    let connect_result = if is_mongo {
        match timeout(
            CONNECT_TIMEOUT,
            mongo_cmd::connect(&payload.config, &host, port),
        )
        .await
        {
            Ok(Ok(client)) => {
                state.insert(conn_id.clone(), DbConn::Mongo(client)).await;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(connect_timeout_error(&target_host, target_port)),
        }
    } else if !is_redis {
        match timeout(
            CONNECT_TIMEOUT,
            db::connect_driver(&db_type, &payload.config, &host, port),
        )
        .await
        {
            Ok(Ok(driver)) => {
                state
                    .insert(conn_id.clone(), DbConn::Relational(driver))
                    .await;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(connect_timeout_error(&target_host, target_port)),
        }
    } else {
        match timeout(CONNECT_TIMEOUT, redis_cmd::connect(&payload.config, &host, port)).await {
            Ok(Ok(cm)) => {
                state.insert(conn_id.clone(), DbConn::Redis(cm)).await;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(connect_timeout_error(&target_host, target_port)),
        }
    };

    match connect_result {
        Ok(()) => Ok(conn_id),
        Err(e) => {
            // 连接失败时拆除已建立的隧道
            state.tunnels.lock().await.remove(&conn_id);
            Err(e)
        }
    }
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>, conn_id: String) -> Result<(), String> {
    // 移除隧道（Tunnel 的 Drop 会 abort SSH 会话）
    state.tunnels.lock().await.remove(&conn_id);

    // 驱动 / ConnectionManager 直接 drop 即可关闭连接
    state.connections.lock().await.remove(&conn_id);
    Ok(())
}

/// 把已在前端内存中的内容原样（含 BOM/CRLF）覆盖写入文件。
#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content.as_bytes()).map_err(|e| e.to_string())
}

/// 读取文本文件（外部 SQL 文件用）：UTF-8 优先，非法序列回退 GB18030
#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(csv_import::decode_bytes(&bytes, "auto").0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let store_dir = app.path().app_data_dir()?.join("data");
            std::fs::create_dir_all(&store_dir)?;
            store::migrate_legacy(&store_dir);
            app.manage(AppState::new(store_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            write_text_file,
            read_text_file,
            db::db_databases,
            db::db_tables,
            db::db_columns,
            db::db_query,
            db::db_export,
            db::db_table_ddl,
            db::db_table_structure,
            db::db_export_data_sql,
            db::db_exec_script,
            csv_import::csv_preview,
            csv_import::db_import_csv,
            redis_cmd::redis_info,
            redis_cmd::redis_keys,
            redis_cmd::redis_get,
            redis_cmd::redis_select_db,
            redis_cmd::redis_databases,
            redis_cmd::redis_set_ttl,
            redis_cmd::redis_set_value,
            mongo_cmd::mongo_databases,
            mongo_cmd::mongo_collections,
            mongo_cmd::mongo_query,
            mongo_cmd::mongo_indexes,
            mongo_cmd::mongo_export,
            llm::ai_generate_sql,
            llm::ai_test_config,
            store::store_load,
            store::store_set,
            secrets::secret_set,
            secrets::secret_get,
            secrets::secret_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
