// CSV 导入：文本解码（UTF-8 / GB18030）、RFC4180 解析、预览、批量 INSERT
use encoding_rs::{GB18030, UTF_8};
use tauri::State;

use crate::models::{CsvImportOptions, CsvImportResult, CsvPreview};
use crate::state::AppState;

/// 预览时允许读取的文件上限，避免一次把超大文件读进内存
const MAX_PREVIEW_BYTES: u64 = 128 * 1024 * 1024;

/// 单次导入允许的文件上限
const MAX_IMPORT_BYTES: u64 = 512 * 1024 * 1024;

/// 失败明细最多保留的条数
const MAX_ERROR_SAMPLES: usize = 20;

// ===== 解码 =====

fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.len() >= 3 && &bytes[..3] == [0xEF, 0xBB, 0xBF] {
        &bytes[3..]
    } else {
        bytes
    }
}

/// 解码文件字节，返回（文本, 实际使用的编码名）。
/// encoding 传 "auto" 时：合法 UTF-8 即按 UTF-8，否则回退 GB18030。
pub(crate) fn decode_bytes(bytes: &[u8], encoding: &str) -> (String, &'static str) {
    let body = strip_bom(bytes);
    match encoding.to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => match std::str::from_utf8(body) {
            Ok(s) => (s.to_string(), "utf-8"),
            Err(_) => {
                // 强制 UTF-8 时用替换字符兜底，避免整个文件读不出来
                let (s, _, _) = UTF_8.decode(body);
                (s.into_owned(), "utf-8")
            }
        },
        "gbk" | "gb2312" | "gb18030" => {
            let (s, _, _) = GB18030.decode(body);
            (s.into_owned(), "gb18030")
        }
        _ => {
            if let Ok(s) = std::str::from_utf8(body) {
                return (s.to_string(), "utf-8");
            }
            let (s, _, _) = GB18030.decode(body);
            (s.into_owned(), "gb18030")
        }
    }
}

// ===== CSV 解析 =====

