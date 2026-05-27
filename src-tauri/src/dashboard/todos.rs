//! Todos dashboard.
//!
//! Serves the rich interactive todo browser (formerly hosted at `/todo`)
//! merged in under `/dashboard/todos`. The legacy `/todo` route has been
//! decommissioned in favor of this single entry point.

use crate::plugins::todo;

pub fn todos_html() -> String {
    todo::todo_live_html()
}

pub fn todos_data_json() -> String {
    todo::todo_live_data_json()
}
