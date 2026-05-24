pub mod ai;
pub mod plugins;
pub mod settings;

pub use ai::router::{AiResponse, ConversationContext, Router};
pub use plugins::{PluginManager, QueryResult};
pub use settings::{load_settings, save_settings, AppSettings};

pub fn create_plugin_manager() -> PluginManager {
    let mut pm = PluginManager::new();
    pm.register(Box::new(plugins::app_launcher::AppLauncherPlugin::new()));
    pm.register(Box::new(plugins::web_search::WebSearchPlugin));
    pm.register(Box::new(plugins::calculator::CalculatorPlugin));
    pm.register(Box::new(plugins::file_search::FileSearchPlugin));
    pm.register(Box::new(plugins::shell_plugin::ShellPlugin));
    pm.register(Box::new(plugins::system_commands::SystemCommandsPlugin));
    pm.register(Box::new(plugins::clipboard::ClipboardPlugin::new()));
    pm
}
