use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use walkdir::WalkDir;

pub struct FileSearchPlugin;

/// In-memory file index for `f `/`open `/`* ` lookups.
///
/// On first activation, the snapshot path on disk is loaded synchronously
/// (cheap — just a read of newline-separated paths) so even cold starts
/// serve from a pre-warmed index. A background `spawn_blocking` rebuild
/// kicks off after `INDEX_TTL`. During a refresh, queries continue to be
/// served from the stale index so the user never blocks.
struct FileIndex {
    paths: Vec<PathBuf>,
    built_at: Instant,
}

const INDEX_TTL: Duration = Duration::from_secs(60);
const MAX_INDEX_DEPTH: usize = 5;
const MAX_RESULTS: usize = 10;
/// Disk snapshot is considered usable for up to this long. Keeps stale
/// entries from haunting users for weeks if they don't rebuild — but
/// long enough that subsequent cold starts feel instant.
const DISK_INDEX_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn index_cell() -> &'static OnceLock<Mutex<Option<FileIndex>>> {
    static CELL: OnceLock<Mutex<Option<FileIndex>>> = OnceLock::new();
    &CELL
}

/// True iff some other task already kicked off an index build.
fn refresh_inflight() -> &'static std::sync::atomic::AtomicBool {
    static FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &FLAG
}

fn disk_snapshot_path() -> PathBuf {
    crate::path_config::data_dir()
        .join("cache")
        .join("file_index.txt")
}

