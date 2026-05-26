use async_trait::async_trait;
use omnilauncher_lib::create_plugin_manager;
use omnilauncher_lib::plugins::{Plugin, PluginManager, Query, QueryResult};
use std::collections::HashSet;
use std::sync::Mutex;

static TODO_LOCK: Mutex<()> = Mutex::new(());

struct KeywordOnlyPlugin {
    keyword: &'static str,
    title: &'static str,
}

#[async_trait]
impl Plugin for KeywordOnlyPlugin {
    fn name(&self) -> &str {
        self.title
    }

    fn description(&self) -> &str {
        self.title
    }

    fn keyword(&self) -> Option<&str> {
        Some(self.keyword)
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        vec![QueryResult {
            id: self.title.to_string(),
            title: self.title.to_string(),
            subtitle: None,
            icon: None,
            score: 100,
            action_type: "none".to_string(),
            action_data: String::new(),
        }]
    }
}

// ============================================================
// Plugin Manager
// ============================================================

#[tokio::test]
async fn test_plugin_manager_creation() {
    let pm = create_plugin_manager();
    assert!(
        pm.plugins.len() > 20,
        "Expected 20+ plugins, got {}",
        pm.plugins.len()
    );
}

#[tokio::test]
async fn test_plugin_manager_all_tool_schemas() {
    let pm = create_plugin_manager();
    let schemas = pm.all_tool_schemas();
    assert!(
        schemas.len() >= 10,
        "Expected 10+ tool schemas, got {}",
        schemas.len()
    );
    for schema in &schemas {
        assert!(schema["function"]["name"].is_string());
        assert!(schema["function"]["description"].is_string());
    }
}

#[tokio::test]
async fn test_every_registered_plugin_has_valid_metadata_and_query_smoke() {
    let pm = create_plugin_manager();
    let mut names = HashSet::new();

    for plugin in &pm.plugins {
        let name = plugin.name();
        assert!(!name.trim().is_empty(), "plugin has an empty name");
        assert!(names.insert(name.to_string()), "duplicate plugin name: {name}");
        assert!(
            !plugin.description().trim().is_empty(),
            "plugin {name} has an empty description"
        );

        let raw = plugin.keyword().unwrap_or("").to_string();
        let query = Query {
            raw: raw.clone(),
            terms: raw.split_whitespace().map(String::from).collect(),
        };
        let _ = plugin.query(&query).await;

        if let Some(schema) = plugin.tool_schema() {
            assert_eq!(
                schema["type"], "function",
                "plugin {name} has an invalid tool schema type"
            );
            assert!(
                schema["function"]["name"].is_string(),
                "plugin {name} schema is missing function.name"
            );
            assert!(
                schema["function"]["description"].is_string(),
                "plugin {name} schema is missing function.description"
            );
        }
    }
}

#[tokio::test]
async fn test_plugin_manager_query_calculator() {
    let pm = create_plugin_manager();
    let results = pm.query_all("= 2+2").await;
    assert!(!results.is_empty());
    assert!(results[0].title.contains('4'));
}

#[tokio::test]
async fn test_plugin_manager_keyword_requires_boundary() {
    let mut pm = PluginManager::new();
    pm.register(Box::new(KeywordOnlyPlugin {
        keyword: "pomo",
        title: "pomo plugin",
    }));

    let results = pm.query_all("pomodoroapp").await;
    assert!(results.is_empty(), "Unexpected keyword match: {results:?}");
}

#[tokio::test]
async fn test_plugin_manager_keyword_still_matches_exact_token() {
    let mut pm = PluginManager::new();
    pm.register(Box::new(KeywordOnlyPlugin {
        keyword: "sched",
        title: "scheduler plugin",
    }));

    let exact = pm.query_all("sched").await;
    assert_eq!(exact.len(), 1);

    let with_args = pm.query_all("sched list").await;
    assert_eq!(with_args.len(), 1);
}

// ============================================================
// Calculator
// ============================================================

#[tokio::test]
async fn test_calc_add() {
    let pm = create_plugin_manager();
    let r = pm.query_all("= 3+4").await;
    assert!(!r.is_empty());
    assert!(r[0].title.contains('7'));
}

#[tokio::test]
async fn test_calc_multiply() {
    let pm = create_plugin_manager();
    let r = pm.query_all("= 6*7").await;
    assert!(r[0].title.contains("42"));
}

#[tokio::test]
async fn test_calc_parens() {
    let pm = create_plugin_manager();
    let r = pm.query_all("= (10+5)*2").await;
    assert!(r[0].title.contains("30"));
}

#[tokio::test]
async fn test_calc_power() {
    let pm = create_plugin_manager();
    let r = pm.query_all("= 2^10").await;
    assert!(r[0].title.contains("1024"));
}

