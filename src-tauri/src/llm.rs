//! 第一期：大模型生成 SQL。
//!
//! 设计要点（对齐 tslclock 的 IntentParser）：
//! - 请求走 Rust（浏览器直连会被 CORS 挡，密钥也不该进 JS）；二期只需换 base_url 加鉴权头。
//! - **不向模型预传任何表名/表结构**：一次生成通常只用到 1~2 张表，全量表名绝大多数是冗余。
//!   模型必须先用 `search_tables` 找候选表，再用 `get_table_columns` 取字段。
//! - system prompt 保持纯静态（利于上游前缀缓存），动态内容（库名/表总数/需求）放最后一条 user 消息。
//! - 工具结果只给「表名 + 注释 + 字段名 + 类型 + 注释」，不给任何真实数据行。
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::mysql;
use crate::state::AppState;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_MAX_ROUNDS: u8 = 6;
/// search_tables 无关键词时最多返回的表数量（超出只给总数）。
const SEARCH_LIMIT: usize = 50;
/// 生成过程中的实时进度事件：每完成一步就推一条，前端不必等整轮结束才看到结果。
const PROGRESS_EVENT: &str = "ai:progress";

/// system prompt 必须纯静态：只含角色与规则，不含库名/表名，
/// 这样每次请求的前缀完全一致，可命中上游大模型的前缀缓存。
const SYSTEM_PROMPT: &str = "\
你是资深 MySQL 数据库工程师，负责把用户的自然语言需求转换成一条可直接执行的 SQL。

工作方式（必须遵守）：
1. 你**看不到**数据库里的任何表名和字段，严禁凭猜测编造表名或列名。
2. 生成 SQL 前，必须先调用 search_tables 找到相关的表；不确定表名时用简短关键词搜索。
3. 确定候选表后，调用 get_table_columns 获取这些表的真实字段（名称/类型/注释）；只能使用工具返回过的表名与列名。
4. 一次需求通常只涉及 1~2 张表，只查询你确实需要用到的表，不要索取全部表结构。

输出要求：
1. 只输出一条 SQL 语句本身，不要任何解释文字，不要使用 ``` 代码围栏。
2. 使用 MySQL 语法，表名与列名用反引号包裹。
3. 除非用户明确要求，不要生成 INSERT / UPDATE / DELETE / DROP / ALTER 等改动数据或结构的语句。
4. 若需求里的时间范围对应的是时间戳字段，按用户描述的时间语义处理。";

#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    #[serde(default, rename = "baseUrl")]
    pub base_url: String,
    #[serde(default, rename = "apiKey")]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default, rename = "timeoutSecs")]
    pub timeout_secs: Option<u64>,
    #[serde(default, rename = "maxRounds")]
    pub max_rounds: Option<u8>,
}

/// 暴露给模型的工具定义（OpenAI 兼容格式）。
fn tools_schema() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "search_tables",
                "description": "按关键词模糊搜索数据库里的表，返回表名与表注释。不传 keyword 时返回前 50 张表及表总数。不确定该用哪些表时先调用它。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "keyword": {
                            "type": "string",
                            "description": "表名或表注释的关键词，可留空"
                        }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_table_columns",
                "description": "获取指定表的字段列表（字段名、类型、注释）。table 必须是 search_tables 返回过的真实表名。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "table": { "type": "string", "description": "表名" }
                    },
                    "required": ["table"]
                }
            }
        }
    ])
}

/// 把用户填的地址归一化成完整的 chat/completions 端点。
fn normalize_endpoint(base: &str) -> Result<String, String> {
    let b = base.trim().trim_end_matches('/');
    if b.is_empty() {
        return Err("请先在 AI 设置中填写接口地址".to_string());
    }
    if !b.starts_with("http://") && !b.starts_with("https://") {
        return Err("接口地址需以 http:// 或 https:// 开头".to_string());
    }
    if b.ends_with("/chat/completions") {
        return Ok(b.to_string());
    }
    if b.ends_with("/v1") {
        return Ok(format!("{}/chat/completions", b));
    }
    Ok(format!("{}/v1/chat/completions", b))
}

/// 去掉 ```sql 代码围栏与首尾多余空白。
fn clean_sql(raw: &str) -> String {
    let mut text = raw.trim();
    if text.starts_with("```") {
        if let Some(idx) = text.find('\n') {
            text = &text[idx + 1..];
        }
        if let Some(end) = text.rfind("```") {
            text = &text[..end];
        }
    }
    text.trim().to_string()
}

