//! A minimal interactive multi-select list for the `ol` CLI.
//!
//! Used by `ol mcp login` when no server name is given: the user arrows through
//! the OAuth-capable servers, toggles the ones to authorize with Space, and
//! confirms with Enter.
//!
//! The module is deliberately split in two:
//!   - [`SelectionState`] is a pure state machine (no I/O), so navigation and
//!     toggling are unit-testable without a PTY.
//!   - [`select_many`] wires that state machine to crossterm's raw-mode key
//!     events and restores the terminal via [`TerminalGuard`] on every exit
//!     path, including panics.

use std::io::{IsTerminal, Write};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{cursor, terminal};

/// One selectable row: a value plus an optional dim right-hand hint.
pub(crate) struct SelectionItem {
    pub label: String,
    pub hint: String,
}

/// What the user did with the list.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SelectionOutcome {
    /// Confirmed with Enter. Indices into the original `items` slice, ascending.
    /// May be empty when the user confirmed without checking anything.
    Selected(Vec<usize>),
    /// Dismissed with Esc, `q`, or Ctrl-C.
    Cancelled,
}

/// A key press translated into an intent, so the state machine never sees
/// crossterm types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionAction {
    Up,
    Down,
    Toggle,
    Confirm,
    Cancel,
}

/// Cursor position + checked set for a list of `len` rows.
#[derive(Debug)]
pub(crate) struct SelectionState {
    len: usize,
    cursor: usize,
    checked: Vec<bool>,
}

impl SelectionState {
    pub fn new(len: usize) -> Self {
        SelectionState {
            len,
            cursor: 0,
            checked: vec![false; len],
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_checked(&self, index: usize) -> bool {
        self.checked.get(index).copied().unwrap_or(false)
    }

    /// Indices of every checked row, ascending.
    pub fn selected(&self) -> Vec<usize> {
        self.checked
            .iter()
            .enumerate()
            .filter_map(|(i, on)| on.then_some(i))
            .collect()
    }

    /// Apply one action. Returns `Some(outcome)` once the user is done.
    ///
    /// Navigation wraps around, which matters most for short lists where
    /// reaching the last row from the top should take one keystroke.
    pub fn apply(&mut self, action: SelectionAction) -> Option<SelectionOutcome> {
        if self.len == 0 {
            // Nothing to drive; only leaving makes sense.
            return match action {
                SelectionAction::Confirm => Some(SelectionOutcome::Selected(Vec::new())),
                SelectionAction::Cancel => Some(SelectionOutcome::Cancelled),
                _ => None,
            };
        }
        match action {
            SelectionAction::Up => {
                self.cursor = if self.cursor == 0 {
                    self.len - 1
                } else {
                    self.cursor - 1
                };
                None
            }
            SelectionAction::Down => {
                self.cursor = (self.cursor + 1) % self.len;
                None
            }
            SelectionAction::Toggle => {
                let slot = &mut self.checked[self.cursor];
                *slot = !*slot;
                None
            }
            SelectionAction::Confirm => Some(SelectionOutcome::Selected(self.selected())),
            SelectionAction::Cancel => Some(SelectionOutcome::Cancelled),
        }
    }
}

/// Translate a crossterm key event into an action. `None` for keys we ignore.
fn action_for(code: KeyCode, modifiers: KeyModifiers) -> Option<SelectionAction> {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return match code {
            KeyCode::Char('c') => Some(SelectionAction::Cancel),
            _ => None,
        };
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(SelectionAction::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(SelectionAction::Down),
        KeyCode::Char(' ') => Some(SelectionAction::Toggle),
        KeyCode::Enter => Some(SelectionAction::Confirm),
        KeyCode::Esc | KeyCode::Char('q') => Some(SelectionAction::Cancel),
        _ => None,
    }
}

/// Restores the terminal on every exit path, including unwinding panics.
///
/// Order matters: show the cursor *before* leaving raw mode, so a failure
/// midway never leaves an invisible cursor on a cooked terminal.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut err = std::io::stderr();
        let _ = crossterm::execute!(err, cursor::Hide);
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut err = std::io::stderr();
        let _ = crossterm::execute!(err, cursor::Show);
        let _ = terminal::disable_raw_mode();
        let _ = err.flush();
    }
}

/// True when an interactive selector can be shown: both stdin (key events) and
/// stderr (the rendering surface) must be attached to a terminal.
pub(crate) fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Draw the list. Rendering goes to stderr so stdout stays clean for anything
/// the caller prints afterwards.
///
/// `first` is the index of the topmost visible row; the list scrolls when it
/// does not fit the terminal height.
fn render(
    w: &mut dyn Write,
    prompt: &str,
    items: &[SelectionItem],
    state: &SelectionState,
    first: usize,
    visible: usize,
) -> std::io::Result<()> {
    writeln!(w, "{prompt}\r")?;
    let last = (first + visible).min(items.len());
    for (index, item) in items.iter().enumerate().take(last).skip(first) {
        let pointer = if index == state.cursor() { ">" } else { " " };
        let box_ = if state.is_checked(index) {
            "[x]"
        } else {
            "[ ]"
        };
        if item.hint.is_empty() {
            writeln!(w, "{pointer} {box_} {}\r", item.label)?;
        } else {
            writeln!(w, "{pointer} {box_} {}  {}\r", item.label, item.hint)?;
        }
    }
    write!(
        w,
        "\r\n  ↑/↓ move · space toggle · enter confirm · esc cancel\r"
    )?;
    w.flush()
}

/// Number of rows the list body may occupy, leaving room for the prompt and
/// the key-hint footer.
fn viewport_rows(items: usize) -> usize {
    let height = terminal::size().map(|(_, h)| h as usize).unwrap_or(24);
    items.min(height.saturating_sub(4).max(1))
}

