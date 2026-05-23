use omnilauncher_lib::{
    create_plugin_manager, load_settings, save_settings,
    ai::{client::AiClient, router::Router},
    QueryResult, AppSettings,
};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub plugin_manager: Arc<Mutex<omnilauncher_lib::PluginManager>>,
    pub ai_client: Arc<AiClient>,
    pub settings: Arc<Mutex<AppSettings>>,
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
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            search,
            ai_query,
            execute_result,
            get_settings,
            save_settings_cmd,
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
    let pm = state.plugin_manager.lock().await;
    Ok(Router::route(&query, &pm, &state.ai_client).await)
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