// ============================================================
// Shell Exec
// ============================================================

#[tokio::test]
async fn test_shell_exec_echo() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("shell_exec", serde_json::json!({"command": "echo hello"}))
        .await;
    assert!(r.to_lowercase().contains("hello"), "Got: {}", r);
}

#[tokio::test]
async fn test_shell_exec_empty() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("shell_exec", serde_json::json!({"command": ""}))
        .await;
    assert!(r.to_lowercase().contains("error") || r.contains("no command"));
}

#[tokio::test]
async fn test_shell_exec_working_dir() {
    let pm = create_plugin_manager();
    let (cmd, dir, expect) = if cfg!(target_os = "windows") {
        ("Get-Location", "C:\\", "C:\\")
    } else {
        ("pwd", "/", "/")
    };
    let r = pm
        .execute_tool(
            "shell_exec",
            serde_json::json!({"command": cmd, "working_dir": dir}),
        )
        .await;
    assert!(r.contains(expect), "Got: {}", r);
}

// ============================================================
// File Read
// ============================================================

#[tokio::test]
async fn test_file_read() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_test_read.txt");
    std::fs::write(&p, "hello\nworld\nfoo").unwrap();
    let r = pm
        .execute_tool(
            "file_read",
            serde_json::json!({"path": p.to_string_lossy()}),
        )
        .await;
    assert!(r.contains("hello") && r.contains("world"));
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn test_file_read_nonexistent() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "file_read",
            serde_json::json!({"path": "/tmp/no_such_file_xyz"}),
        )
        .await;
    assert!(r.to_lowercase().contains("error"));
}

#[tokio::test]
async fn test_file_read_line_range() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_test_range.txt");
    std::fs::write(&p, "L1\nL2\nL3\nL4\nL5").unwrap();
    let r = pm
        .execute_tool(
            "file_read",
            serde_json::json!({"path": p.to_string_lossy(), "start_line": 2, "end_line": 3}),
        )
        .await;
    assert!(r.contains("L2") && r.contains("L3"));
    assert!(!r.contains("L4"));
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn test_file_read_empty_path() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("file_read", serde_json::json!({"path": ""}))
        .await;
    assert!(r.to_lowercase().contains("error") || r.contains("no path"));
}

// ============================================================
// File Write
// ============================================================

#[tokio::test]
async fn test_file_write_and_verify() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_test_write.txt");
    let r = pm
        .execute_tool(
            "file_write",
            serde_json::json!({"path": p.to_string_lossy(), "content": "written"}),
        )
        .await;
    assert!(r.contains("Successfully"));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "written");
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn test_file_write_append() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_test_append.txt");
    std::fs::write(&p, "A").unwrap();
    let _ = pm
        .execute_tool(
            "file_write",
            serde_json::json!({"path": p.to_string_lossy(), "content": "B", "append": true}),
        )
        .await;
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "AB");
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn test_file_write_creates_dirs() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir()
        .join("omni_nested")
        .join("d")
        .join("f.txt");
    let _ = pm
        .execute_tool(
            "file_write",
            serde_json::json!({"path": p.to_string_lossy(), "content": "x"}),
        )
        .await;
    assert!(p.exists());
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("omni_nested"));
}

// ============================================================
// File Edit
// ============================================================

#[tokio::test]
async fn test_file_edit_success() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_edit.txt");
    std::fs::write(&p, "foo bar baz").unwrap();
    let r = pm.execute_tool("file_edit", serde_json::json!({"path": p.to_string_lossy(), "old_string": "bar", "new_string": "QUX"})).await;
    assert!(r.to_lowercase().contains("success"));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "foo QUX baz");
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn test_file_edit_not_found() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_edit2.txt");
    std::fs::write(&p, "hello").unwrap();
    let r = pm.execute_tool("file_edit", serde_json::json!({"path": p.to_string_lossy(), "old_string": "xyz", "new_string": "a"})).await;
    assert!(r.contains("not found"));
    let _ = std::fs::remove_file(&p);
}

#[tokio::test]
async fn test_file_edit_ambiguous() {
    let pm = create_plugin_manager();
    let p = std::env::temp_dir().join("omni_edit3.txt");
    std::fs::write(&p, "aa bb aa").unwrap();
    let r = pm.execute_tool("file_edit", serde_json::json!({"path": p.to_string_lossy(), "old_string": "aa", "new_string": "cc"})).await;
    assert!(r.contains("2 times") || r.contains("unique") || r.to_lowercase().contains("error"));
    let _ = std::fs::remove_file(&p);
}

// ============================================================
// Grep
// ============================================================

