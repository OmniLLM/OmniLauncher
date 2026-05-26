/// Cron expression explainer plugin — ported from Raycast `cron-description` extension concept.
/// Prefix: "cron "
/// Examples:
///   cron 0 9 * * 1-5    → "At 09:00, Monday through Friday"
///   cron */5 * * * *     → "Every 5 minutes"
///   cron 0 0 1 * *       → "At midnight, on day 1 of the month"
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct CronExplainerPlugin;

fn explain_cron(expr: &str) -> Option<String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let (minute, hour, dom, month, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    let minute_str = explain_field(minute, "minute", &minute_names());
    let hour_str = explain_field(hour, "hour", &hour_names());
    let dom_str = explain_dom(dom);
    let month_str = explain_field(month, "month", &month_names());
    let dow_str = explain_dow(dow);

    let mut parts = vec![];

    // Time
    if minute == "0" && hour != "*" {
        if let Ok(h) = hour.parse::<u32>() {
            parts.push(format!("At {:02}:00", h));
        }
    } else if minute != "*" || hour != "*" {
        let time_part = format!("{} past {}", minute_str, hour_str);
        parts.push(time_part);
    } else {
        parts.push("Every minute".to_string());
    }

    // DOM / month
    if dom != "*" {
        parts.push(dom_str);
    }
    if month != "*" {
        parts.push(format!("in {}", month_str));
    }
    if dow != "*" {
        parts.push(dow_str);
    }

    // Override with simple patterns
    let result = if expr == "* * * * *" {
        "Every minute".to_string()
    } else if minute.starts_with("*/") {
        let n = minute.trim_start_matches("*/");
        if hour == "*" && dom == "*" && month == "*" && dow == "*" {
            format!("Every {} minutes", n)
        } else {
            parts.join(", ")
        }
    } else if hour.starts_with("*/") {
        let n = hour.trim_start_matches("*/");
        format!("Every {} hours", n)
    } else {
        parts.join(", ")
    };

    Some(result)
}

fn minute_names() -> Vec<String> {
    (0..60).map(|i: u32| i.to_string()).collect()
}
fn hour_names() -> Vec<String> {
    (0..24).map(|i: u32| format!("{:02}:00", i)).collect()
}
fn month_names() -> Vec<String> {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
fn dow_names_full() -> Vec<&'static str> {
    vec![
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ]
}

fn explain_field(field: &str, unit: &str, names: &[String]) -> String {
    if field == "*" {
        return format!("every {}", unit);
    }
    if let Some(step) = field.strip_prefix("*/") {
        return format!("every {} {}s", step, unit);
    }
    if field.contains('-') {
        let parts: Vec<&str> = field.splitn(2, '-').collect();
        let from = resolve_name(parts[0], names);
        let to = resolve_name(parts.get(1).copied().unwrap_or(""), names);
        return format!("{} through {}", from, to);
    }
    if field.contains(',') {
        let items: Vec<String> = field.split(',').map(|p| resolve_name(p, names)).collect();
        return items.join(", ");
    }
    resolve_name(field, names)
}

fn resolve_name(val: &str, names: &[String]) -> String {
    if let Ok(n) = val.parse::<usize>() {
        names.get(n).cloned().unwrap_or_else(|| val.to_string())
    } else {
        val.to_string()
    }
}

fn explain_dom(dom: &str) -> String {
    if dom == "*" {
        return String::new();
    }
    if dom == "L" {
        return "on the last day of the month".to_string();
    }
    if let Some(w) = dom.strip_suffix('W') {
        return format!("on the nearest weekday to day {}", w);
    }
    format!("on day {} of the month", dom)
}

fn explain_dow(dow: &str) -> String {
    if dow == "*" {
        return String::new();
    }
    let full = dow_names_full();
    if dow.contains('-') {
        let parts: Vec<&str> = dow.splitn(2, '-').collect();
        let from = parts[0]
            .parse::<usize>()
            .ok()
            .and_then(|i| full.get(i))
            .copied()
            .unwrap_or(parts[0]);
        let to = parts
            .get(1)
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| full.get(i))
            .copied()
            .unwrap_or(parts.get(1).copied().unwrap_or(""));
        return format!("{} through {}", from, to);
    }
    if dow.contains(',') {
        let names: Vec<&str> = dow
            .split(',')
            .map(|p| {
                p.parse::<usize>()
                    .ok()
                    .and_then(|i| full.get(i))
                    .copied()
                    .unwrap_or(p)
            })
            .collect();
        return format!("on {}", names.join(", "));
    }
    let name = dow
        .parse::<usize>()
        .ok()
        .and_then(|i| full.get(i))
        .copied()
        .unwrap_or(dow);
    format!("on {}", name)
}

fn next_description(expr: &str) -> Option<String> {
    // Very basic next-run hint
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let (m, h) = (parts[0], parts[1]);
    if let ("0", h_val) = (m, h) {
        if let Ok(hour) = h_val.parse::<u32>() {
            return Some(format!("Next run: today or tomorrow at {:02}:00", hour));
        }
    }
    Some("Next run: depends on current time".to_string())
}

#[async_trait]
impl Plugin for CronExplainerPlugin {
    fn name(&self) -> &str {
        "cron_explainer"
    }

    fn description(&self) -> &str {
        "Explain cron expressions in plain English — cron */5 * * * *"
    }

    fn keyword(&self) -> Option<&str> {
        Some("cron ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let expr = q.raw.strip_prefix("cron ").unwrap_or("").trim();
        if expr.is_empty() {
            // Show examples
            return vec![
                mk_example("*/5 * * * *", "Every 5 minutes"),
                mk_example("0 9 * * 1-5", "Mon-Fri at 09:00"),
                mk_example("0 0 * * *", "Daily at midnight"),
                mk_example("0 0 1 * *", "Monthly on 1st"),
                mk_example("0 0 * * 0", "Every Sunday midnight"),
                mk_example("*/15 9-17 * * 1-5", "Every 15 min, 9am-5pm weekdays"),
            ];
        }

        let explanation = explain_cron(expr);
        let next_hint = next_description(expr);

        match explanation {
            Some(human) => {
                vec![QueryResult {
                    id: format!("cron:{}", expr),
                    title: format!("📅 {}", human),
                    subtitle: Some(next_hint.unwrap_or_else(|| format!("Expression: {}", expr))),
                    icon: Some("📅".to_string()),
                    score: 100,
                    action_type: "copy".to_string(),
                    action_data: human,
                }]
            }
            None => {
                vec![QueryResult {
                    id: "cron:invalid".to_string(),
                    title: "❌ Invalid cron expression".to_string(),
                    subtitle: Some("Format: min hour dom month dow (e.g. */5 * * * *)".to_string()),
                    icon: Some("❌".to_string()),
                    score: 50,
                    action_type: "none".to_string(),
                    action_data: String::new(),
                }]
            }
        }
    }
}

fn mk_example(expr: &str, desc: &str) -> QueryResult {
    QueryResult {
        id: format!("cron:eg:{}", expr),
        title: format!("📅 {} — {}", expr, desc),
        subtitle: Some("Press Enter to copy expression".to_string()),
        icon: Some("📅".to_string()),
        score: 60,
        action_type: "copy".to_string(),
        action_data: expr.to_string(),
    }
}
