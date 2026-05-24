use omnilauncher_lib::{
    ai::{
        client::AiClient,
        router::{ConversationContext, Router},
    },
    create_plugin_manager, load_settings, save_settings, AppSettings, QueryResult,
};
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tokio::sync::Mutex;

pub struct AppState {
    pub plugin_manager: Arc<Mutex<omnilauncher_lib::PluginManager>>,
    pub ai_client: Mutex<AiClient>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub conversation: Arc<Mutex<ConversationContext>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = load_settings();
    let ai_client = AiClient::new(
        settings.ai_base_url.clone(),
        settings.ai_api_key.clone(),
        settings.ai_model.clone(),
    );

    let state = AppState {
        plugin_manager: Arc::new(Mutex::new(create_plugin_manager())),
        ai_client: Mutex::new(ai_client),
        settings: Arc::new(Mutex::new(settings)),
        conversation: Arc::new(Mutex::new(ConversationContext::default())),
    };

    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyO);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            let window = app.get_webview_window("main").unwrap();

            // Resize window to 1/2 of screen
            if let Ok(Some(monitor)) = window.current_monitor() {
                let screen_size = monitor.size();
                let scale = monitor.scale_factor();
                let width = (screen_size.width as f64 / scale) / 2.0;
                let height = (screen_size.height as f64 / scale) / 2.0;
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
                let _ = window.center();
            }

            let global_shortcut = app.global_shortcut();

            global_shortcut
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    if let tauri_plugin_global_shortcut::ShortcutState::Pressed = event.state() {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .expect("Failed to register global shortcut");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            ai_query,
            execute_result,
            get_settings,
            save_settings_cmd,
            clear_conversation,
            list_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn search(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QueryResult>, String> {
    let pm = state.plugin_manager.lock().await;
    Ok(pm.query_all(&query).await)
}

#[tauri::command]
async fn ai_query(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<omnilauncher_lib::AiResponse, String> {
    // Add to conversation context
    {
        let mut ctx = state.conversation.lock().await;
        ctx.add_user(&query);
    }

    let pm = state.plugin_manager.lock().await;
    let client = state.ai_client.lock().await;
    let ctx = state.conversation.lock().await;
    // Always use AI route in chat mode (bypass NL detection)
    let response = Router::ai_route(&pm, &client, &ctx).await;
    drop(ctx);

    // Add assistant response to context
    {
        let mut ctx = state.conversation.lock().await;
        ctx.add_assistant(&response.content);
    }

    Ok(response)
}

#[tauri::command]
async fn clear_conversation(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let mut ctx = state.conversation.lock().await;
    ctx.clear();
    Ok(true)
}

#[tauri::command]
async fn execute_result(result: QueryResult) -> Result<bool, String> {
    use std::process::Command;
    let success = match result.action_type.as_str() {
        "url" | "open_url" => {
            #[cfg(target_os = "linux")]
            let _ = Command::new("xdg-open").arg(&result.action_data).spawn();
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(&result.action_data).spawn();
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd")
                .args(["/C", "start", "", &result.action_data])
                .spawn();
            true
        }
        "shell" | "open_app" => {
            // Like Flow.Launcher: use ShellExecute to open the file (.lnk, .exe, etc.)
            // `cmd /c start "" "path"` invokes ShellExecuteEx which resolves .lnk shortcuts
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd")
                .args(["/C", "start", "", &result.action_data])
                .spawn();
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(&result.action_data).spawn();
            #[cfg(target_os = "linux")]
            let _ = Command::new("sh")
                .arg("-c")
                .arg(&result.action_data)
                .spawn();
            true
        }
        "open" => {
            #[cfg(target_os = "linux")]
            let _ = Command::new("xdg-open").arg(&result.action_data).spawn();
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(&result.action_data).spawn();
            #[cfg(target_os = "windows")]
            let _ = Command::new("explorer").arg(&result.action_data).spawn();
            true
        }
        "copy" => {
            // Just a copy action — frontend handles clipboard
            true
        }
        _ => false,
    };
    Ok(success)
}

#[tauri::command]
async fn list_models(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.bearer_auth(&api_key);
    }

    let response = req.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, text));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let models = json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().await;
    Ok(settings.clone())
}

#[tauri::command]
async fn save_settings_cmd(
    settings: AppSettings,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let mut current = state.settings.lock().await;
    *current = settings.clone();
    // Recreate AiClient with new settings
    let mut client = state.ai_client.lock().await;
    *client = AiClient::new(
        settings.ai_base_url.clone(),
        settings.ai_api_key.clone(),
        settings.ai_model.clone(),
    );
    Ok(save_settings(&settings))
}

fn main() {
    run();
}
