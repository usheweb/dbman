mod csv_import;
mod llm;
mod models;
mod mysql;
mod redis_cmd;
mod ssh;
mod state;
mod store;

use std::sync::Arc;

use tauri::{Manager, State};
use tokio::sync::Mutex;

use models::ConnectRequest;
use state::{AppState, DbConn};

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
    if db_type != "mysql" && db_type != "redis" {
        return Err("Unsupported database type".to_string());
    }

    let conn_id = generate_id();
    let ssh_cfg = payload.ssh.clone().unwrap_or_default();

    let mut host = payload.config.host.clone();
    let mut port = payload
        .config
        .port
        .unwrap_or_else(|| mysql::default_port(&db_type));

    if ssh_cfg.enabled {
        let remote_port = payload
            .config
            .port
            .unwrap_or_else(|| mysql::default_port(&db_type));
        let tunnel = ssh::connect_tunnel(&ssh_cfg, &payload.config.host, remote_port).await?;
        let local_port = tunnel.local_port;
        state.tunnels.lock().await.insert(conn_id.clone(), tunnel);
        host = "127.0.0.1".to_string();
        port = local_port;
    }

    let connect_result = if db_type == "mysql" {
        match mysql::connect(&payload.config, &host, port).await {
            Ok(conn) => {
                state
                    .insert(conn_id.clone(), DbConn::Mysql(Arc::new(Mutex::new(conn))))
                    .await;
                Ok(())
            }
            Err(e) => Err(e),
        }
    } else {
        match redis_cmd::connect(&payload.config, &host, port).await {
            Ok(cm) => {
                state.insert(conn_id.clone(), DbConn::Redis(cm)).await;
                Ok(())
            }
            Err(e) => Err(e),
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

    let conn = state.connections.lock().await.remove(&conn_id);
    match conn {
        Some(DbConn::Mysql(conn)) => {
            if let Ok(mutex) = Arc::try_unwrap(conn) {
                let conn = mutex.into_inner();
                let _ = conn.disconnect().await;
            }
        }
        Some(DbConn::Redis(_cm)) => {
            // ConnectionManager 直接 drop 即可关闭
        }
        None => {}
    }
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
            mysql::mysql_databases,
            mysql::mysql_tables,
            mysql::mysql_columns,
            mysql::mysql_query,
            mysql::mysql_export,
            mysql::mysql_table_ddl,
            mysql::mysql_table_structure,
            mysql::mysql_export_data_sql,
            mysql::mysql_exec_ddl,
            csv_import::csv_preview,
            csv_import::mysql_import_csv,
            redis_cmd::redis_info,
            redis_cmd::redis_keys,
            redis_cmd::redis_get,
            llm::ai_generate_sql,
            llm::ai_test_config,
            store::store_load,
            store::store_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
