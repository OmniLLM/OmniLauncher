use omnilauncher_lib::create_plugin_manager;
use std::sync::Mutex;

static TODO_LOCK: Mutex<()> = Mutex::new(());

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
async fn test_plugin_manager_query_calculator() {
    let pm = create_plugin_manager();
    let results = pm.query_all("= 2+2").await;
    assert!(!results.is_empty());
    assert!(results[0].title.contains('4'));
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

    assert!(result.contains("http://127.0.0.1:1421/todo"), "Got: {}", result);
}

#[tokio::test]
async fn test_todo_view_tool_reports_browser_open_success() {
    let pm = create_plugin_manager();
    let result = pm
        .execute_tool("todo_memory", serde_json::json!({"action": "view"}))
        .await;

    assert!(result.contains("Opened"), "Expected direct success message, got: {}", result);
    assert!(result.contains("http://127.0.0.1:1421/todo"), "Got: {}", result);
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
