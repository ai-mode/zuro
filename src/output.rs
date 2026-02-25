use crate::constants::{
    TABLE_WIDTH, COL_EXCHANGE_ID, COL_TIMESTAMP, COL_MODEL, COL_TOKENS, COL_DURATION,
    SESSION_ID_PREFIX_LEN, MESSAGE_ID_PREFIX_LEN,
    RFC3339_SECONDS_LEN, RFC3339_TIME_OFFSET,
};
use crate::provider::Usage;
use crate::session::{Exchange, SessionStats};

fn print_text_response(content: &str, model: &str, usage: Option<&Usage>, session_id: &str, show_stats: bool) {
    print!("{content}");
    if !content.ends_with('\n') { println!(); }
    if show_stats {
        if let Some(u) = usage {
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            let sid = &session_id[..SESSION_ID_PREFIX_LEN.min(session_id.len())];
            eprintln!("[tokens in={inp} out={out} | model={model} | session={sid}]");
        }
    }
}

fn print_json_response(content: &str, model: &str, usage: Option<&Usage>, session_id: &str) {
    let val = serde_json::json!({
        "answer":     content,
        "session_id": session_id,
        "model":      model,
        "usage": usage.map(|u| serde_json::json!({
            "input_tokens":  u.input_tokens,
            "output_tokens": u.output_tokens,
        })),
    });
    println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
}

pub fn print_response(
    content:    &str,
    model:      &str,
    usage:      Option<&Usage>,
    json_mode:  bool,
    session_id: &str,
    show_stats: bool,
) {
    if json_mode {
        print_json_response(content, model, usage, session_id);
    } else {
        print_text_response(content, model, usage, session_id, show_stats);
    }
}

pub fn print_session_show(exchanges: &[Exchange], format: &str) {
    match format {
        "chat"  => print_show_chat(exchanges),
        "table" => print_show_table(exchanges),
        "json"  => print_show_json(exchanges),
        _       => print_show_text(exchanges),
    }
}

fn display_id(ex: &Exchange) -> String {
    let xid = ex.meta.exchange_id.as_deref()
        .map(|s| &s[..MESSAGE_ID_PREFIX_LEN.min(s.len())]);
    let mid = &ex.message_id[..MESSAGE_ID_PREFIX_LEN.min(ex.message_id.len())];
    match xid {
        Some(x) => format!("{x}@{mid}"),
        None    => mid.to_string(),
    }
}

fn print_show_text(exchanges: &[Exchange]) {
    for ex in exchanges {
        let ts   = &ex.ts[..RFC3339_SECONDS_LEN.min(ex.ts.len())];
        let role = format!("{:>9}", ex.role);
        let id   = display_id(ex);
        println!("[{ts}] {id}  {role}: {}", ex.content);
    }
}

fn print_show_chat(exchanges: &[Exchange]) {
    for ex in exchanges {
        let end = RFC3339_SECONDS_LEN.min(ex.ts.len());
        let ts  = ex.ts.get(RFC3339_TIME_OFFSET..end).unwrap_or(&ex.ts);
        let id  = display_id(ex);
        println!("{id} | {ts} | {}", ex.role);
        println!("{}", ex.content);
        println!();
    }
}