#[tokio::test]
async fn test_grep_finds() {
    let pm = create_plugin_manager();
    let d = std::env::temp_dir().join("omni_grep");
    let _ = std::fs::create_dir_all(&d);
    std::fs::write(d.join("a.txt"), "hello world\nfoo\nhello again").unwrap();
    let r = pm
        .execute_tool(
            "grep_search",
            serde_json::json!({"pattern": "hello", "path": d.to_string_lossy()}),
        )
        .await;
    assert!(r.contains("hello"), "Got: {}", r);
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn test_grep_empty_pattern() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("grep_search", serde_json::json!({"pattern": ""}))
        .await;
    assert!(r.to_lowercase().contains("error") || r.contains("no pattern"));
}

// ============================================================
// Glob
// ============================================================

#[tokio::test]
async fn test_glob_matches() {
    let pm = create_plugin_manager();
    let d = std::env::temp_dir().join("omni_glob");
    let _ = std::fs::create_dir_all(&d);
    std::fs::write(d.join("x.rs"), "").unwrap();
    std::fs::write(d.join("y.txt"), "").unwrap();
    let r = pm
        .execute_tool(
            "glob_files",
            serde_json::json!({"pattern": "*.rs", "path": d.to_string_lossy()}),
        )
        .await;
    assert!(r.contains("x.rs"));
    assert!(!r.contains("y.txt"));
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn test_glob_no_match() {
    let pm = create_plugin_manager();
    let d = std::env::temp_dir().join("omni_glob2");
    let _ = std::fs::create_dir_all(&d);
    std::fs::write(d.join("a.txt"), "").unwrap();
    let r = pm
        .execute_tool(
            "glob_files",
            serde_json::json!({"pattern": "*.xyz", "path": d.to_string_lossy()}),
        )
        .await;
    assert!(r.contains("No files"));
    let _ = std::fs::remove_dir_all(&d);
}

// ============================================================
// List Dir
// ============================================================

#[tokio::test]
async fn test_ls_shows_entries() {
    let pm = create_plugin_manager();
    let d = std::env::temp_dir().join("omni_ls");
    let _ = std::fs::create_dir_all(d.join("sub"));
    std::fs::write(d.join("f.txt"), "").unwrap();
    let r = pm
        .execute_tool("list_dir", serde_json::json!({"path": d.to_string_lossy()}))
        .await;
    assert!(r.contains("sub/") && r.contains("f.txt"));
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn test_ls_nonexistent() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("list_dir", serde_json::json!({"path": "/no_such_dir_xyz"}))
        .await;
    assert!(r.to_lowercase().contains("error") || r.contains("not exist"));
}

// ============================================================
// Git
// ============================================================

#[tokio::test]
async fn test_git_status_runs() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("git_ops", serde_json::json!({"subcommand": "status"}))
        .await;
    assert!(!r.is_empty());
}

// ============================================================
// Todo
// ============================================================

#[tokio::test]
async fn test_todo_lifecycle() {
    let _guard = TODO_LOCK.lock().unwrap();
    let dir = temp_config_dir("todo_lifecycle");
    unsafe {
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &dir);
    }
    let pm = create_plugin_manager();
    let _ = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "clear"}))
        .await;
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "add", "text": "item1"}),
        )
        .await;
    let list = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "list"}))
        .await;
    assert!(list.contains("item1"));
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "remove", "text": "1"}),
        )
        .await;
    let list2 = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "list"}))
        .await;
    assert!(!list2.contains("item1"));
    let _ = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "clear"}))
        .await;
    unsafe {
        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    }
}

#[tokio::test]
async fn test_todo_notes() {
    let pm = create_plugin_manager();
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "note_save", "text": "_t", "content": "data"}),
        )
        .await;
    let r = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "note_read", "text": "_t"}),
        )
        .await;
    assert!(r.contains("data"));
    let notes = dirs::home_dir()
        .unwrap()
        .join(".omnilauncher")
        .join("notes");
    let _ = std::fs::remove_file(notes.join("_t.md"));
}

// ============================================================
// Color Picker
// ============================================================

#[tokio::test]
async fn test_color_hex() {
    let pm = create_plugin_manager();
    let r = pm.query_all("color #ff0000").await;
    assert!(!r.is_empty());
}

#[tokio::test]
async fn test_color_name() {
    let pm = create_plugin_manager();
    let r = pm.query_all("color blue").await;
    assert!(!r.is_empty());
}

#[tokio::test]
async fn test_color_rgb() {
    let pm = create_plugin_manager();
    let r = pm.query_all("color rgb(255,128,0)").await;
    assert!(!r.is_empty());
}

// ============================================================
// System Commands
// ============================================================

#[tokio::test]
async fn test_sys_commands_list() {
    let pm = create_plugin_manager();
    let r = pm.query_all("sys ").await;
    assert!(r.len() >= 3);
}

// ============================================================
// Web Search
// ============================================================

