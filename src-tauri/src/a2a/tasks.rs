use std::collections::HashMap;

use super::types::{A2aArtifact, A2aMessage, A2aPart, A2aTask, A2aTaskState, A2aTaskStatus};

/// In-memory record for a single A2A task.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub state: A2aTaskState,
    /// Original A2A request ID, if the client provided one.
    pub request_id: Option<String>,
    /// The `contextId` associated with this task, if the client provided one.
    /// Echoed to callers via the wire-format task.
    pub context_id: Option<String>,
    /// Short human-readable summary of the original request.
    pub request_summary: String,
    /// Output messages from the agent.
    pub output_messages: Vec<A2aMessage>,
    /// Structured artifacts produced by the task.
    pub artifacts: Vec<A2aArtifact>,
    /// Error detail string (should already be masked before storing).
    pub error: Option<String>,
    /// If true, a cancel request was received.
    pub cancel_requested: bool,
}

impl TaskRecord {
    /// Convert this registry record into the wire-format A2A task.
    pub fn to_a2a_task(&self) -> A2aTask {
        let status_message = if let Some(ref err) = self.error {
            Some(A2aMessage {
                role: "agent".to_string(),
                parts: vec![A2aPart::Text { text: err.clone() }],
            })
        } else {
            self.output_messages.last().cloned()
        };

        A2aTask {
            id: self.id.clone(),
            context_id: self.context_id.clone(),
            status: A2aTaskStatus {
                state: self.state,
                message: status_message,
                timestamp: Some(self.updated_at.to_rfc3339()),
            },
            artifacts: self.artifacts.clone(),
            history: self.output_messages.clone(),
        }
    }
}

// ── Task ID generation ──────────────────────────────────────────────────────

/// Generate a random 16-byte hex ID (32 chars). Used for task and artifact IDs.
pub(crate) fn generate_task_id() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().fold(String::with_capacity(32), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
        s
    })
}

// ── Registry ────────────────────────────────────────────────────────────────

/// Bounded in-memory task registry.
///
/// Retains up to `max_terminal` completed/failed/canceled/rejected tasks plus
/// any currently-active tasks. Oldest terminal tasks are evicted first when the
/// cap is exceeded.
pub struct TaskRegistry {
    tasks: HashMap<String, TaskRecord>,
    max_terminal: usize,
}

impl TaskRegistry {
    pub fn new(max_terminal: usize) -> Self {
        Self {
            tasks: HashMap::new(),
            max_terminal,
        }
    }

    /// Create a new task in the `submitted` state.
    pub fn create_submitted(
        &mut self,
        request_summary: String,
        request_id: Option<String>,
        context_id: Option<String>,
    ) -> String {
        let now = chrono::Utc::now();
        let id = generate_task_id();
        let record = TaskRecord {
            id: id.clone(),
            created_at: now,
            updated_at: now,
            state: A2aTaskState::Submitted,
            request_id,
            context_id,
            request_summary,
            output_messages: Vec::new(),
            artifacts: Vec::new(),
            error: None,
            cancel_requested: false,
        };
        self.tasks.insert(id.clone(), record);
        id
    }

    /// Transition a task to `working`.
    pub fn mark_working(&mut self, task_id: &str) {
        if let Some(record) = self.tasks.get_mut(task_id) {
            if !record.state.is_terminal() {
                record.state = A2aTaskState::Working;
                record.updated_at = chrono::Utc::now();
            }
        }
    }

    /// Transition a task to `completed` with output messages and optional
    /// artifacts.
    pub fn mark_completed(
        &mut self,
        task_id: &str,
        messages: Vec<A2aMessage>,
        artifacts: Vec<A2aArtifact>,
    ) {
        if let Some(record) = self.tasks.get_mut(task_id) {
            if !record.state.is_terminal() {
                record.state = A2aTaskState::Completed;
                record.output_messages = messages;
                record.artifacts = artifacts;
                record.updated_at = chrono::Utc::now();
            }
        }
        self.evict_if_needed();
    }

