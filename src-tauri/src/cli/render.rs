//! Output system for the `ol` CLI.
//!
//! Every command funnels through this module so human output reads as one
//! consistent visual system and machine output (`--json`) is uniform. Colors
//! use an internal ANSI helper ported from the `scripts/ops.sh` palette (no new
//! color crate). Color/decoration is auto-suppressed when stdout is not a TTY
//! or `NO_COLOR` is set (the NO_COLOR standard), and forced off with
//! `--no-color`.

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
}
