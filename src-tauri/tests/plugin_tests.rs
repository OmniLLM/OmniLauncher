use omnilauncher_lib::plugins::calculator::evaluate;
use omnilauncher_lib::plugins::web_search::WebSearchPlugin;
use omnilauncher_lib::plugins::{Plugin, PluginManager, Query};
use omnilauncher_lib::ai::router::Router;
use omnilauncher_lib::settings::{load_settings, save_settings, AppSettings, settings_path};

// ---- Calculator tests ----

#[test]
fn test_calculator_basic() {
    assert_eq!(evaluate("2 + 2"), Some(4.0));
    assert_eq!(evaluate("10 - 3"), Some(7.0));
    assert_eq!(evaluate("3 * 4"), Some(12.0));
    assert_eq!(evaluate("10 / 2"), Some(5.0));
}

#[test]
fn test_calculator_expression() {
    let result = evaluate("(2 + 3) * 4");
    assert_eq!(result, Some(20.0));
    let result2 = evaluate("2 ^ 10");
    assert_eq!(result2, Some(1024.0));
    let result3 = evaluate("100 / 4 + 5 * 2");
    assert_eq!(result3, Some(35.0));
}

// ---- Web Search tests ----

#[tokio::test]
async fn test_web_search_google_prefix() {
    let plugin = WebSearchPlugin;
    let q = Query {
        raw: "g rust programming".to_string(),
        terms: vec!["g".to_string(), "rust".to_string(), "programming".to_string()],
    };
    let results = plugin.query(&q).await;
    assert!(!results.is_empty());
    assert!(results[0].action_data.contains("google.com"));
    assert!(results[0].action_data.contains("rust"));
    assert_eq!(results[0].score, 90);
}

#[tokio::test]
async fn test_web_search_youtube_prefix() {
    let plugin = WebSearchPlugin;
    let q = Query {
        raw: "yt lofi music".to_string(),
        terms: vec!["yt".to_string(), "lofi".to_string()],
    };
    let results = plugin.query(&q).await;
    assert!(!results.is_empty());
    assert!(results[0].action_data.contains("youtube.com"));
}

#[tokio::test]
async fn test_web_search_github_prefix() {
    let plugin = WebSearchPlugin;
    let q = Query {
        raw: "gh tauri".to_string(),
        terms: vec!["gh".to_string(), "tauri".to_string()],
    };
    let results = plugin.query(&q).await;
    assert!(!results.is_empty());
    assert!(results[0].action_data.contains("github.com"));
}

#[tokio::test]
async fn test_web_search_fallback() {
    let plugin = WebSearchPlugin;
    let q = Query {
        raw: "rust async".to_string(),
        terms: vec!["rust".to_string(), "async".to_string()],
    };
    let results = plugin.query(&q).await;
    assert!(!results.is_empty());
    assert!(results[0].action_data.contains("google.com"));
    assert_eq!(results[0].score, 30); // fallback score
}

// ---- Plugin Manager routing tests ----

#[tokio::test]
async fn test_plugin_manager_keyword_routing() {
    use omnilauncher_lib::plugins::system_commands::SystemCommandsPlugin;

    let mut pm = PluginManager::new();
    pm.register(Box::new(SystemCommandsPlugin));
    pm.register(Box::new(WebSearchPlugin));

    // sys prefix should only return system commands
    let results = pm.query_all("sys lock").await;
    assert!(results.iter().any(|r| r.id.contains("sys:")));

    // web fallback
    let results2 = pm.query_all("hello world").await;
    assert!(results2.iter().any(|r| r.id.contains("google")));
}

// ---- File search ----

#[tokio::test]
async fn test_file_search_returns_results() {
    use omnilauncher_lib::plugins::file_search::FileSearchPlugin;
    let plugin = FileSearchPlugin;

    // Search for something that likely exists
    let q = Query {
        raw: "f .bashrc".to_string(),
        terms: vec!["f".to_string(), ".bashrc".to_string()],
    };
    // We just check it doesn't panic; may or may not find files
    let _results = plugin.query(&q).await;

    // Search with open prefix
    let q2 = Query {
        raw: "open README".to_string(),
        terms: vec!["open".to_string(), "README".to_string()],
    };
    let _results2 = plugin.query(&q2).await;
}

// ---- Settings tests ----

#[test]
fn test_settings_save_load() {
    let mut settings = AppSettings::default();
    settings.ai_base_url = "http://test-server:9999".to_string();
    settings.ai_model = "test-model".to_string();
    settings.theme = "light".to_string();

    let saved = save_settings(&settings);
    assert!(saved, "Failed to save settings");

    let loaded = load_settings();
    assert_eq!(loaded.ai_base_url, "http://test-server:9999");
    assert_eq!(loaded.ai_model, "test-model");
    assert_eq!(loaded.theme, "light");

    // Cleanup: restore defaults
    save_settings(&AppSettings::default());
}

// ---- AI Router detection tests ----

#[test]
fn test_ai_router_detects_natural_language() {
    assert!(Router::is_natural_language("find all rust files in my project"));
    assert!(Router::is_natural_language("show me the latest news about AI"));
    assert!(Router::is_natural_language("what is the capital of France?"));
    assert!(Router::is_natural_language("how do I install Rust on Ubuntu"));
    assert!(Router::is_natural_language("help me write a cover letter"));
    // Long query (> 20 chars)
    assert!(Router::is_natural_language("this is a very long query that exceeds twenty chars"));
}

#[test]
fn test_ai_router_detects_keyword_query() {
    assert!(!Router::is_natural_language("g rust"));
    assert!(!Router::is_natural_language("= 2+2"));
    assert!(!Router::is_natural_language("firefox"));
    assert!(!Router::is_natural_language("sys lock"));
    assert!(!Router::is_natural_language("> ls -la"));
}