/// RFC4180 风格解析：双引号包裹字段、`""` 转义、字段内可含分隔符与换行；CRLF / LF 均可
pub(crate) fn parse_csv(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        if c == '"' && field.is_empty() {
            in_quotes = true;
        } else if c == delimiter {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' {
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else if c != '\r' {
            field.push(c);
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// 统计单行中位于引号外的某个字符出现次数
fn count_outside_quotes(line: &str, target: char) -> usize {
    let mut in_quotes = false;
    let mut n = 0;
    for ch in line.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && ch == target {
            n += 1;
        }
    }
    n
}

/// 首行里出现次数最多的候选分隔符即判定为分隔符；都不出现时按逗号处理
pub(crate) fn detect_delimiter(text: &str) -> char {
    let first = text.lines().next().unwrap_or("");
    let mut best = (',', 0usize);
    for c in [',', '\t', ';', '|'] {
        let n = count_outside_quotes(first, c);
        if n > best.1 {
            best = (c, n);
        }
    }
    best.0
}

/// 把用户指定的分隔符（auto / tab / 具体字符）解析成实际字符
fn resolve_delimiter(spec: Option<&str>, text: &str) -> char {
    match spec.map(|s| s.trim()) {
        None | Some("") | Some("auto") => detect_delimiter(text),
        Some("tab") | Some("\\t") => '\t',
        Some(other) => other.chars().next().unwrap_or(','),
    }
}

// ===== 预览 =====

#[tauri::command]
pub fn csv_preview(
    path: String,
    encoding: Option<String>,
    delimiter: Option<String>,
    has_header: bool,
    limit: usize,
) -> Result<CsvPreview, String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_PREVIEW_BYTES {
        return Err(format!(
            "文件过大（{} MB），暂时无法预览",
            meta.len() / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let (text, enc) = decode_bytes(&bytes, encoding.as_deref().unwrap_or("auto"));
    let delim = resolve_delimiter(delimiter.as_deref(), &text);
    let all = parse_csv(&text, delim);

    // 列数取所有行的最大值，短行在返回前补齐，保证前端映射表对齐
    let columns = all.iter().map(|r| r.len()).max().unwrap_or(0);
    let (mut headers, data): (Vec<String>, &[Vec<String>]) = if has_header && !all.is_empty() {
        (all[0].clone(), &all[1..])
    } else {
        (Vec::new(), &all[..])
    };
    while headers.len() < columns {
        headers.push(format!("列{}", headers.len() + 1));
    }

    let lim = limit.clamp(1, 500);
    let rows: Vec<Vec<String>> = data
        .iter()
        .take(lim)
        .map(|r| {
            let mut v = r.clone();
            v.resize(columns, String::new());
            v
        })
        .collect();

    Ok(CsvPreview {
        headers,
        rows,
        total_rows: data.len(),
        encoding: enc.to_string(),
        delimiter: delim.to_string(),
        columns,
    })
}

// ===== 导入 =====

/// mapping：下标对应 CSV 列序，值为目标字段名（None / 空串表示该列不导入）
#[tauri::command]
pub async fn db_import_csv(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
    path: String,
    mapping: Vec<Option<String>>,
    options: CsvImportOptions,
) -> Result<CsvImportResult, String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(format!(
            "文件过大（{} MB），超过导入上限 512 MB",
            meta.len() / 1024 / 1024
        ));
    }

    let cols: Vec<(usize, String)> = mapping
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            m.as_ref()
                .filter(|c| !c.trim().is_empty())
                .map(|c| (i, c.clone()))
        })
        .collect();
    if cols.is_empty() {
        return Err("请至少为一个 CSV 列选择目标字段".to_string());
    }

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let (text, _) = decode_bytes(&bytes, options.encoding.as_deref().unwrap_or("auto"));
    let delim = resolve_delimiter(options.delimiter.as_deref(), &text);
    let mut rows = parse_csv(&text, delim);

    if options.has_header && !rows.is_empty() {
        rows.remove(0);
    }
    if rows.is_empty() {
        return Err("CSV 中没有可导入的数据行".to_string());
    }

    let null_value = options.null_value.clone().unwrap_or_default();
    let empty_as_null = options.empty_as_null.unwrap_or(false);
    let stop_on_error = options.stop_on_error.unwrap_or(true);
    let truncate_first = options.truncate_first.unwrap_or(false);
    let batch_size = options.batch_size.unwrap_or(500).clamp(1, 5000);

    let col_names: Vec<String> = cols.iter().map(|(_, c)| c.clone()).collect();

    let driver = state.get_driver(&conn_id).await?;

    if truncate_first {
        driver
            .truncate_table(&database, &table)
            .await
            .map_err(|e| format!("清空表失败：{}", e))?;
    }

    // 把一批 CSV 行映射为与 col_names 对齐的单元格（None 表示 SQL NULL）
    let to_batch = |slice: &[Vec<String>]| -> Vec<Vec<Option<String>>> {
        slice
            .iter()
            .map(|row| {
                cols.iter()
                    .map(|(idx, _)| match row.get(*idx) {
                        None => None,
                        Some(s) => {
                            if !null_value.is_empty() && s == &null_value {
                                None
                            } else if empty_as_null && s.is_empty() {
                                None
                            } else {
                                Some(s.clone())
                            }
                        }
                    })
                    .collect()
            })
            .collect()
    };

    let mut inserted: u64 = 0;
    let mut failed: u64 = 0;
    let mut errors: Vec<String> = Vec::new();
    let total = rows.len();
    let mut start = 0usize;

    while start < total {
        let end = (start + batch_size).min(total);
        let slice = &rows[start..end];
        let batch = to_batch(slice);
        let batch_result = driver
            .insert_rows(&database, &table, &col_names, &batch)
            .await;

        match batch_result {
            Ok(()) => inserted += (end - start) as u64,
            Err(e) => {
                if stop_on_error {
                    return Err(format!("第 {} 行导入失败：{}", start + 1, e));
                }
                // 跳过错误行模式：退回逐行插入，精确定位坏行
                for (off, row) in slice.iter().enumerate() {
                    let one = std::slice::from_ref(row);
                    let one_batch = to_batch(one);
                    match driver
                        .insert_rows(&database, &table, &col_names, &one_batch)
                        .await
                    {
                        Ok(()) => inserted += 1,
                        Err(er) => {
                            failed += 1;
                            if errors.len() < MAX_ERROR_SAMPLES {
                                errors.push(format!("第 {} 行：{}", start + off + 1, er));
                            }
                        }
                    }
                }
            }
        }
        start = end;
    }

    Ok(CsvImportResult {
        inserted,
        failed,
        errors,
        truncated: truncate_first,
    })
}
