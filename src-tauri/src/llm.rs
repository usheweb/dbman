//! 第一期：大模型生成 SQL。
//!
//! 设计要点（对齐 tslclock 的 IntentParser）：
//! - 请求走 Rust（浏览器直连会被 CORS 挡，密钥也不该进 JS）；二期只需换 base_url 加鉴权头。
//! - **不向模型预传任何表名/表结构**：一次生成通常只用到 1~2 张表，全量表名绝大多数是冗余。
//!   模型必须先用 `search_tables` 找候选表，再用 `get_table_columns` 取字段。
//! - system prompt 保持纯静态（利于上游前缀缓存），动态内容（当前时间/库名/表总数/需求）放最后一条 user 消息。
//! - 工具结果只给「表名 + 注释 + 字段名 + 类型 + 注释」，不给任何真实数据行。
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, Weekday};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::db::RelationalDriver;
use crate::state::{AiSession, AppState};

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_MAX_ROUNDS: u8 = 6;
/// search_tables 无关键词时最多返回的表数量（超出只给总数）。
const SEARCH_LIMIT: usize = 50;
/// 「继续微调」会话的有效期：超期后当作全新会话。
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);
/// 同时保留的会话数量上限，超出按最久未使用淘汰。
const MAX_SESSIONS: usize = 20;
/// 会话消息裁剪时保留的尾部条数（过长会拖慢请求、浪费 token）。
const KEEP_TAIL: usize = 8;
/// 生成过程中的实时进度事件：每完成一步就推一条，前端不必等整轮结束才看到结果。
const PROGRESS_EVENT: &str = "ai:progress";

