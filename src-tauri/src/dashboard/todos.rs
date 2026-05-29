//! Todos dashboard.
//!
//! Serves the rich interactive todo browser (formerly hosted at `/todo`)
//! merged in under `/dashboard/todos`. The legacy `/todo` route has been
//! decommissioned in favor of this single entry point.

use super::common::render_page;
use crate::plugins::todo;

pub fn todos_html() -> String {
    let raw_html = todo::todo_live_html();

    // Extract the inner HTML inside <body>...</body> and <script>...</script>
    let body_start = raw_html.find("<body>").map(|i| i + 6).unwrap_or(0);
    let body_end = raw_html.find("</body>").unwrap_or(raw_html.len());
    let mut body_content = raw_html[body_start..body_end].to_string();

    // Remove the `<div class="container">` and matching trailing `</div>` and `<header>...</header>`
    // to match other dashboard pages beautifully. we will wrap page contents nicely!
    if let Some(h_start) = body_content.find("<header>") {
        if let Some(h_end) = body_content.find("</header>") {
            body_content.replace_range(h_start..h_end + 9, "");
        }
    }
    if body_content.starts_with("\n<div class=\"container\">") {
        body_content = body_content.replacen("\n<div class=\"container\">", "", 1);
        if let Some(last_div) = body_content.rfind("</div>") {
            body_content.replace_range(last_div..last_div + 6, "");
        }
    } else if body_content.contains("<div class=\"container\">") {
        body_content = body_content.replace("<div class=\"container\">", "");
        if let Some(last_div) = body_content.rfind("</div>") {
            body_content.replace_range(last_div..last_div + 6, "");
        }
    }

    let script_start = raw_html.find("<script>").map(|i| i + 8).unwrap_or(0);
    let script_end = raw_html.find("</script>").unwrap_or(raw_html.len());
    let script_content = raw_html[script_start..script_end].to_string();

    let title_html = r#"<h1 class="page-title">📝 Todos Dashboard</h1>"#;
    let combined_body = format!("{}\n{}", title_html, body_content);

    render_page("Todos", "todos", &combined_body, &script_content)
}

pub fn todos_data_json() -> String {
    todo::todo_live_data_json()
}