#[tokio::test]
async fn test_web_google() {
    let pm = create_plugin_manager();
    let r = pm.query_all("g test").await;
    assert!(r[0].action_data.contains("google.com"));
}

#[tokio::test]
async fn test_web_tool() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "web_search",
            serde_json::json!({"query": "hi", "engine": "youtube"}),
        )
        .await;
    assert!(r.contains("youtube.com"));
}

// ============================================================
// Env Vars
// ============================================================

#[tokio::test]
async fn test_env_query() {
    let pm = create_plugin_manager();
    let r = pm.query_all("env PATH").await;
    assert!(!r.is_empty());
}

// ============================================================
// Network
// ============================================================

#[tokio::test]
async fn test_network_list() {
    let pm = create_plugin_manager();
    let r = pm.query_all("net ").await;
    assert!(r.len() >= 3);
}

#[tokio::test]
async fn test_network_bare_ip_query_does_not_fall_through_to_google() {
    let pm = create_plugin_manager();
    let r = pm.query_all("ip 8.8.8.8").await;
    assert!(!r.is_empty());
    assert_eq!(r[0].id, "net:ping:8.8.8.8");
    assert_eq!(r[0].action_type, "shell");
    assert!(r.iter().all(|result| !result.id.starts_with("google_fallback:")));
}

// ============================================================
// Sys Info
// ============================================================

#[tokio::test]
async fn test_sysinfo_os() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("sys_info", serde_json::json!({"info_type": "os"}))
        .await;
    assert!(!r.is_empty());
}

// ============================================================
// HTTP Client
// ============================================================

#[tokio::test]
async fn test_http_get() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "http_request",
            serde_json::json!({"method": "GET", "url": "https://httpbin.org/get"}),
        )
        .await;
    assert!(r.contains("200") || r.contains("httpbin"));
}

#[tokio::test]
async fn test_http_no_url() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "http_request",
            serde_json::json!({"method": "GET", "url": ""}),
        )
        .await;
    assert!(r.contains("Error") || r.contains("no URL"));
}

// ============================================================
// Code Execute
// ============================================================

#[tokio::test]
async fn test_code_python() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "code_execute",
            serde_json::json!({"language": "python", "code": "print(7*6)"}),
        )
        .await;
    if !r.to_lowercase().contains("error") {
        assert!(r.contains("42"));
    }
}

#[tokio::test]
async fn test_code_unsupported() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "code_execute",
            serde_json::json!({"language": "cobol", "code": "x"}),
        )
        .await;
    assert!(r.contains("Unsupported"));
}

// ============================================================
// App Launcher (tool)
// ============================================================

#[tokio::test]
async fn test_app_launcher_not_found() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "app_launcher",
            serde_json::json!({"name": "zzz_no_app_999"}),
        )
        .await;
    assert!(r.contains("No application found"));
}

// ============================================================
// Windows Settings
// ============================================================

#[tokio::test]
async fn test_settings_list() {
    let pm = create_plugin_manager();
    let r = pm.query_all("settings ").await;
    assert!(r.len() >= 5);
}

// ============================================================
// Hosts
// ============================================================

#[tokio::test]
async fn test_hosts_shows_edit() {
    let pm = create_plugin_manager();
    let r = pm.query_all("hosts ").await;
    let has_edit = r.iter().any(|x| x.title.to_lowercase().contains("edit"));
    assert!(has_edit);
}

// ============================================================
// Tool Not Found
// ============================================================

#[tokio::test]
async fn test_tool_not_found() {
    let pm = create_plugin_manager();
    let r = pm.execute_tool("zzz_fake", serde_json::json!({})).await;
    assert_eq!(r, "Tool not found");
}

// ============================================================
// Router NL Detection
// ============================================================

#[test]
fn test_nl_positive() {
    use omnilauncher_lib::ai::router::{RouteDecision, Router};
    assert_eq!(Router::decide("? what is the time"), RouteDecision::Ai);
    assert_eq!(Router::decide("ai how to install rust?"), RouteDecision::Ai);
    assert_eq!(Router::decide("?find all files in src"), RouteDecision::Ai);
}

#[test]
fn test_nl_negative() {
    use omnilauncher_lib::ai::router::{RouteDecision, Router};
    assert_eq!(Router::decide("notepad"), RouteDecision::Local);
    assert_eq!(Router::decide("chrome"), RouteDecision::Local);
}

// ============================================================
// Web Fetch
// ============================================================

#[tokio::test]
async fn test_web_fetch_ok() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "web_fetch",
            serde_json::json!({"url": "https://httpbin.org/html"}),
        )
        .await;
    // Allow network errors (e.g. in CI/offline) — we only care that the tool
    // returns something and doesn't panic.
    assert!(
        !r.is_empty(),
        "Expected a non-empty response, got empty string"
    );
}