fn accumulate_usage(total: &mut Value, resp: &Value) {
    let usage = resp.get("usage");
    for key in ["prompt_tokens", "completion_tokens", "total_tokens"] {
        let add = usage
            .and_then(|u| u.get(key))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let prev = total.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
        total[key] = json!(prev + add);
    }
}

/// 发一次 chat/completions 请求，把 HTTP/业务错误翻译成可读文案。
async fn post_chat(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    body: &Value,
) -> Result<Value, String> {
    let resp = client
        .post(endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "调用大模型超时，请检查网络或接口地址".to_string()
            } else {
                format!("无法连接大模型接口：{}", e)
            }
        })?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取大模型响应失败：{}", e))?;

    if !status.is_success() {
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| text.chars().take(200).collect());
        return Err(match status.as_u16() {
            401 | 403 => format!("API Key 无效或无权限：{}", msg),
            404 => format!("接口地址或模型名不存在：{}", msg),
            429 => format!("请求过于频繁或额度不足：{}", msg),
            code => format!("大模型返回错误（{}）：{}", code, msg),
        });
    }

    serde_json::from_str::<Value>(&text).map_err(|e| format!("解析大模型响应失败：{}", e))
}

/// 本地执行工具。返回 (给模型看的 JSON 文本, 给前端展示的一行摘要)。
async fn run_tool(
    conn: &Arc<Mutex<mysql_async::Conn>>,
    database: &str,
    name: &str,
    args: &Value,
) -> (String, String) {
    match name {
        "search_tables" => {
            let keyword = args
                .get("keyword")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let mut guard = conn.lock().await;
            match mysql::fetch_tables(&mut guard, database, keyword).await {
                Ok(list) => {
                    let total = list.len();
                    let shown: Vec<Value> = list
                        .iter()
                        .take(SEARCH_LIMIT)
                        .map(|t| json!({ "name": t.table_name, "comment": t.table_comment }))
                        .collect();
                    let returned = shown.len();
                    let summary = match keyword {
                        Some(k) => format!("搜索「{}」命中 {} 张表", k, total),
                        None => format!("库共 {} 张表，返回前 {} 张", total, returned),
                    };
                    (
                        json!({ "total": total, "returned": returned, "tables": shown }).to_string(),
                        summary,
                    )
                }
                Err(e) => (json!({ "error": e }).to_string(), "查询表失败".to_string()),
            }
        }
        "get_table_columns" => {
            let table = args
                .get("table")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if table.is_empty() {
                return (
                    json!({ "error": "缺少 table 参数" }).to_string(),
                    "参数缺失".to_string(),
                );
            }
            let mut guard = conn.lock().await;
            match mysql::fetch_columns(&mut guard, database, &table).await {
                Ok(cols) => {
                    let list: Vec<Value> = cols
                        .iter()
                        .map(|c| {
                            json!({
                                "name": c.column_name,
                                "type": c.data_type,
                                "comment": c.column_comment
                            })
                        })
                        .collect();
                    let count = list.len();
                    (
                        json!({ "table": table, "columns": list }).to_string(),
                        format!("表 {} 返回 {} 个字段", table, count),
                    )
                }
                Err(e) => (
                    json!({ "error": e }).to_string(),
                    format!("查询表 {} 字段失败", table),
                ),
            }
        }
        other => (
            json!({ "error": format!("未知工具：{}", other) }).to_string(),
            "未知工具".to_string(),
        ),
    }
}

/// 测试配置是否可用：只发一次最小请求，不涉及数据库与工具。
#[tauri::command]
pub async fn ai_test_config(config: AiConfig) -> Result<String, String> {
    let api_key = config.api_key.trim().to_string();
    let model = config.model.trim().to_string();
    if api_key.is_empty() {
        return Err("请先填写 API Key".to_string());
    }
    if model.is_empty() {
        return Err("请先填写模型名称".to_string());
    }
    let endpoint = normalize_endpoint(&config.base_url)?;
    let timeout = config
        .timeout_secs
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败：{}", e))?;
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 1,
    });
    post_chat(&client, &endpoint, &api_key, &body).await?;
    Ok("连接成功".to_string())
}