fn load_disk_snapshot() -> Option<Vec<PathBuf>> {
    let path = disk_snapshot_path();
    let meta = std::fs::metadata(&path).ok()?;
    let modified = meta.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;
    if age > DISK_INDEX_MAX_AGE {
        return None;
    }
    let body = std::fs::read_to_string(&path).ok()?;
    Some(
        body.lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

fn save_disk_snapshot(paths: &[PathBuf]) {
    let path = disk_snapshot_path();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    // Best-effort write — never block / panic the indexing path.
    let mut buf = String::with_capacity(paths.len() * 64);
    for p in paths {
        if let Some(s) = p.to_str() {
            buf.push_str(s);
            buf.push('\n');
        }
    }
    let _ = std::fs::write(&path, buf);
}

fn build_index_sync() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    WalkDir::new(&home)
        .max_depth(MAX_INDEX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .collect()
}

/// Spawn a background indexing job if one isn't already running. The job runs
/// on `spawn_blocking` so the heavy `WalkDir` does not stall the async runtime.
fn kick_off_refresh() {
    use std::sync::atomic::Ordering;
    if refresh_inflight()
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return; // already refreshing
    }
    tokio::task::spawn_blocking(|| {
        let paths = build_index_sync();
        save_disk_snapshot(&paths);
        let cell = index_cell().get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = cell.lock() {
            *guard = Some(FileIndex {
                paths,
                built_at: Instant::now(),
            });
        }
        refresh_inflight().store(false, Ordering::SeqCst);
    });
}

/// Snapshot result returned to query callers.
enum IndexSnapshot {
    /// No index yet — caller should show a placeholder.
    Cold,
    /// Have an index (possibly stale; refresh kicked off if expired).
    Ready(Vec<PathBuf>),
}

fn snapshot_index() -> IndexSnapshot {
    let cell = index_cell().get_or_init(|| Mutex::new(None));
    let snap = {
        let guard = match cell.lock() {
            Ok(g) => g,
            Err(_) => return IndexSnapshot::Cold,
        };
        match &*guard {
            Some(idx) => Some((idx.paths.clone(), idx.built_at.elapsed())),
            None => None,
        }
    };
    match snap {
        None => {
            // First call this process: try to seed from disk so the
            // very first `f ` query returns hits instead of "indexing…".
            // We mark the seeded entry as already past its TTL, so the
            // standard Ready-but-stale path kicks off a background
            // refresh on the next call.
            if let Some(paths) = load_disk_snapshot() {
                if let Ok(mut guard) = cell.lock() {
                    if guard.is_none() {
                        *guard = Some(FileIndex {
                            paths: paths.clone(),
                            // Pretend the snapshot is already past its TTL so a
                            // refresh is scheduled on next query — but this call
                            // returns Ready immediately.
                            built_at: Instant::now()
                                .checked_sub(INDEX_TTL + Duration::from_secs(1))
                                .unwrap_or_else(Instant::now),
                        });
                    }
                }
                kick_off_refresh();
                return IndexSnapshot::Ready(paths);
            }
            kick_off_refresh();
            IndexSnapshot::Cold
        }
        Some((paths, age)) => {
            if age >= INDEX_TTL {
                kick_off_refresh(); // serve stale, refresh in bg
            }
            IndexSnapshot::Ready(paths)
        }
    }
}

#[async_trait]
impl Plugin for FileSearchPlugin {
    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Search files in your home directory"
    }

    fn keyword(&self) -> Option<&str> {
        None // We handle "f " and "open " prefixes manually
    }

    fn cheap_prefix_match(&self, raw: &str) -> bool {
        raw.starts_with("f ") || raw.starts_with("open ") || raw.starts_with("* ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = &q.raw;
        let term = if let Some(t) = raw.strip_prefix("* ") {
            t.trim()
        } else if let Some(t) = raw.strip_prefix("f ") {
            t.trim()
        } else if let Some(t) = raw.strip_prefix("open ") {
            t.trim()
        } else {
            return vec![];
        };

        if term.is_empty() {
            return vec![];
        }

        let term_lower = term.to_lowercase();

        match snapshot_index() {
            IndexSnapshot::Cold => {
                // First touch — index is being built in the background. Return a
                // friendly placeholder instead of blocking the runtime.
                vec![QueryResult {
                    id: "file_search:indexing".to_string(),
                    title: "Indexing files…".to_string(),
                    subtitle: Some(
                        "First-time scan of your home folder. Try again in a moment.".to_string(),
                    ),
                    icon: Some("⏳".to_string()),
                    score: 10,
                    action_type: "none".to_string(),
                    action_data: String::new(),
                    source: None,
                }]
            }
            IndexSnapshot::Ready(paths) => {
                let mut results = Vec::with_capacity(MAX_RESULTS);
                for path in paths {
                    let Some(file_name_os) = path.file_name() else {
                        continue;
                    };
                    let file_name = file_name_os.to_string_lossy().to_lowercase();
                    if !file_name.contains(&term_lower) {
                        continue;
                    }
                    let path_str = path.to_string_lossy().to_string();
                    let score = if file_name == term_lower { 95 } else { 70 };
                    let is_dir = path.is_dir();
                    let icon = if is_dir { "📁" } else { "📄" };
                    results.push(QueryResult {
                        id: format!("file:{}", path_str),
                        title: file_name_os.to_string_lossy().to_string(),
                        subtitle: Some(path_str.clone()),
                        icon: Some(icon.to_string()),
                        score,
                        action_type: "open".to_string(),
                        action_data: path_str,
                        source: None,
                    });
                    if results.len() >= MAX_RESULTS {
                        break;
                    }
                }
                results
            }
        }
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "file_search",
                "description": "Search for files on the filesystem",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Filename or partial name to search for" }
                    },
                    "required": ["query"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        // AI tool path — does NOT go through the cached index. We do a fresh
        // synchronous scan inside spawn_blocking so the AI gets up-to-date
        // results without being limited to the home-folder index.
        let query = args["query"].as_str().unwrap_or("").to_string();
        if query.is_empty() {
            return "Error: 'query' parameter is required".to_string();
        }
        let term_lower = query.to_lowercase();
        let join = tokio::task::spawn_blocking(move || {
            let Some(home) = dirs::home_dir() else {
                return Vec::<String>::new();
            };
            let mut matches: Vec<String> = vec![];
            for entry in WalkDir::new(&home)
                .max_depth(MAX_INDEX_DEPTH)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let file_name = entry.file_name().to_string_lossy().to_lowercase();
                if file_name.contains(&term_lower) {
                    matches.push(entry.path().to_string_lossy().to_string());
                    if matches.len() >= 20 {
                        break;
                    }
                }
            }
            matches
        })
        .await;
        let matches = join.unwrap_or_default();
        if matches.is_empty() {
            format!("No files found matching '{}'", query)
        } else {
            matches.join("\n")
        }
    }
}