    /// Transition a task to `failed` with an error message.
    pub fn mark_failed(&mut self, task_id: &str, error: String) {
        if let Some(record) = self.tasks.get_mut(task_id) {
            if !record.state.is_terminal() {
                record.state = A2aTaskState::Failed;
                record.error = Some(error);
                record.updated_at = chrono::Utc::now();
            }
        }
        self.evict_if_needed();
    }

    /// Request cancellation for a task. Returns `true` if the task was found
    /// and was not already terminal.
    pub fn cancel(&mut self, task_id: &str) -> bool {
        if let Some(record) = self.tasks.get_mut(task_id) {
            if record.state.is_terminal() {
                return false;
            }
            record.cancel_requested = true;
            record.state = A2aTaskState::Canceled;
            record.updated_at = chrono::Utc::now();
            self.evict_if_needed();
            true
        } else {
            false
        }
    }

    /// Mark a task as `rejected` (invalid or unauthorized after routing).
    pub fn mark_rejected(&mut self, task_id: &str, reason: String) {
        if let Some(record) = self.tasks.get_mut(task_id) {
            if !record.state.is_terminal() {
                record.state = A2aTaskState::Rejected;
                record.error = Some(reason);
                record.updated_at = chrono::Utc::now();
            }
        }
        self.evict_if_needed();
    }

    /// Retrieve a task record by ID.
    pub fn get(&self, task_id: &str) -> Option<&TaskRecord> {
        self.tasks.get(task_id)
    }

    /// List all tasks, ordered by creation time (oldest first).
    pub fn list(&self) -> Vec<&TaskRecord> {
        let mut records: Vec<_> = self.tasks.values().collect();
        records.sort_by_key(|r| r.created_at);
        records
    }

    /// Check whether a cancel has been requested for a task.
    pub fn is_cancel_requested(&self, task_id: &str) -> bool {
        self.tasks.get(task_id).is_some_and(|r| r.cancel_requested)
    }

    /// Evict the oldest terminal tasks when the count exceeds the cap.
    fn evict_if_needed(&mut self) {
        let terminal_ids: Vec<(String, chrono::DateTime<chrono::Utc>)> = self
            .tasks
            .iter()
            .filter(|(_, r)| r.state.is_terminal())
            .map(|(id, r)| (id.clone(), r.updated_at))
            .collect();

        if terminal_ids.len() <= self.max_terminal {
            return;
        }

        let mut to_evict = terminal_ids;
        to_evict.sort_by_key(|(_, ts)| *ts);
        let excess = to_evict.len() - self.max_terminal;
        for (id, _) in to_evict.into_iter().take(excess) {
            self.tasks.remove(&id);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get_task() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("test query".to_string(), None, None);
        let task = reg.get(&id).expect("task should exist");
        assert_eq!(task.state, A2aTaskState::Submitted);
        assert_eq!(task.request_summary, "test query");
    }

    #[test]
    fn lifecycle_submitted_working_completed() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);

        reg.mark_working(&id);
        assert_eq!(reg.get(&id).unwrap().state, A2aTaskState::Working);