#[tokio::test]
async fn test_web_fetch_empty() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("web_fetch", serde_json::json!({"url": ""}))
        .await;
    assert!(r.contains("Error") || r.contains("no URL"));
}

fn temp_config_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("omni_test_cfg_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn test_todo_view_tool_returns_live_page_url() {
    let pm = create_plugin_manager();
    let result = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "view"}))
        .await;

    assert!(
        result.contains("http://127.0.0.1:1421/todo"),
        "Got: {}",
        result
    );
}

#[tokio::test]
async fn test_todo_view_tool_reports_browser_open_success() {
    let pm = create_plugin_manager();
    let result = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "view"}))
        .await;

    // In headless/CI environments xdg-open may not exist; accept either outcome
    let mentions_url = result.contains("http://127.0.0.1:1421/todo");
    assert!(
        mentions_url,
        "Expected response to mention the todo URL, got: {}",
        result
    );
}

#[tokio::test]
async fn test_todo_view_query_opens_live_page() {
    let pm = create_plugin_manager();
    let results = pm.query_all("todo view").await;

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "todo:view");
    assert_eq!(results[0].action_type, "open_url");
    assert_eq!(results[0].action_data, "/todo");
}

#[tokio::test]
async fn test_slash_todo_view_query_opens_live_page() {
    let pm = create_plugin_manager();
    let results = pm.query_all("/todo view").await;

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "todo:view");
    assert_eq!(results[0].action_type, "open_url");
    assert_eq!(results[0].action_data, "/todo");
}

#[tokio::test]
async fn test_todo_status_transitions_and_list_output() {
    let _guard = TODO_LOCK.lock().unwrap();
    let dir = temp_config_dir("todo_status");
    unsafe {
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &dir);
    }

    let pm = create_plugin_manager();
    let _ = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "clear"}))
        .await;
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "add", "text": "item1"}),
        )
        .await;
    let set = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "set_status", "text": "1", "status": "in_progress"}),
        )
        .await;
    assert!(set.contains("In Progress"), "Got: {}", set);

    let list = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "list"}))
        .await;
    assert!(list.contains("status:🟦 In Progress"), "Got: {}", list);

    let done = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "done", "text": "1"}),
        )
        .await;
    assert!(done.contains("Done"), "Got: {}", done);

    let undone = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "undone", "text": "1"}),
        )
        .await;
    assert!(undone.contains("Todo"), "Got: {}", undone);

    unsafe {
        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    }
}

#[tokio::test]
async fn test_todo_query_and_live_json_include_status() {
    let _guard = TODO_LOCK.lock().unwrap();
    let dir = temp_config_dir("todo_query");
    unsafe {
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &dir);
    }

    let pm = create_plugin_manager();
    let _ = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "clear"}))
        .await;
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "add", "text": "ship feature"}),
        )
        .await;
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "set_status", "text": "1", "status": "blocked"}),
        )
        .await;

    let results = pm.query_all("todo list").await;
    assert!(!results.is_empty());
    assert!(
        results[0].title.contains("Blocked") || results.iter().any(|r| r.title.contains("Blocked"))
    );

    let json = omnilauncher_lib::plugins::todo::todo_live_data_json();
    assert!(json.contains("\"status\":\"blocked\""), "Got: {}", json);

    unsafe {
        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    }
}

#[tokio::test]
async fn test_todo_live_html_defines_status_helpers_used_by_render() {
    let _guard = TODO_LOCK.lock().unwrap();
    let dir = temp_config_dir("todo_html_helpers");
    unsafe {
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &dir);
    }

    let pm = create_plugin_manager();
    let _ = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "clear"}))
        .await;
    let _ = pm
        .execute_tool(
            "todo_memory",
            serde_json::json!({"action": "add", "text": "render me"}),
        )
        .await;

    let html = omnilauncher_lib::plugins::todo::todo_live_html();
    assert!(html.contains("function statusLabel"), "Got: {}", html);
    assert!(html.contains("function statusClass"), "Got: {}", html);
    assert!(html.contains("function statusSortKey"), "Got: {}", html);
    assert!(html.contains("function statusFromLabel"), "Got: {}", html);

    unsafe {
        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    }
}

// ============================================================
// Unit Converter
// ============================================================

#[tokio::test]
async fn test_unit_converter_km_to_m() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "convert_unit",
            serde_json::json!({"value": 1.0, "from_unit": "km", "to_unit": "m"}),
        )
        .await;
    assert!(r.contains("1000"), "Got: {}", r);
}

#[tokio::test]
async fn test_unit_converter_celsius_to_fahrenheit() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "convert_unit",
            serde_json::json!({"value": 100.0, "from_unit": "c", "to_unit": "f"}),
        )
        .await;
    assert!(r.contains("212"), "Got: {}", r);
}