/// system prompt 按驱动方言生成（除方言外的文字保持纯静态，只含角色与规则，
/// 不含库名/表名，这样每次请求的前缀基本一致，可命中上游大模型的前缀缓存）。
/// 新增驱动时在此登记「方言名 + 标识符包裹说明」。
fn system_prompt(kind: &str) -> String {
    let (dialect, ident_rule) = match kind {
        // 后续接入 PostgreSQL / SQLite / SQL Server / Oracle 时在此补充各自方言
        _ => ("MySQL", "表名与列名用反引号包裹"),
    };
    format!(
        "\
你是资深 {dialect} 数据库工程师，负责把用户的自然语言需求转换成一条可直接执行的 SQL。

工作方式（必须遵守）：
1. 你**看不到**数据库里的任何表名和字段，严禁凭猜测编造表名或列名。
   用户需求里以 @ 开头的是他指定的表名，请优先使用这些表（仍需用 get_table_columns 取字段）。
2. 生成 SQL 前，必须先调用 search_tables 找到相关的表；不确定表名时用简短关键词搜索。
3. 确定候选表后，调用 get_table_columns 获取这些表的真实字段（名称/类型/注释）；只能使用工具返回过的表名与列名。
   若过滤条件涉及某个字段的具体取值，按第 5 条处理。
4. 一次需求通常只涉及 1~2 张表，只查询你确实需要用到的表，不要索取全部表结构。
5. 当需求里的词可能是某张表字段的具体取值（渠道名、市场名、状态名等）时，先调用 distinct_values 查该字段的真实取值，拿到准确值再写过滤条件，不要凭字面猜测。

输出要求：
1. 只输出一条 SQL 语句本身，不要任何解释文字，不要使用 ``` 代码围栏。
2. 使用 {dialect} 语法，{ident_rule}。
3. 除非用户明确要求，不要生成 INSERT / UPDATE / DELETE / DROP / ALTER 等改动数据或结构的语句。
4. 若需求里的时间范围对应的是时间戳字段，按用户描述的时间语义处理。
5. 需求里出现「今天/昨天/本周/最近 N 天」等相对时间时，一律以用户消息里给出的「当前时间」为基准换算，禁止使用你自己的训练日期。"
    )
}

/// 当前本地时间（含星期）。动态内容，注入到最后一条 user 消息里，
/// 否则模型会用训练数据里的日期去算「今天」，导致生成的时间条件完全错误。
fn now_text() -> String {
    let now = Local::now();
    let weekday = match now.weekday() {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    };
    format!("{} {}", now.format("%Y-%m-%d %H:%M:%S"), weekday)
}

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
        },
        {
            "type": "function",
            "function": {
                "name": "distinct_values",
                "description": "查询某张表某个字段的去重取值（可选关键词模糊过滤），用于确认需求里的词（如渠道名、市场名、状态名）对应的真实数据值，从而写对 WHERE 条件。只返回该字段的取值，不返回其他数据。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "table": { "type": "string", "description": "表名，必须是 search_tables 返回过的真实表名" },
                        "column": { "type": "string", "description": "字段名，必须是 get_table_columns 返回过的真实字段" },
                        "keyword": { "type": "string", "description": "取值关键词，可留空表示取前若干个值" },
                        "limit": { "type": "integer", "description": "最多返回多少条，默认 30，最大 100" }
                    },
                    "required": ["table", "column"]
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

/// 到达轮数上限后不再下发 tools，但模型仍可能把「工具调用」当成正文吐出来，形如
/// `<｜｜DSML｜｜invoke name="search_tables">…</｜｜DSML｜｜invoke>`。
/// 这不是 SQL，直接写进编辑器用户只会看到一堆乱码，必须按无效结果拦掉。
fn looks_like_tool_call(text: &str) -> bool {
    // "\u{FF5C}" 是全角竖线，在模型特殊标记里成对出现，正常 SQL 不会用到
    text.contains("DSML") || text.contains("\u{FF5C}\u{FF5C}") || text.contains("invoke name=")
}

/// 会话消息裁剪：保留 system + 第一条 user（含库名与原始需求），
/// 尾部最多 KEEP_TAIL 条，且尾部起点必须回退到 role=user 的消息，
/// 否则可能切断 assistant(tool_calls) 与其 tool 响应的配对。
fn trim_messages(msgs: &mut Vec<Value>) {
    if msgs.len() <= 2 + KEEP_TAIL {
        return;
    }
    let mut start = msgs.len() - KEEP_TAIL;
    while start > 2 && msgs[start].get("role").and_then(|r| r.as_str()) != Some("user") {
        start -= 1;
    }
    let tail: Vec<Value> = msgs[start..].to_vec();
    msgs.truncate(2);
    msgs.extend(tail);
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
    driver: &Arc<dyn RelationalDriver>,
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
            match driver.tables(database, keyword).await {
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
            match driver.columns(database, &table).await {
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
        "distinct_values" => {
            let get = |k: &str| {
                args.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            };
            let table = get("table");
            let column = get("column");
            if table.is_empty() || column.is_empty() {
                return (
                    json!({ "error": "缺少 table 或 column 参数" }).to_string(),
                    "参数缺失".to_string(),
                );
            }
            let keyword = args
                .get("keyword")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let limit = args
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(30) as usize;
            match driver
                .distinct_values(database, &table, &column, keyword, limit)
                .await
            {
                Ok((values, truncated)) => {
                    let count = values.len();
                    let summary = format!(
                        "表 {} 字段 {} 取到 {} 个值{}",
                        table,
                        column,
                        count,
                        if truncated { "（已截断）" } else { "" }
                    );
                    (
                        json!({
                            "table": table,
                            "column": column,
                            "keyword": keyword,
                            "total": count,
                            "truncated": truncated,
                            "values": values,
                        })
                        .to_string(),
                        summary,
                    )
                }
                Err(e) => (
                    json!({ "error": e }).to_string(),
                    format!("查询表 {} 字段 {} 取值失败", table, column),
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

/// 生成 SQL 的入口命令。返回 { sql, rounds, usage, sessionId, turn, hasSession }。
/// 生成过程中的每一步通过 `ai:progress` 事件实时推送（前端逐条追加显示）。
/// continue_session 为 true 时在上一次的会话上下文基础上继续（追问式微调）。
#[tauri::command]
pub async fn ai_generate_sql(
    app: AppHandle,
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    prompt: String,
    config: AiConfig,
    continue_session: Option<bool>,
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

    let driver = state.get_driver(&conn_id).await?;

    // 只取表总数用于提示（不传任何表名/结构）；动态内容放最后一条 user 消息。
    let total_tables = driver.tables(&database, None).await?.len();

    // 追问式微调：按「连接 + 库」保留会话上下文，继续时在其基础上追加一条 user 消息。
    let key = format!("{}::{}", conn_id, database);
    let mut turn: u32 = 1;
    let reused: Option<Vec<Value>> = {
        let mut sessions = state.ai_sessions.lock().await;
        // 惰性清理：过期淘汰 + 数量上限（超限删最久未使用的）
        sessions.retain(|_, s| s.touched.elapsed() < SESSION_TTL);
        while sessions.len() >= MAX_SESSIONS {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, s)| s.touched)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    sessions.remove(&k);
                }
                None => break,
            }
        }
        if continue_session.unwrap_or(false) {
            sessions.get(&key).map(|s| {
                turn = s.turn + 1;
                s.messages.clone()
            })
        } else {
            None
        }
    };

    let mut messages: Vec<Value> = reused.unwrap_or_default();
    if messages.is_empty() {
        messages = vec![
            json!({ "role": "system", "content": system_prompt(driver.kind()) }),
            json!({
                "role": "user",
                "content": format!(
                    "当前时间：{}\n当前数据库：{}\n该库共有 {} 张表。请通过工具按需查询表结构后生成 SQL。\n\n用户需求：{}",
                    now_text(), database, total_tables, prompt
                )
            }),
        ];
        turn = 1;
    } else {
        // 追问也要带上最新时间：会话可能跨天，旧的「当前时间」已过期
        messages.push(json!({
            "role": "user",
            "content": format!("当前时间：{}。用户补充/修正：{}", now_text(), prompt)
        }));
    }
    trim_messages(&mut messages);

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
                let (content, summary) = run_tool(&driver, &database, &name, &args).await;
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
        // 收回工具后模型有时仍把工具调用当正文吐出来，这种内容不是 SQL，不能写进编辑器
        if cleaned.is_empty() || looks_like_tool_call(&cleaned) {
            return Err(if force_answer {
                format!(
                    "已达工具调用轮数上限（{} 轮），模型仍未给出 SQL。\
                     请把需求描述得更具体，或直接写明涉及的表名/字段后重试",
                    max_rounds
                )
            } else {
                "大模型未返回可用的 SQL，请调整描述后重试".to_string()
            });
        }
        sql = cleaned;
        // 把最终答案也留在上下文里，追问时模型知道上一版长什么样
        messages.push(json!({ "role": "assistant", "content": sql.as_str() }));
        state.ai_sessions.lock().await.insert(
            key.clone(),
            AiSession {
                messages: messages.clone(),
                turn,
                touched: Instant::now(),
            },
        );
        break;
    }

    Ok(json!({
        "sql": sql,
        "rounds": rounds,
        "usage": usage,
        "sessionId": key,
        "turn": turn,
        "hasSession": true,
    }))
}
