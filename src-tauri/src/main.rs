use omnilauncher_lib::{
    create_plugin_manager, load_settings, save_settings,
    ai::{client::AiClient, router::{Router, ConversationContext}},
    QueryResult, AppSettings,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, Code, Modifiers};

pub struct AppState {
    pub plugin_manager: Arc<Mutex<omnilauncher_lib::PluginManager>>,
    pub ai_client: Arc<AiClient>,
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
        ai_client: Arc::new(ai_client),
        settings: Arc::new(Mutex::new(settings)),
        conversation: Arc::new(Mutex::new(ConversationContext::default())),
    };

    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyO);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state)
        .setup(move |app| {
            let window = app.get_webview_window("main").unwrap();
            let global_shortcut = app.global_shortcut();

            global_shortcut.on_shortcut(shortcut, move |_app, _shortcut, event| {
                if let tauri_plugin_global_shortcut::ShortcutState::Pressed = event.state() {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }).expect("Failed to register global shortcut");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            ai_query,
            execute_result,
            get_settings,
            save_settings_cmd,
            clear_conversation,
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
    window: tauri::WebviewWindow,
) -> Result<omnilauncher_lib::AiResponse, String> {
    // Add to conversation context
    {
        let mut ctx = state.conversation.lock().await;
        ctx.add_user(&query);
    }

    let pm = state.plugin_manager.lock().await;
    let response = Router::route(&query, &pm, &state.ai_client).await;

    // Add assistant response to context
    {
        let mut ctx = state.conversation.lock().await;
        ctx.add_assistant(&response.content);
    }

    // Emit streaming events for the response content
    if !response.content.is_empty() {
        // Simulate streaming by emitting chunks
        for tool in &response.tools_used {
            let _ = window.emit("ai-tool-call", tool.clone());
        }
        let _ = window.emit("ai-stream", response.content.clone());
        let _ = window.emit("ai-stream-done", "".to_string());
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
        "url" => {
            #[cfg(target_os = "linux")]
            let _ = Command::new("xdg-open").arg(&result.action_data).spawn();
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(&result.action_data).spawn();
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd").args(["/C", "start", &result.action_data]).spawn();
            true
        }
        "shell" => {
            #[cfg(not(target_os = "windows"))]
            let _ = Command::new("sh").arg("-c").arg(&result.action_data).spawn();
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd").args(["/C", &result.action_data]).spawn();
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
        _ => false,
    };
    Ok(success)
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> AppSettings {
    let settings = state.settings.blocking_lock();
    settings.clone()
}

#[tauri::command]
async fn save_settings_cmd(
    settings: AppSettings,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let mut current = state.settings.lock().await;
    *current = settings.clone();
    Ok(save_settings(&settings))
}

fn main() {
    run();
}