fn print_show_table(exchanges: &[Exchange]) {
    let term_width = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(120);

    let col_width = (term_width.saturating_sub(3)) / 2;

    let pairs = pair_exchanges(exchanges);

    let border_top    = format!("┌{}┬{}┐", "─".repeat(col_width), "─".repeat(col_width));
    let border_mid    = format!("├{}┼{}┤", "─".repeat(col_width), "─".repeat(col_width));
    let border_bottom = format!("└{}┴{}┘", "─".repeat(col_width), "─".repeat(col_width));

    println!("{border_top}");
    let first_user_id = pairs.first().and_then(|(u, _)| u.as_ref()).map(|e| display_id(e)).unwrap_or_default();
    let first_asst_id = pairs.first().and_then(|(_, a)| a.as_ref()).map(|e| display_id(e)).unwrap_or_default();
    println!("│{:<col_width$}│{:<col_width$}│",
        format!(" {first_user_id} | user"),
        format!(" {first_asst_id} | assistant"),
    );

    for (i, (user, asst)) in pairs.iter().enumerate() {
        if i > 0 { println!("{border_mid}"); }

        let user_lines = user.as_ref()
            .map(|e| wrap_lines(&e.content, col_width.saturating_sub(2)))
            .unwrap_or_default();
        let asst_lines = asst.as_ref()
            .map(|e| wrap_lines(&e.content, col_width.saturating_sub(2)))
            .unwrap_or_default();

        let user_id = user.as_ref().map(|e| display_id(e)).unwrap_or_default();
        let asst_id = asst.as_ref().map(|e| display_id(e)).unwrap_or_default();

        if i > 0 {
            println!("│{:<col_width$}│{:<col_width$}│",
                format!(" {user_id} | user"),
                format!(" {asst_id} | assistant"),
            );
        }

        let row_height = user_lines.len().max(asst_lines.len());
        for r in 0..row_height {
            let u = user_lines.get(r).map(|s| format!(" {s}")).unwrap_or_default();
            let a = asst_lines.get(r).map(|s| format!(" {s}")).unwrap_or_default();
            println!("│{u:<col_width$}│{a:<col_width$}│");
        }
    }
    println!("{border_bottom}");
}

fn print_show_json(exchanges: &[Exchange]) {
    println!("{}", serde_json::to_string_pretty(exchanges).unwrap_or_default());
}

fn pair_exchanges(exchanges: &[Exchange]) -> Vec<(Option<&Exchange>, Option<&Exchange>)> {
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < exchanges.len() {
        let current = &exchanges[i];
        if current.role == "user" {
            let asst = if i + 1 < exchanges.len() && exchanges[i + 1].role == "assistant" {
                i += 1;
                Some(&exchanges[i])
            } else {
                None
            };
            pairs.push((Some(current), asst));
        } else {
            pairs.push((None, Some(current)));
        }
        i += 1;
    }
    pairs
}

fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    if width == 0 { return vec![text.to_string()]; }
    let mut lines = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut start = 0;
        let chars: Vec<char> = raw_line.chars().collect();
        while start < chars.len() {
            let end = (start + width).min(chars.len());
            lines.push(chars[start..end].iter().collect());
            start = end;
        }
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

pub fn format_duration(ms: u64) -> String {
    if ms == 0 { return "-".into(); }
    let secs = ms as f64 / 1000.0;
    let mins = ms / 60_000;
    if mins > 0 {
        let rem = (ms % 60_000) as f64 / 1000.0;
        format!("{mins}m {rem:06.3}s")
    } else {
        format!("{secs:.3}s")
    }
}

pub fn print_session_stats(stats: &SessionStats) {
    if stats.exchanges.is_empty() {
        eprintln!("No token data recorded for this session.");
        return;
    }
    eprintln!("{:<COL_EXCHANGE_ID$} {:<COL_TIMESTAMP$} {:<COL_MODEL$} {:>COL_TOKENS$} {:>COL_TOKENS$} {:>COL_DURATION$}",
        "Exchange", "Timestamp", "Model", "In", "Out", "Duration");
    eprintln!("{}", "─".repeat(TABLE_WIDTH));
    for ex in &stats.exchanges {
        let xid = ex.exchange_id.as_deref().unwrap_or("-");
        eprintln!(
            "{:<COL_EXCHANGE_ID$} {:<COL_TIMESTAMP$} {:<COL_MODEL$} {:>COL_TOKENS$} {:>COL_TOKENS$} {:>COL_DURATION$}",
            xid,
            &ex.ts[..COL_TIMESTAMP.min(ex.ts.len())],
            &ex.model[..COL_MODEL.min(ex.model.len())],
            ex.input_tokens,
            ex.output_tokens,
            format_duration(ex.duration_ms.unwrap_or(0)),
        );
    }
    eprintln!("{}", "─".repeat(TABLE_WIDTH));
    eprintln!("{:<COL_EXCHANGE_ID$} {:<COL_TIMESTAMP$} {:<COL_MODEL$} {:>COL_TOKENS$} {:>COL_TOKENS$} {:>COL_DURATION$}",
        "TOTAL", "", "", stats.total_input, stats.total_output,
        format_duration(stats.total_dur_ms));
}
