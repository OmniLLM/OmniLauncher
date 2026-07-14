pub mod a2a;
pub mod ai;
pub mod dashboard;
pub mod db;
pub mod gh_helper;
pub mod guardrails;
pub mod http_util;
pub mod launcher_config;
pub mod live_server;
pub mod log_masking;
pub mod mcp;
pub mod path_config;
pub mod plugins;
pub mod python_installer;
pub mod server;
pub mod settings;
pub mod skills;

pub use ai::router::{AiResponse, ConversationContext, Router};
pub use plugins::{PluginManager, QueryResult};
pub use settings::{
    load_settings, provider_caps, save_settings, AppSettings, McpOAuthConfig, McpServerConfig,
    Provider, ProviderKind,
};
pub use skills::{SkillInfo, SkillManager};

pub fn create_plugin_manager() -> PluginManager {
    let mut pm = create_plugin_manager_builtin_only();

    // Load external plugins: ~/.omnilauncher/plugins/ + any extra dirs from settings.plugin_dirs
    // register_override() handles all conflicts: same name or same keyword → evict existing.
    let settings = load_settings();
    for plugin in plugins::external::load_external_plugins_from(&settings.plugin_dirs) {
        pm.register_override(Box::new(plugin));
    }

    pm
}

/// Like `create_plugin_manager`, but skips the (potentially slow) external
/// plugin discovery. Use this when you want to bring the launcher window up
/// fast and load externals afterwards on a background task — see
/// `reload_external_plugins` in `main.rs`.
pub fn create_plugin_manager_builtin_only() -> PluginManager {
    let mut pm = PluginManager::new();
    pm.register(Box::new(plugins::agent_delegate::AgentDelegatePlugin));
    pm.register(Box::new(plugins::app_launcher::AppLauncherPlugin::new()));
    pm.register(Box::new(plugins::bash_exec::ShellExecPlugin));
    pm.register(Box::new(plugins::browser_bookmarks::BrowserBookmarksPlugin));
    pm.register(Box::new(plugins::calculator::CalculatorPlugin));
    pm.register(Box::new(plugins::clipboard::ClipboardPlugin::new()));
    pm.register(Box::new(plugins::code_tools::CodeExecPlugin));
    pm.register(Box::new(plugins::code_tools::PatchPlugin));
    pm.register(Box::new(plugins::color_picker::ColorPickerPlugin));
    pm.register(Box::new(plugins::env_vars::EnvVarsPlugin));
    pm.register(Box::new(plugins::file_read::FileReadPlugin));
    pm.register(Box::new(plugins::file_search::FileSearchPlugin));
    pm.register(Box::new(plugins::file_write::FileWritePlugin));
    pm.register(Box::new(plugins::git::GitPlugin));
    pm.register(Box::new(plugins::github::GitHubPlugin));
    pm.register(Box::new(plugins::glob::GlobPlugin));
    pm.register(Box::new(plugins::grep::GrepPlugin));
    pm.register(Box::new(plugins::hosts::HostsPlugin));
    pm.register(Box::new(plugins::http_client::HttpClientPlugin));
    pm.register(Box::new(plugins::ls::LsPlugin));
    pm.register(Box::new(plugins::network::NetworkPlugin));
    pm.register(Box::new(plugins::process_manager::ProcessManagerPlugin));
    pm.register(Box::new(plugins::shell_plugin::ShellPlugin));
    pm.register(Box::new(plugins::skill_runner::SkillRunnerPlugin));
    pm.register(Box::new(plugins::snippets::SnippetsPlugin));
    pm.register(Box::new(plugins::sys_info::SysInfoPlugin));
    pm.register(Box::new(plugins::system_commands::SystemCommandsPlugin));
    pm.register(Box::new(plugins::timer::TimerPlugin));
    pm.register(Box::new(plugins::todo::TodoPlugin));
    pm.register(Box::new(plugins::translate::TranslatePlugin));
    pm.register(Box::new(plugins::unit_converter::UnitConverterPlugin));
    pm.register(Box::new(plugins::url_opener::UrlOpenerPlugin));
    pm.register(Box::new(plugins::web_fetch::WebFetchPlugin));
    pm.register(Box::new(plugins::web_search::WebSearchPlugin));
    pm.register(Box::new(plugins::windows_settings::WindowsSettingsPlugin));
    pm.register(Box::new(plugins::script_runner::ScriptRunnerPlugin));
    pm.register(Box::new(plugins::screenshot::ScreenshotPlugin));
    pm.register(Box::new(plugins::selection::SelectionPlugin));
    pm.register(Box::new(plugins::emoji_picker::EmojiPickerPlugin));
    pm.register(Box::new(plugins::pomodoro::PomodoroPlugin));
    pm.register(Box::new(plugins::window_resize::WindowResizePlugin));
    pm.register(Box::new(plugins::cron_explainer::CronExplainerPlugin));
    pm.register(Box::new(plugins::scheduler::SchedulerPlugin));
    pm.register(Box::new(plugins::vision_analyze::VisionAnalyzePlugin));
    pm
}