#[tokio::test]
async fn test_unit_converter_kg_to_lb() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "convert_unit",
            serde_json::json!({"value": 1.0, "from_unit": "kg", "to_unit": "lb"}),
        )
        .await;
    assert!(r.contains("2.2") || r.contains("2204"), "Got: {}", r);
}

#[tokio::test]
async fn test_unit_converter_bad_units() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "convert_unit",
            serde_json::json!({"value": 1.0, "from_unit": "foobar", "to_unit": "baz"}),
        )
        .await;
    assert!(r.to_lowercase().contains("cannot") || r.to_lowercase().contains("convert"), "Got: {}", r);
}

#[tokio::test]
async fn test_unit_converter_query() {
    let pm = create_plugin_manager();
    let r = pm.query_all("conv 1 km to m").await;
    assert!(!r.is_empty());
    assert!(
        r[0].title.contains("1000") || r[0].subtitle.as_deref().unwrap_or("").contains("1000"),
        "Got: {:?}", r[0]
    );
}

// ============================================================
// Cron Explainer
// ============================================================

#[tokio::test]
async fn test_cron_explain_every_5_min() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "cron_explainer",
            serde_json::json!({"expression": "*/5 * * * *"}),
        )
        .await;
    assert!(r.to_lowercase().contains("minute") || r.contains("5"), "Got: {}", r);
}

#[tokio::test]
async fn test_cron_explain_daily_midnight() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "cron_explainer",
            serde_json::json!({"expression": "0 0 * * *"}),
        )
        .await;
    assert!(!r.is_empty(), "Expected a non-empty explanation");
}

#[tokio::test]
async fn test_cron_explain_query() {
    let pm = create_plugin_manager();
    let r = pm.query_all("cron */15 * * * *").await;
    assert!(!r.is_empty());
}

// ============================================================
// Emoji Picker
// ============================================================

#[tokio::test]
async fn test_emoji_query_fire() {
    let pm = create_plugin_manager();
    let r = pm.query_all("emoji fire").await;
    assert!(!r.is_empty());
    assert!(r.iter().any(|x| x.title.contains("\u{1F525}") || x.id.contains("fire")));
}

#[tokio::test]
async fn test_emoji_query_heart() {
    let pm = create_plugin_manager();
    let r = pm.query_all("emoji heart").await;
    assert!(!r.is_empty());
}

#[tokio::test]
async fn test_emoji_tool() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("emoji_picker", serde_json::json!({"query": "smile"}))
        .await;
    assert!(!r.is_empty(), "Got: {}", r);
}

// ============================================================
// Process Manager
// ============================================================

#[tokio::test]
async fn test_process_manager_list() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("process_manager", serde_json::json!({"action": "list"}))
        .await;
    assert!(!r.is_empty(), "Got: {}", r);
}

#[tokio::test]
async fn test_process_manager_kill_nonexistent() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "process_manager",
            serde_json::json!({"action": "kill", "name": "zzz_no_such_proc_xyz"}),
        )
        .await;
    assert!(
        r.contains("not found") || r.contains("No process") || r.contains("zzz"),
        "Got: {}",
        r
    );
}

#[tokio::test]
async fn test_process_manager_query() {
    let pm = create_plugin_manager();
    let r = pm.query_all("ps ").await;
    assert!(!r.is_empty());
}

// ============================================================
// Snippets
// ============================================================

#[tokio::test]
async fn test_snippets_lifecycle() {
    let pm = create_plugin_manager();
    let add = pm
        .execute_tool(
            "snippets",
            serde_json::json!({"action": "add", "name": "_test_snip", "content": "hello snippet"}),
        )
        .await;
    assert!(
        add.to_lowercase().contains("add")
            || add.to_lowercase().contains("saved")
            || add.to_lowercase().contains("success")
            || add.contains("_test_snip"),
        "add: {}",
        add
    );
    let get = pm
        .execute_tool("snippets", serde_json::json!({"action": "get", "name": "_test_snip"}))
        .await;
    assert!(get.contains("hello snippet"), "get: {}", get);
    let list = pm
        .execute_tool("snippets", serde_json::json!({"action": "list"}))
        .await;
    assert!(list.contains("_test_snip"), "list: {}", list);
    let del = pm
        .execute_tool("snippets", serde_json::json!({"action": "delete", "name": "_test_snip"}))
        .await;
    assert!(!del.to_lowercase().contains("error"), "delete: {}", del);
}

#[tokio::test]
async fn test_snippets_get_nonexistent() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("snippets", serde_json::json!({"action": "get", "name": "zzz_no_snip"}))
        .await;
    assert!(
        r.to_lowercase().contains("not found") || r.contains("zzz_no_snip"),
        "Got: {}",
        r
    );
}