/// Scroll `first` so that `cursor` stays visible in a `visible`-row window.
fn scroll_to(first: usize, cursor: usize, visible: usize) -> usize {
    if cursor < first {
        cursor
    } else if cursor >= first + visible {
        cursor + 1 - visible
    } else {
        first
    }
}

/// Show an interactive checklist and return what the user chose.
///
/// The caller must have already confirmed [`is_interactive`]; this function
/// blocks on key events and never falls back to reading lines from stdin.
pub(crate) fn select_many(
    prompt: &str,
    items: &[SelectionItem],
) -> std::io::Result<SelectionOutcome> {
    let mut state = SelectionState::new(items.len());
    let visible = viewport_rows(items.len());
    let mut first = 0usize;
    let mut err = std::io::stderr();

    let _guard = TerminalGuard::enter()?;
    // Draw into the alternate screen so the user's scrollback survives.
    crossterm::execute!(err, terminal::EnterAlternateScreen)?;
    let outcome = loop {
        crossterm::execute!(
            err,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        render(&mut err, prompt, items, &state, first, visible)?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Windows reports press *and* release; acting on both double-toggles.
        if key.kind == KeyEventKind::Release {
            continue;
        }
        let Some(action) = action_for(key.code, key.modifiers) else {
            continue;
        };
        if let Some(outcome) = state.apply(action) {
            break outcome;
        }
        first = scroll_to(first, state.cursor(), visible);
    };
    crossterm::execute!(err, terminal::LeaveAlternateScreen)?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(len: usize) -> SelectionState {
        SelectionState::new(len)
    }

    #[test]
    fn down_and_up_wrap_around() {
        let mut s = state(3);
        assert_eq!(s.cursor(), 0);
        s.apply(SelectionAction::Up);
        assert_eq!(s.cursor(), 2, "up from the top wraps to the last row");
        s.apply(SelectionAction::Down);
        assert_eq!(s.cursor(), 0, "down from the bottom wraps to the first row");
        s.apply(SelectionAction::Down);
        assert_eq!(s.cursor(), 1);
    }

    #[test]
    fn toggle_marks_only_the_cursor_row() {
        let mut s = state(3);
        s.apply(SelectionAction::Down);
        s.apply(SelectionAction::Toggle);
        assert!(!s.is_checked(0));
        assert!(s.is_checked(1));
        assert!(!s.is_checked(2));
        s.apply(SelectionAction::Toggle);
        assert!(!s.is_checked(1), "toggling twice clears the row");
    }

    #[test]
    fn confirm_returns_checked_indices_in_order() {
        let mut s = state(4);
        s.apply(SelectionAction::Toggle); // row 0
        s.apply(SelectionAction::Down);
        s.apply(SelectionAction::Down);
        s.apply(SelectionAction::Toggle); // row 2
        let outcome = s.apply(SelectionAction::Confirm);
        assert_eq!(outcome, Some(SelectionOutcome::Selected(vec![0, 2])));
    }

    #[test]
    fn confirm_with_nothing_checked_selects_nothing() {
        let mut s = state(2);
        assert_eq!(
            s.apply(SelectionAction::Confirm),
            Some(SelectionOutcome::Selected(Vec::new()))
        );
    }

    #[test]
    fn cancel_discards_checked_rows() {
        let mut s = state(2);
        s.apply(SelectionAction::Toggle);
        assert_eq!(
            s.apply(SelectionAction::Cancel),
            Some(SelectionOutcome::Cancelled)
        );
    }

    #[test]
    fn empty_list_only_responds_to_confirm_and_cancel() {
        let mut s = state(0);
        assert_eq!(s.apply(SelectionAction::Up), None);
        assert_eq!(s.apply(SelectionAction::Toggle), None);
        assert_eq!(
            s.apply(SelectionAction::Confirm),
            Some(SelectionOutcome::Selected(Vec::new()))
        );
    }

    #[test]
    fn ctrl_c_cancels_but_bare_c_is_ignored() {
        assert_eq!(
            action_for(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(SelectionAction::Cancel)
        );
        assert_eq!(action_for(KeyCode::Char('c'), KeyModifiers::NONE), None);
    }

    #[test]
    fn vim_keys_navigate() {
        assert_eq!(
            action_for(KeyCode::Char('j'), KeyModifiers::NONE),
            Some(SelectionAction::Down)
        );
        assert_eq!(
            action_for(KeyCode::Char('k'), KeyModifiers::NONE),
            Some(SelectionAction::Up)
        );
        assert_eq!(
            action_for(KeyCode::Char('q'), KeyModifiers::NONE),
            Some(SelectionAction::Cancel)
        );
    }

    #[test]
    fn scrolling_keeps_the_cursor_inside_the_window() {
        assert_eq!(scroll_to(0, 2, 3), 0, "cursor already visible: no scroll");
        assert_eq!(scroll_to(0, 3, 3), 1, "cursor past the bottom scrolls down");
        assert_eq!(scroll_to(4, 2, 3), 2, "cursor above the top scrolls up");
    }

    #[test]
    fn render_marks_cursor_and_checked_rows() {
        let items = vec![
            SelectionItem {
                label: "alpha".to_string(),
                hint: "https://a".to_string(),
            },
            SelectionItem {
                label: "beta".to_string(),
                hint: String::new(),
            },
        ];
        let mut s = SelectionState::new(2);
        s.apply(SelectionAction::Toggle);
        let mut buf: Vec<u8> = Vec::new();
        render(&mut buf, "pick:", &items, &s, 0, 2).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("> [x] alpha  https://a"));
        assert!(text.contains("  [ ] beta"));
    }
}
