//! Output system for the `ol` CLI.
//!
//! Every command funnels through this module so human output reads as one
//! consistent visual system and machine output (`--json`) is uniform. Colors
//! use an internal ANSI helper ported from the `scripts/ops.sh` palette (no new
//! color crate). Color/decoration is auto-suppressed when stdout is not a TTY
//! or `NO_COLOR` is set (the NO_COLOR standard), and forced off with
//! `--no-color`.

use omnilauncher_lib::QueryResult;
use std::io::IsTerminal;

/// Presentation options resolved once from the global CLI flags + environment.
/// Threaded through every command so a single place decides color/JSON/quiet.
#[derive(Debug, Clone, Copy)]
pub struct Output {
    /// Emit machine-readable JSON instead of decorated text.
    pub json: bool,
    /// Whether ANSI color/glyph decoration is enabled.
    pub color: bool,
    /// Errors-only: suppress success chrome and headers.
    pub quiet: bool,
}

impl Default for Output {
    fn default() -> Self {
        Output {
            json: false,
            color: true,
            quiet: false,
        }
    }
}

impl Output {
    /// Resolve presentation from the parsed global flags.
    ///
    /// Color precedence (most to least authoritative):
    ///   1. `--json`            → never colorize (structured output)
    ///   2. `--no-color` flag   → off
    ///   3. `NO_COLOR` env set  → off (https://no-color.org/)
    ///   4. stdout not a TTY    → off (piped / redirected)
    ///   5. otherwise           → on
    pub fn resolve(json: bool, no_color: bool, quiet: bool) -> Self {
        let color = !json
            && !no_color
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal();
        Output { json, color, quiet }
    }
}

// ── ANSI palette (ported from scripts/ops.sh) ────────────────────────────────
const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const CYAN: &str = "\x1b[0;36m";
const DIM: &str = "\x1b[2m";
const NC: &str = "\x1b[0m";

impl Output {
    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("{code}{s}{NC}")
        } else {
            s.to_string()
        }
    }

    pub fn green(&self, s: &str) -> String {
        self.paint(GREEN, s)
    }
    pub fn red(&self, s: &str) -> String {
        self.paint(RED, s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.paint(YELLOW, s)
    }
    pub fn cyan(&self, s: &str) -> String {
        self.paint(CYAN, s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.paint(DIM, s)
    }

    /// A success line: `✓ <msg>` in green (ASCII `+` under `--no-color`).
    /// Suppressed entirely under `--quiet`.
    pub fn success(&self, msg: &str) {
        if self.quiet {
            return;
        }
        let glyph = if self.color { "✓" } else { "+" };
        println!("{} {}", self.green(glyph), msg);
    }

    /// A failure line: `✗ <msg>` in red (ASCII `x` under `--no-color`).
    /// Always written to **stderr** so it survives `--quiet` and stdout redirection.
    pub fn failure(&self, msg: &str) {
        let glyph = if self.color { "✗" } else { "x" };
        eprintln!("{} {}", self.red(glyph), msg);
    }

    /// An informational line (suppressed under `--quiet`).
    pub fn info(&self, msg: &str) {
        if self.quiet {
            return;
        }
        println!("{msg}");
    }

    /// A status glyph for an up/down/error tri-state.
    /// `●` green (up) · `○` dim (down) · `●` red (error).
    /// Falls back to `[OK]` / `[--]` / `[XX]` when color is disabled.
    pub fn glyph(&self, state: Status) -> String {
        match (self.color, state) {
            (true, Status::Up) => self.green("●"),
            (true, Status::Down) => self.dim("○"),
            (true, Status::Error) => self.red("●"),
            (false, Status::Up) => "[OK]".to_string(),
            (false, Status::Down) => "[--]".to_string(),
            (false, Status::Error) => "[XX]".to_string(),
        }
    }
}

/// Tri-state used by status glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Up,
    Down,
    Error,
}

/// Render an `AiResponse`-shaped result to stdout: a table when the response
/// carries launcher `results`, otherwise the free-text `content`. Kept generic
/// over the pieces so callers don't need to import `AiResponse` here.
pub fn render_results(out: &Output, content: &str, results: &[QueryResult]) {
    if out.json {
        // Serialized by the caller for the AiResponse case; this path handles
        // the plain (content, results) shape used by search/query.
        let payload = serde_json::json!({
            "content": content,
            "results": results,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }

    if !results.is_empty() {
        render_result_table(out, results);
        return;
    }
    // Scalar/text content. Trim a single trailing newline for tidy output.
    let trimmed = content.trim_end_matches('\n');
    if !trimmed.is_empty() {
        println!("{trimmed}");
    }
}

/// Render launcher `QueryResult` rows as an aligned two-column table
/// (title, subtitle) with a dim trailing count line.
pub fn render_result_table(out: &Output, results: &[QueryResult]) {
    let rows: Vec<(String, String)> = results
        .iter()
        .map(|r| (r.title.clone(), r.subtitle.clone().unwrap_or_default()))
        .collect();

    let title_w = rows
        .iter()
        .map(|(t, _)| display_width(t))
        .max()
        .unwrap_or(0);

    for (title, subtitle) in &rows {
        if subtitle.is_empty() {
            println!("  {title}");
        } else {
            let pad = " ".repeat(title_w.saturating_sub(display_width(title)));
            println!("  {title}{pad}   {}", out.dim(subtitle));
        }
    }

    let n = results.len();
    let noun = if n == 1 { "result" } else { "results" };
    println!("  {}", out.dim(&format!("{n} {noun}")));
}

/// Approximate display width: counts `char`s, treating the handful of wide
/// glyphs we emit as width-2. Good enough for CLI table alignment without
/// pulling in a unicode-width crate.
fn display_width(s: &str) -> usize {
    s.chars().map(|c| if is_wide(c) { 2 } else { 1 }).sum()
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 |
        0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 |
        0xFFE0..=0xFFE6 | 0x1F300..=0x1FAFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Output {
        Output {
            json: false,
            color: false,
            quiet: false,
        }
    }

    #[test]
    fn no_color_glyphs_fall_back_to_ascii() {
        let o = plain();
        assert_eq!(o.glyph(Status::Up), "[OK]");
        assert_eq!(o.glyph(Status::Down), "[--]");
        assert_eq!(o.glyph(Status::Error), "[XX]");
    }

    #[test]
    fn no_color_paints_are_plain() {
        let o = plain();
        assert_eq!(o.green("x"), "x");
        assert_eq!(o.red("x"), "x");
        // color path wraps in ANSI codes
        let c = Output {
            color: true,
            ..plain()
        };
        assert!(c.green("x").contains("\x1b["));
    }

    #[test]
    fn json_disables_color_on_resolve() {
        // --json always wins regardless of other flags.
        let o = Output::resolve(true, false, false);
        assert!(o.json);
        assert!(!o.color);
    }

    #[test]
    fn display_width_counts_wide_glyphs_as_two() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("中"), 2); // CJK ideograph is double-width
        assert_eq!(display_width("🚀"), 2); // emoji in the wide range
    }
}