/// 生成 SQL 的入口命令。返回 { sql, rounds, usage }。
/// 生成过程中的每一步通过 `ai:progress` 事件实时推送（前端逐条追加显示）。
#[tauri::command]
pub async fn ai_generate_sql(
    app: AppHandle,
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    prompt: String,
    config: AiConfig,
) -> Result<Value, String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("请先输入查询需求的描述".to_string());
    }
    let api_key = config.api_key.trim().to_string();
    let model = config.model.trim().to_string();
    if api_key.is_empty() {
        return Err("请先在 AI 设置中填写 API Key".to_string());
    }
    if model.is_empty() {
        return Err("请先在 AI 设置中填写模型名称".to_string());
    }
    let endpoint = normalize_endpoint(&config.base_url)?;
    let timeout = config
        .timeout_secs
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    let max_rounds = config
        .max_rounds
        .filter(|v| *v >= 2)
        .unwrap_or(DEFAULT_MAX_ROUNDS);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败：{}", e))?;

    let conn = state.get_mysql(&conn_id).await?;

    // 只取表总数用于提示（不传任何表名/结构）；动态内容放最后一条 user 消息。
    let total_tables = {
        let mut guard = conn.lock().await;
        mysql::fetch_tables(&mut guard, &database, None).await?.len()
    };

    let mut messages: Vec<Value> = vec![
        json!({ "role": "system", "content": SYSTEM_PROMPT }),
        json!({
            "role": "user",
            "content": format!(
                "当前数据库：{}\n该库共有 {} 张表。请通过工具按需查询表结构后生成 SQL。\n\n用户需求：{}",
                database, total_tables, prompt
            )
        }),
    ];

    let tools = tools_schema();
    let mut usage = json!({ "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 });
    let mut rounds: u8 = 0;
    let sql;

    loop {
        rounds += 1;
        // 到达上限：最后一次不带 tools，逼模型直接作答，防死循环。
        let force_answer = rounds >= max_rounds;
        // 每轮发请求前先推一条，前端显示「正在请求模型（第 N 轮）」
        let _ = app.emit(PROGRESS_EVENT, json!({ "phase": "thinking", "round": rounds }));
        let mut body = json!({
            "model": model,
            "messages": messages.clone(),
            "temperature": 0.1,
            "max_tokens": 2000,
        });
        if !force_answer {
            body["tools"] = tools.clone();
            body["tool_choice"] = json!("auto");
        }

        let think_started = Instant::now();
        let resp = post_chat(&client, &endpoint, &api_key, &body).await?;
        let think_ms = think_started.elapsed().as_millis() as u64;
        accumulate_usage(&mut usage, &resp);
        // 模型这一轮 HTTP 往返才是真正耗时所在，单独推一条，避免用户误以为「生成很慢却每步都 1ms」
        let _ = app.emit(
            PROGRESS_EVENT,
            json!({
                "phase": "step",
                "round": rounds,
                "tool": "大模型",
                "summary": format!("第 {} 轮请求", rounds),
                "elapsed_ms": think_ms,
            }),
        );

        let message = resp
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .cloned()
            .ok_or_else(|| "大模型返回格式异常：缺少 choices[0].message".to_string())?;

        let tool_calls = message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if !force_answer && !tool_calls.is_empty() {
            messages.push(message.clone());
            for call in &tool_calls {
                let name = call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let call_id = call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let args: Value = call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_else(|| json!({}));

                let started = Instant::now();
                let (content, summary) = run_tool(&conn, &database, &name, &args).await;
                // 该步执行完立刻推给前端。本地元数据查询多在几十毫秒内，
                // 按秒保留 1 位小数会被四舍五入成 0.0s，故传毫秒由前端决定显示单位。
                let elapsed_ms = started.elapsed().as_millis() as u64;
                let _ = app.emit(
                    PROGRESS_EVENT,
                    json!({
                        "phase": "step",
                        "round": rounds,
                        "tool": &name,
                        "args": &args,
                        "summary": &summary,
                        "elapsed_ms": elapsed_ms,
                    }),
                );
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content,
                }));
            }
            continue;
        }

        let cleaned = clean_sql(
            message
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        if cleaned.is_empty() {
            return Err("大模型未返回可用的 SQL，请调整描述后重试".to_string());
        }
        sql = cleaned;
        break;
    }

    Ok(json!({
        "sql": sql,
        "rounds": rounds,
        "usage": usage,
    }))
}