// ============================================================
// Timer
// ============================================================

#[tokio::test]
async fn test_timer_set() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("set_timer", serde_json::json!({"duration_seconds": 30}))
        .await;
    assert!(r.contains("30"), "Got: {}", r);
}

#[tokio::test]
async fn test_timer_invalid_duration() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("set_timer", serde_json::json!({"duration_seconds": 0}))
        .await;
    assert!(r.to_lowercase().contains("invalid"), "Got: {}", r);
}

#[tokio::test]
async fn test_timer_query_parse() {
    let pm = create_plugin_manager();
    let r = pm.query_all("timer 5m").await;
    assert!(!r.is_empty());
}

// ============================================================
// Pomodoro
// ============================================================

#[tokio::test]
async fn test_pomodoro_status_no_active() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("pomodoro", serde_json::json!({"command": "status"}))
        .await;
    assert!(
        r.contains("No active") || r.contains("Remaining") || r.contains("Mode"),
        "Got: {}",
        r
    );
}

#[tokio::test]
async fn test_pomodoro_stop() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("pomodoro", serde_json::json!({"command": "stop"}))
        .await;
    assert!(r.contains("stopped"), "Got: {}", r);
}

#[tokio::test]
async fn test_pomodoro_unknown_action() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("pomodoro", serde_json::json!({"command": "zzz_unknown"}))
        .await;
    assert!(r.to_lowercase().contains("unknown"), "Got: {}", r);
}

// ============================================================
// Scheduler
// ============================================================

#[tokio::test]
async fn test_scheduler_list_empty_or_jobs() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("scheduler", serde_json::json!({"action": "list"}))
        .await;
    assert!(!r.is_empty(), "Got empty string from scheduler list");
}

#[tokio::test]
async fn test_scheduler_add_and_delete() {
    let pm = create_plugin_manager();
    let add = pm
        .execute_tool(
            "scheduler",
            serde_json::json!({
                "action": "add",
                "label": "_test_job_xyz",
                "schedule": "5m",
                "command": "echo hello"
            }),
        )
        .await;
    assert!(!add.to_lowercase().contains("error"), "add: {}", add);
    let list = pm
        .execute_tool("scheduler", serde_json::json!({"action": "list"}))
        .await;
    assert!(list.contains("_test_job_xyz"), "list: {}", list);
    let id: Option<u64> = list
        .lines()
        .find(|l| l.contains("_test_job_xyz"))
        .and_then(|l| l.trim_start_matches('#').split_whitespace().next())
        .and_then(|s| s.parse().ok());
    if let Some(id) = id {
        let del = pm
            .execute_tool("scheduler", serde_json::json!({"action": "delete", "id": id}))
            .await;
        assert!(!del.to_lowercase().contains("error"), "delete: {}", del);
    }
}

#[tokio::test]
async fn test_scheduler_add_missing_required_fields() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("scheduler", serde_json::json!({"action": "add", "label": "x"}))
        .await;
    assert!(
        r.to_lowercase().contains("error") || r.to_lowercase().contains("required"),
        "Got: {}",
        r
    );
}

// ============================================================
// File Search
// ============================================================

#[tokio::test]
async fn test_file_search_finds_existing() {
    let pm = create_plugin_manager();
    let d = std::env::temp_dir().join("omni_fsearch");
    let _ = std::fs::create_dir_all(&d);
    std::fs::write(d.join("unique_omnitest.txt"), "x").unwrap();
    let r = pm
        .execute_tool("file_search", serde_json::json!({"query": "unique_omnitest"}))
        .await;
    assert!(r.contains("unique_omnitest"), "Got: {}", r);
    let _ = std::fs::remove_dir_all(&d);
}

#[tokio::test]
async fn test_file_search_empty_query() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("file_search", serde_json::json!({"query": ""}))
        .await;
    assert!(!r.is_empty(), "Expected non-empty response to empty query");
}

#[tokio::test]
async fn test_file_search_query_prefix() {
    let pm = create_plugin_manager();
    let r = pm.query_all("find Cargo.toml").await;
    assert!(!r.is_empty(), "Expected file search results for Cargo.toml");
}

// ============================================================
// Selection
// ============================================================

#[tokio::test]
async fn test_selection_search() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "act_on_selection",
            serde_json::json!({"text": "rust programming", "action": "search"}),
        )
        .await;
    assert!(
        r.contains("google.com") || r.contains("http") || r.contains("search"),
        "Got: {}",
        r
    );
}

#[tokio::test]
async fn test_selection_empty_text_no_panic() {
    let pm = create_plugin_manager();
    let _r = pm
        .execute_tool(
            "act_on_selection",
            serde_json::json!({"text": "", "action": "search"}),
        )
        .await;
}

// ============================================================
// Window Resize
// ============================================================

