//! Browser dashboards served by the embedded live server.
//!
//! Each feature has its own dedicated page + JSON data endpoint:
//!
//! | Page                       | Data endpoint                   |
//! |----------------------------|---------------------------------|
//! | `/dashboard`               | (index — links to all pages)    |
//! | `/dashboard/todos`         | `/dashboard/todos/data`         |
//! | `/dashboard/conversation`  | `/dashboard/conversation/data`  |
//! | `/dashboard/jobs`          | `/dashboard/jobs/data`          |
//! | `/dashboard/tables`        | `/dashboard/tables/data`        |

mod common;
mod conversation;
mod index;
mod jobs;
mod tables;
mod todos;

pub use conversation::{conversation_data_json, conversation_html};
pub use index::{index_data_json, index_html};
pub use jobs::{jobs_data_json, jobs_html};
pub use tables::{tables_data_json, tables_html};
pub use todos::{todos_data_json, todos_html};