        let msg = A2aMessage {
            role: "agent".to_string(),
            parts: vec![A2aPart::Text {
                text: "done".to_string(),
            }],
        };
        reg.mark_completed(&id, vec![msg], vec![]);
        assert_eq!(reg.get(&id).unwrap().state, A2aTaskState::Completed);
    }

    #[test]
    fn lifecycle_submitted_working_failed() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);
        reg.mark_working(&id);
        reg.mark_failed(&id, "timeout".to_string());
        let task = reg.get(&id).unwrap();
        assert_eq!(task.state, A2aTaskState::Failed);
        assert_eq!(task.error.as_deref(), Some("timeout"));
    }

    #[test]
    fn cancel_before_completion() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);
        reg.mark_working(&id);
        assert!(reg.cancel(&id));
        assert_eq!(reg.get(&id).unwrap().state, A2aTaskState::Canceled);
    }

    #[test]
    fn cancel_completed_task_is_noop() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);
        reg.mark_completed(&id, vec![], vec![]);
        assert!(!reg.cancel(&id));
        assert_eq!(reg.get(&id).unwrap().state, A2aTaskState::Completed);
    }

    #[test]
    fn terminal_state_does_not_revert() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);
        reg.mark_failed(&id, "err".to_string());

        // Attempts to move to working or completed should be ignored.
        reg.mark_working(&id);
        assert_eq!(reg.get(&id).unwrap().state, A2aTaskState::Failed);

        reg.mark_completed(&id, vec![], vec![]);
        assert_eq!(reg.get(&id).unwrap().state, A2aTaskState::Failed);
    }

    #[test]
    fn list_returns_sorted_by_creation() {
        let mut reg = TaskRegistry::new(100);
        let id1 = reg.create_submitted("first".to_string(), None, None);
        let id2 = reg.create_submitted("second".to_string(), None, None);
        let id3 = reg.create_submitted("third".to_string(), None, None);

        let list = reg.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].id, id1);
        assert_eq!(list[1].id, id2);
        assert_eq!(list[2].id, id3);
    }

    #[test]
    fn eviction_removes_oldest_terminal() {
        let mut reg = TaskRegistry::new(2);

        let id1 = reg.create_submitted("a".to_string(), None, None);
        reg.mark_completed(&id1, vec![], vec![]);

        let id2 = reg.create_submitted("b".to_string(), None, None);
        reg.mark_completed(&id2, vec![], vec![]);

        // Both terminal tasks should still be retained at cap.
        assert!(reg.get(&id1).is_some());
        assert!(reg.get(&id2).is_some());

        // Adding a third terminal task triggers eviction of id1.
        let id3 = reg.create_submitted("c".to_string(), None, None);
        reg.mark_completed(&id3, vec![], vec![]);

        assert!(reg.get(&id1).is_none(), "oldest terminal should be evicted");
        assert!(reg.get(&id2).is_some());
        assert!(reg.get(&id3).is_some());
    }

    #[test]
    fn eviction_preserves_active_tasks() {
        let mut reg = TaskRegistry::new(1);

        // One working (active) task.
        let active = reg.create_submitted("active".to_string(), None, None);
        reg.mark_working(&active);

        // Two terminal tasks — only 1 allowed.
        let t1 = reg.create_submitted("done1".to_string(), None, None);
        reg.mark_completed(&t1, vec![], vec![]);
        let t2 = reg.create_submitted("done2".to_string(), None, None);
        reg.mark_completed(&t2, vec![], vec![]);

        // Active task MUST be preserved, oldest terminal evicted.
        assert!(reg.get(&active).is_some(), "active task must survive");
        assert!(reg.get(&t1).is_none(), "oldest terminal evicted");
        assert!(reg.get(&t2).is_some(), "newest terminal retained");
    }

    #[test]
    fn to_a2a_task_conversion() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("hello".to_string(), None, None);
        reg.mark_working(&id);
        let msg = A2aMessage {
            role: "agent".to_string(),
            parts: vec![A2aPart::Text {
                text: "world".to_string(),
            }],
        };
        reg.mark_completed(&id, vec![msg], vec![]);

        let a2a = reg.get(&id).unwrap().to_a2a_task();
        assert_eq!(a2a.id, id);
        assert_eq!(a2a.status.state, A2aTaskState::Completed);
        assert!(a2a.status.timestamp.is_some());
    }

    #[test]
    fn create_submitted_with_context_id_stores_and_echoes_it() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, Some("ctx-abc".to_string()));
        let record = reg.get(&id).expect("task should exist");
        assert_eq!(record.context_id.as_deref(), Some("ctx-abc"));

        let a2a = record.to_a2a_task();
        assert_eq!(a2a.context_id.as_deref(), Some("ctx-abc"));
    }

    #[test]
    fn create_submitted_with_no_context_id_omits_it_from_a2a_task() {
        let mut reg = TaskRegistry::new(100);
        let id = reg.create_submitted("q".to_string(), None, None);
        let a2a = reg.get(&id).unwrap().to_a2a_task();
        assert!(a2a.context_id.is_none());
    }
}