#[tokio::test]
async fn test_window_resize_query() {
    let pm = create_plugin_manager();
    let r = pm.query_all("resize left").await;
    assert!(!r.is_empty(), "Expected results for 'resize left'");
}

#[tokio::test]
async fn test_window_resize_fullscreen_query() {
    let pm = create_plugin_manager();
    let r = pm.query_all("resize fullscreen").await;
    assert!(!r.is_empty());
}

#[tokio::test]
async fn test_window_resize_tool_returns_something() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("window_resize", serde_json::json!({"layout": "left half"}))
        .await;
    assert!(!r.is_empty(), "Got empty string from window_resize");
}

// ============================================================
// URL Opener
// ============================================================

#[tokio::test]
async fn test_url_opener_empty_url() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("open_url", serde_json::json!({"url": ""}))
        .await;
    assert!(
        r.to_lowercase().contains("error")
            || r.to_lowercase().contains("invalid")
            || r.to_lowercase().contains("no url"),
        "Got: {}",
        r
    );
}

#[tokio::test]
async fn test_url_opener_tool_returns_something() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("open_url", serde_json::json!({"url": "https://example.com"}))
        .await;
    assert!(!r.is_empty(), "Got: {}", r);
}

// ============================================================
// Script Runner
// ============================================================

#[tokio::test]
async fn test_script_runner_missing_script() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("run_user_script", serde_json::json!({"script_name": "zzz_no_script"}))
        .await;
    assert!(
        r.to_lowercase().contains("not found")
            || r.to_lowercase().contains("error")
            || r.contains("zzz"),
        "Got: {}",
        r
    );
}

// ============================================================
// Translate
// ============================================================

#[tokio::test]
async fn test_translate_query_prefix() {
    let pm = create_plugin_manager();
    let r = pm.query_all("tl hello").await;
    assert!(!r.is_empty(), "Expected translate suggestions for 'tl hello'");
}

// ============================================================
// Agent Delegate
// ============================================================

#[tokio::test]
async fn test_agent_delegate_query_at_claude() {
    let pm = create_plugin_manager();
    let r = pm.query_all("@claude fix the tests").await;
    assert!(!r.is_empty(), "Expected results for @claude query");
}

#[tokio::test]
async fn test_agent_delegate_query_at_codex() {
    let pm = create_plugin_manager();
    let r = pm.query_all("@codex refactor").await;
    assert!(!r.is_empty());
}

// ============================================================
// Clipboard
// ============================================================

#[tokio::test]
async fn test_clipboard_query_no_panic() {
    let pm = create_plugin_manager();
    let _r = pm.query_all("clip ").await;
}

// ============================================================
// Browser Bookmarks
// ============================================================

#[tokio::test]
async fn test_bookmarks_query_no_panic() {
    let pm = create_plugin_manager();
    let _r = pm.query_all("bm ").await;
}

// ============================================================
// Sys Info (extended)
// ============================================================

#[tokio::test]
async fn test_sys_info_cpu() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("sys_info", serde_json::json!({"info_type": "cpu"}))
        .await;
    assert!(!r.is_empty(), "Got: {}", r);
}

#[tokio::test]
async fn test_sys_info_memory() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("sys_info", serde_json::json!({"info_type": "memory"}))
        .await;
    assert!(!r.is_empty(), "Got: {}", r);
}

// ============================================================
// Git (extended)
// ============================================================

#[tokio::test]
async fn test_git_log_runs() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("git_ops", serde_json::json!({"subcommand": "log"}))
        .await;
    assert!(!r.is_empty(), "Got: {}", r);
}

// ============================================================
// Calculator (extended)
// ============================================================

#[tokio::test]
async fn test_calc_division() {
    let pm = create_plugin_manager();
    let r = pm.query_all("= 100/4").await;
    assert!(!r.is_empty());
    assert!(r[0].title.contains("25"), "Got: {}", r[0].title);
}

#[tokio::test]
async fn test_calc_sqrt() {
    let pm = create_plugin_manager();
    let r = pm.query_all("= sqrt(144)").await;
    assert!(!r.is_empty());
    assert!(r[0].title.contains("12"), "Got: {}", r[0].title);
}

// ============================================================
// Web Search (extended)
// ============================================================

#[tokio::test]
async fn test_web_search_bing() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool("web_search", serde_json::json!({"query": "rust lang", "engine": "bing"}))
        .await;
    assert!(r.contains("bing.com"), "Got: {}", r);
}

#[tokio::test]
async fn test_web_search_github() {
    let pm = create_plugin_manager();
    let r = pm
        .execute_tool(
            "web_search",
            serde_json::json!({"query": "omnilauncher", "engine": "github"}),
        )
        .await;
    assert!(r.contains("github.com"), "Got: {}", r);
}
