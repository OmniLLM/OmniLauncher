//! CLI management commands for app resources that are also managed by the UI.
//!
//! The terminal CLI is more than a lifecycle wrapper. Anything backend resources
//! can administer without visual state (settings, skills, external plugins,
//! plugin runtime dependencies) should also be manageable from the binary so
//! `make` can stay build/install-only.

use crate::cli::render::Output;
use omnilauncher_lib::{
    plugins::plugin_manager_cmd, provider_caps, AppSettings, Provider, ProviderKind, SkillInfo,
    SkillManager,
};

const SETTINGS_FIELDS: &[&str] = &[
    "ai_base_url",
    "ai_model",
    "ai_api_key",
    "providers",
    "active_provider_id",
    "ai_timeout_secs",
    "ai_max_tool_iterations",
    "ai_max_retry_attempts",
    "ai_retry_base_delay_ms",
    "ai_loop_detector_enabled",
    "theme",
    "hotkey",
    "max_results",
    "background_url",
    "plugin_dirs",
    "github_servers",
    "capture_selection_on_open",
    "backend_url",
    "a2a_enabled",
    "a2a_bind_lan",
    "a2a_port",
    "a2a_token",
    "a2a_public_url",
    "a2a_hub_url",
    "a2a_hub_admin_key",
    "a2a_hub_upstream_name",
    "a2a_hub_prefix",
    "a2a_hub_auto_register",
    "github_token",
    "github_server",
    "github_orgs",
];

/// Dispatch `ol settings ...`.
pub fn settings(out: &Output, args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        print_settings_help(out);
        return 0;
    };

    match cmd {
        "show" | "list" => settings_show(out),
        "get" => {
            let Some(field) = args.get(1) else {
                out.failure("usage: ol settings get <field>");
                return 2;
            };
            settings_get(out, field)
        }
        "set" => {
            let Some(field) = args.get(1) else {
                out.failure("usage: ol settings set <field> <json-or-string>");
                return 2;
            };
            let Some(value) = args.get(2) else {
                out.failure("usage: ol settings set <field> <json-or-string>");
                return 2;
            };
            settings_set(out, field, value)
        }
        "path" | "dir" => {
            println!("{}", omnilauncher_lib::settings::settings_path().display());
            0
        }
        "help" | "--help" | "-h" => {
            print_settings_help(out);
            0
        }
        other => {
            out.failure(&format!(
                "unknown settings command '{other}' — run `ol settings help`"
            ));
            2
        }
    }
}

/// Dispatch `ol skills ...`.
pub fn skills(out: &Output, args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        print_skills_help(out);
        return 0;
    };

    match cmd {
        "list" | "ls" => skills_list(out),
        "view" | "show" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol skills view <name>");
                return 2;
            };
            skills_view(out, name)
        }
        "install" | "add" => {
            let Some(source) = args.get(1) else {
                out.failure("usage: ol skills install <url-or-SKILL.md-path>");
                return 2;
            };
            skills_install(out, source)
        }
        "update" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol skills update <name>");
                return 2;
            };
            skills_update(out, name)
        }
        "usage" => skills_usage(out),
        "pin" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol skills pin <name>");
                return 2;
            };
            skills_pin(out, name, true)
        }
        "unpin" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol skills unpin <name>");
                return 2;
            };
            skills_pin(out, name, false)
        }
        "curator" => skills_curator(out, &args[1..]),
        "remove" | "delete" | "uninstall" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol skills remove <name>");
                return 2;
            };
            skills_remove(out, name)
        }
        "reload" => {
            // Reload is useful in long-lived processes because it refreshes
            // in-memory state. In a short-lived CLI process loading is already
            // fresh; keep the command as a no-op compatibility affordance.
            let mut mgr = loaded_skill_manager();
            mgr.reload();
            out.success(&format!("reloaded {} skill(s)", mgr.list_meta().len()));
            0
        }
        "dir" | "path" => {
            println!("{}", SkillManager::skill_dir().display());
            0
        }
        "help" | "--help" | "-h" => {
            print_skills_help(out);
            0
        }
        other => {
            out.failure(&format!(
                "unknown skills command '{other}' — run `ol skills help`"
            ));
            2
        }
    }
}

/// Dispatch `ol plugins ...`.
pub fn plugins(out: &Output, args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        print_plugins_help(out);
        return 0;
    };

    match cmd {
        "list" | "ls" => plugins_list(out),
        "collections" => plugins_collections(out),
        "install" | "add" => {
            let Some(source) = args.get(1) else {
                out.failure("usage: ol plugins install <url-or-path> [--target-dir <dir>]");
                return 2;
            };
            let target_dir = arg_value(&args[2..], "--target-dir");
            plugins_install(out, source, target_dir)
        }
        "update" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol plugins update <repo-dir-name>");
                return 2;
            };
            plugins_update(out, name)
        }
        "remove" | "delete" | "uninstall" => {
            let Some(name) = args.get(1) else {
                out.failure("usage: ol plugins remove <repo-dir-name>");
                return 2;
            };
            plugins_remove(out, name)
        }
        "update-collection" => plugins_update_collection(out, &args[1..]),
        "remove-collection" => plugins_remove_collection(out, &args[1..]),
        "runtimes" | "runtime-deps" => plugins_runtimes(out),
        "install-runtime" => {
            let Some(id) = args.get(1) else {
                out.failure("usage: ol plugins install-runtime <python|node|dotnet>");
                return 2;
            };
            plugins_install_runtime(out, id)
        }
        "dir" | "path" => {
            println!(
                "{}",
                omnilauncher_lib::plugins::external::ext_plugins_dir().display()
            );
            0
        }
        "help" | "--help" | "-h" => {
            print_plugins_help(out);
            0
        }
        other => {
            out.failure(&format!(
                "unknown plugins command '{other}' — run `ol plugins help`"
            ));
            2
        }
    }
}

/// Dispatch `ol providers ...`.
pub fn providers(out: &Output, args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        print_providers_help(out);
        return 0;
    };

    match cmd {
        "list" | "ls" => providers_list(out),
        "active" | "current" => providers_active(out),
        "add" => providers_add(out, &args[1..]),
        "set-active" | "select" | "use" => {
            let Some(id) = args.get(1) else {
                out.failure("usage: ol providers set-active <id>");
                return 2;
            };
            providers_set_active(out, id)
        }
        "set-model" | "model" => {
            let Some(model) = args.get(1) else {
                out.failure("usage: ol providers set-model <model> [provider-id]");
                return 2;
            };
            providers_set_model(out, model, args.get(2).map(String::as_str))
        }
        "remove" | "delete" => {
            let Some(id) = args.get(1) else {
                out.failure("usage: ol providers remove <id>");
                return 2;
            };
            providers_remove(out, id)
        }
        "caps" | "kinds" => providers_caps(out),
        "help" | "--help" | "-h" => {
            print_providers_help(out);
            0
        }
        other => {
            out.failure(&format!(
                "unknown providers command '{other}' — run `ol providers help`"
            ));
            2
        }
    }
}

fn providers_list(out: &Output) -> i32 {
    let settings = omnilauncher_lib::load_settings();
    let active = settings.active_provider_id.clone();
    let providers: Vec<Provider> = settings
        .providers
        .iter()
        .map(Provider::masked_for_display)
        .collect();
    if out.json {
        return print_json(
            out,
            &serde_json::json!({ "active_provider_id": active, "providers": providers }),
        );
    }
    if providers.is_empty() {
        out.info("No providers configured.");
        return 0;
    }
    println!(
        "  {:<2} {:<18} {:<16} {:<22} NAME",
        "", "ID", "KIND", "MODEL"
    );
    for p in providers {
        println!(
            "  {:<2} {:<18} {:<16} {:<22} {}",
            if p.id == active { "*" } else { "" },
            truncate(&p.id, 18),
            p.kind,
            truncate(&p.model, 22),
            p.name
        );
    }
    0
}

fn providers_active(out: &Output) -> i32 {
    let settings = omnilauncher_lib::load_settings();
    let provider = settings.active_provider().masked_for_display();
    if out.json {
        return print_json(out, &provider);
    }
    println!("id: {}", provider.id);
    println!("name: {}", provider.name);
    println!("kind: {}", provider.kind);
    println!("base_url: {}", provider.base_url);
    println!("model: {}", provider.model);
    let caps = provider_caps(provider.kind);
    println!(
        "caps: copilot_auth={} api_key={} auto_models={} manual_models={}",
        caps.uses_copilot_auth, caps.requires_api_key, caps.auto_list_models, caps.manual_models
    );
    0
}

fn providers_add(out: &Output, args: &[String]) -> i32 {
    let name = arg_value(args, "--name").or_else(|| args.first().cloned());
    let Some(name) = name else {
        out.failure("usage: ol providers add <name> --kind <custom|github-copilot|azure-foundry> [--base-url URL] [--api-key KEY] [--model MODEL] [--models a,b,c]");
        return 2;
    };
    let kind = match arg_value(args, "--kind")
        .unwrap_or_else(|| "custom".to_string())
        .parse::<ProviderKind>()
    {
        Ok(kind) => kind,
        Err(e) => {
            out.failure(&e);
            return 2;
        }
    };

    let mut settings = omnilauncher_lib::load_settings();
    let id = arg_value(args, "--id").unwrap_or_else(|| provider_id_from_name(&name, &settings));
    if settings.providers.iter().any(|p| p.id == id) {
        out.failure(&format!("provider id '{id}' already exists"));
        return 1;
    }
    let models = arg_value(args, "--models")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let model = arg_value(args, "--model")
        .or_else(|| models.first().cloned())
        .unwrap_or_else(|| "auto".to_string());
    let provider = Provider {
        id: id.clone(),
        name,
        kind,
        base_url: arg_value(args, "--base-url").unwrap_or_default(),
        api_key: arg_value(args, "--api-key").unwrap_or_default(),
        model,
        models,
        ..Provider::default()
    };
    settings.providers.push(provider);
    if settings.active_provider_id.is_empty() || args.iter().any(|a| a == "--active") {
        settings.active_provider_id = id.clone();
    }
    settings.sync_legacy_ai_fields_from_active_provider();
    if omnilauncher_lib::save_settings(&settings) {
        out.success(&format!("added provider: {id}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

fn providers_set_active(out: &Output, id: &str) -> i32 {
    let mut settings = omnilauncher_lib::load_settings();
    if !settings.providers.iter().any(|p| p.id == id) {
        out.failure(&format!("provider '{id}' not found"));
        return 1;
    }
    settings.active_provider_id = id.to_string();
    settings.sync_legacy_ai_fields_from_active_provider();
    if omnilauncher_lib::save_settings(&settings) {
        out.success(&format!("active provider: {id}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

fn providers_set_model(out: &Output, model: &str, provider_id: Option<&str>) -> i32 {
    let mut settings = omnilauncher_lib::load_settings();
    let id = provider_id
        .unwrap_or(&settings.active_provider_id)
        .to_string();
    let Some(provider) = settings.providers.iter_mut().find(|p| p.id == id) else {
        out.failure(&format!("provider '{id}' not found"));
        return 1;
    };
    provider.model = model.to_string();
    if settings.active_provider_id == id {
        settings.sync_legacy_ai_fields_from_active_provider();
    }
    if omnilauncher_lib::save_settings(&settings) {
        out.success(&format!("provider {id} model: {model}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

fn providers_remove(out: &Output, id: &str) -> i32 {
    let mut settings = omnilauncher_lib::load_settings();
    let before = settings.providers.len();
    settings.providers.retain(|p| p.id != id);
    if settings.providers.len() == before {
        out.failure(&format!("provider '{id}' not found"));
        return 1;
    }
    if settings.active_provider_id == id {
        settings.active_provider_id = settings
            .providers
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default();
    }
    settings.ensure_provider_registry();
    if omnilauncher_lib::save_settings(&settings) {
        out.success(&format!("removed provider: {id}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

fn providers_caps(out: &Output) -> i32 {
    let rows = [
        ("custom", provider_caps(ProviderKind::Custom)),
        ("github-copilot", provider_caps(ProviderKind::GithubCopilot)),
        ("azure-foundry", provider_caps(ProviderKind::AzureFoundry)),
    ];
    if out.json {
        return print_json(out, &rows);
    }
    println!(
        "  {:<16} {:<12} {:<8} {:<11} MANUAL_MODELS",
        "KIND", "COPILOT", "API_KEY", "AUTO_MODELS"
    );
    for (kind, caps) in rows {
        println!(
            "  {:<16} {:<12} {:<8} {:<11} {}",
            kind,
            caps.uses_copilot_auth,
            caps.requires_api_key,
            caps.auto_list_models,
            caps.manual_models
        );
    }
    0
}

fn provider_id_from_name(name: &str, settings: &AppSettings) -> String {
    let base = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let base = if base.is_empty() {
        "provider".to_string()
    } else {
        base
    };
    if !settings.providers.iter().any(|p| p.id == base) {
        return base;
    }
    for i in 2..1000 {
        let id = format!("{base}-{i}");
        if !settings.providers.iter().any(|p| p.id == id) {
            return id;
        }
    }
    format!("{base}-{}", settings.providers.len() + 1)
}

fn settings_show(out: &Output) -> i32 {
    let settings = omnilauncher_lib::load_settings();
    print_json(out, &settings)
}

fn settings_get(out: &Output, field: &str) -> i32 {
    let settings = omnilauncher_lib::load_settings();
    let value = match serde_json::to_value(settings) {
        Ok(value) => value,
        Err(e) => {
            out.failure(&format!("failed to encode settings: {e}"));
            return 1;
        }
    };
    if !SETTINGS_FIELDS.contains(&field) {
        out.failure(&format!("unknown settings field '{field}'"));
        return 1;
    };
    let v = value.get(field).unwrap_or(&serde_json::Value::Null);
    if out.json {
        return print_json(out, v);
    }
    match v {
        serde_json::Value::String(s) => println!("{s}"),
        other => println!("{other}"),
    }
    0
}

fn settings_set(out: &Output, field: &str, value: &str) -> i32 {
    let current = omnilauncher_lib::load_settings();
    let mut json = match serde_json::to_value(current) {
        Ok(serde_json::Value::Object(map)) => map,
        Ok(_) => {
            out.failure("settings did not serialize to an object");
            return 1;
        }
        Err(e) => {
            out.failure(&format!("failed to encode settings: {e}"));
            return 1;
        }
    };
    if !SETTINGS_FIELDS.contains(&field) {
        out.failure(&format!("unknown settings field '{field}'"));
        return 1;
    }
    let parsed = serde_json::from_str::<serde_json::Value>(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
    json.insert(field.to_string(), parsed);
    let updated: AppSettings = match serde_json::from_value(serde_json::Value::Object(json)) {
        Ok(settings) => settings,
        Err(e) => {
            out.failure(&format!("invalid value for '{field}': {e}"));
            return 1;
        }
    };
    if omnilauncher_lib::save_settings(&updated) {
        out.success(&format!("updated settings.{field}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

fn loaded_skill_manager() -> SkillManager {
    let mut mgr = SkillManager::new();
    mgr.load_all();
    mgr
}

fn skills_list(out: &Output) -> i32 {
    let mgr = loaded_skill_manager();
    let skills: Vec<SkillInfo> = mgr.list_meta().into_iter().map(SkillInfo::from).collect();
    if out.json {
        return print_json(out, &skills);
    }
    if skills.is_empty() {
        out.info("No skills installed.");
        return 0;
    }
    println!("  {:<28} {:<10} DESCRIPTION", "NAME", "VERSION");
    for s in skills {
        println!(
            "  {:<28} {:<10} {}",
            truncate(&s.name, 28),
            truncate(&s.version, 10),
            s.description
        );
    }
    0
}

fn skills_view(out: &Output, name: &str) -> i32 {
    let mgr = loaded_skill_manager();
    let Some(skill) = mgr.get_by_name(name) else {
        out.failure(&format!("skill '{name}' not found"));
        return 1;
    };
    if out.json {
        let payload = serde_json::json!({
            "meta": SkillInfo::from(&skill.meta),
            "body": skill.body.clone(),
        });
        return print_json(out, &payload);
    }
    println!("# {}", skill.meta.name);
    println!();
    println!("version: {}", skill.meta.version);
    println!("path: {}", skill.meta.path.display());
    if !skill.meta.description.is_empty() {
        println!("description: {}", skill.meta.description);
    }
    if !skill.meta.tags.is_empty() {
        println!("tags: {}", skill.meta.tags.join(", "));
    }
    println!();
    println!("{}", skill.body);
    0
}

fn skills_install(out: &Output, source: &str) -> i32 {
    let mut mgr = loaded_skill_manager();
    let result = if source.starts_with("http://") || source.starts_with("https://") {
        mgr.install_from_url(source)
    } else {
        mgr.install_from_path(source)
    };
    print_result(out, result)
}

fn skills_update(out: &Output, name: &str) -> i32 {
    let mut mgr = loaded_skill_manager();
    print_result(out, mgr.update_skill(name))
}

fn skills_usage(out: &Output) -> i32 {
    let usage = omnilauncher_lib::skills::curator::snapshot();
    if out.json {
        return print_json(out, &usage);
    }
    if usage.skills.is_empty() {
        out.info("No skill usage tracked yet.");
        return 0;
    }
    println!("  {:<28} {:<9} {:<9} PINNED", "NAME", "USES", "STATE");
    let mut rows = usage.skills.into_iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, u) in rows {
        println!(
            "  {:<28} {:<9} {:<9} {}",
            truncate(&name, 28),
            u.uses,
            format!("{:?}", u.state).to_lowercase(),
            if u.pinned { "yes" } else { "no" }
        );
    }
    0
}

fn skills_pin(out: &Output, name: &str, pinned: bool) -> i32 {
    omnilauncher_lib::skills::curator::set_pinned(name, pinned);
    out.success(&format!(
        "{} skill: {name}",
        if pinned { "pinned" } else { "unpinned" }
    ));
    0
}

fn skills_curator(out: &Output, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("run") {
        "run" => {
            let mgr = loaded_skill_manager();
            let names = mgr.user_skill_names();
            let report = omnilauncher_lib::skills::curator::evaluate(&names);
            if out.json {
                return print_json(
                    out,
                    &serde_json::json!({
                        "marked_stale": report.marked_stale,
                        "marked_archived": report.marked_archived,
                        "seen_new": report.seen_new,
                        "total_tracked": report.total_tracked,
                    }),
                );
            }
            out.success(&format!(
                "curator checked {} skill(s): {} new, {} stale, {} archived",
                report.total_tracked,
                report.seen_new.len(),
                report.marked_stale.len(),
                report.marked_archived.len()
            ));
            0
        }
        other => {
            out.failure(&format!(
                "unknown skills curator command '{other}' — expected `run`"
            ));
            2
        }
    }
}

fn skills_remove(out: &Output, name: &str) -> i32 {
    let mut mgr = loaded_skill_manager();
    print_result(out, mgr.delete_skill(name))
}

fn plugins_list(out: &Output) -> i32 {
    let plugins = plugin_manager_cmd::list_plugins();
    if out.json {
        return print_json(out, &plugins);
    }
    if plugins.is_empty() {
        out.info("No external plugins installed.");
        return 0;
    }
    println!("  {:<28} {:<7} {:<10} PLUGINS", "REPO", "GIT", "STATE");
    for repo in plugins {
        let repo_name = json_str(&repo, "dir_name");
        let git = if repo
            .get("is_git_repo")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            "yes"
        } else {
            "no"
        };
        let state = if repo
            .get("is_orphan")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            "orphan"
        } else {
            "ok"
        };
        let child_names = repo
            .get("plugins")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|p| json_str_opt(p, "name"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        println!(
            "  {:<28} {:<7} {:<10} {}",
            truncate(&repo_name, 28),
            git,
            state,
            child_names
        );
    }
    0
}

fn plugins_collections(out: &Output) -> i32 {
    let collections = plugin_manager_cmd::list_plugin_collections();
    if out.json {
        return print_json(out, &collections);
    }
    if collections.is_empty() {
        out.info("No plugin collections installed.");
        return 0;
    }
    println!("  {:<34} {:<7} {:<7} KEY", "COLLECTION", "REPOS", "PLUGINS");
    for c in collections {
        println!(
            "  {:<34} {:<7} {:<7} {}",
            truncate(&c.name, 34),
            c.repos.len(),
            c.plugins.len(),
            c.key
        );
    }
    0
}

fn plugins_install(out: &Output, source: &str, target_dir: Option<String>) -> i32 {
    let result = block_on(plugin_manager_cmd::install_plugin(
        source.to_string(),
        target_dir,
    ));
    print_result(out, result)
}

fn plugins_update(out: &Output, name: &str) -> i32 {
    let result = block_on(plugin_manager_cmd::update_plugin(name.to_string()));
    print_result(out, result)
}

fn plugins_remove(out: &Output, name: &str) -> i32 {
    let name_owned = name.to_string();
    let result = block_on(async move {
        plugin_manager_cmd::remove_plugin(name_owned.clone())
            .await
            .map(|_| format!("Removed plugin: {name_owned}"))
    });
    print_result(out, result)
}

fn plugins_update_collection(out: &Output, args: &[String]) -> i32 {
    if args.is_empty() {
        out.failure("usage: ol plugins update-collection --source <url-or-path> <repo-dir>...");
        return 2;
    }
    let source = arg_value(args, "--source").or_else(|| arg_value(args, "-s"));
    let Some(source) = source else {
        out.failure("usage: ol plugins update-collection --source <url-or-path> <repo-dir>...");
        return 2;
    };
    let repo_dirs = positional_without_flag_values(args, &["--source", "-s"]);
    if repo_dirs.is_empty() {
        out.failure("usage: ol plugins update-collection --source <url-or-path> <repo-dir>...");
        return 2;
    }
    let result = block_on(plugin_manager_cmd::update_plugin_collection(
        source, repo_dirs,
    ));
    print_result(out, result)
}

fn plugins_remove_collection(out: &Output, args: &[String]) -> i32 {
    let repo_dirs = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect::<Vec<_>>();
    if repo_dirs.is_empty() {
        out.failure("usage: ol plugins remove-collection <repo-dir>...");
        return 2;
    }
    let result = block_on(plugin_manager_cmd::remove_plugin_collection(repo_dirs));
    if out.json {
        return match result {
            Ok(value) => print_json(out, &value),
            Err(e) => {
                out.failure(&e);
                1
            }
        };
    }
    match result {
        Ok(value) => {
            out.success(&value.message);
            0
        }
        Err(e) => {
            out.failure(&e);
            1
        }
    }
}

fn plugins_runtimes(out: &Output) -> i32 {
    let deps = omnilauncher_lib::plugins::runtime_deps::list_runtime_dependencies();
    if out.json {
        return print_json(out, &deps);
    }
    println!("  {:<10} {:<8} {:<11} DETAIL", "ID", "READY", "INSTALLABLE");
    for d in deps {
        println!(
            "  {:<10} {:<8} {:<11} {}",
            d.id,
            if d.installed { "yes" } else { "no" },
            if d.installable { "yes" } else { "no" },
            d.detail
        );
        if !d.installed {
            if let Some(cmd) = d.install_command {
                println!("  {:<10} {}", "", out.dim(&format!("manual: {cmd}")));
            }
        }
    }
    0
}

fn plugins_install_runtime(out: &Output, id: &str) -> i32 {
    match id {
        "python" => match block_on(omnilauncher_lib::python_installer::install_bundled_python()) {
            Ok(path) => {
                out.success(&format!("Python installed at {}", path.display()));
                0
            }
            Err(e) => {
                out.failure(&e);
                1
            }
        },
        "node" | "dotnet" => {
            use omnilauncher_lib::plugins::runtime_deps::{
                runtime_install_plan, runtime_manual_command,
            };
            let (program, args, display) = match runtime_install_plan(id) {
                Ok(plan) => plan,
                Err(e) => {
                    if let Some(manual) = runtime_manual_command(id) {
                        out.failure(&format!("{e}\nRun manually: {manual}"));
                    } else {
                        out.failure(&e);
                    }
                    return 1;
                }
            };
            out.info(&format!("running: {display}"));
            match std::process::Command::new(&program).args(&args).status() {
                Ok(status) if status.success() => {
                    out.success(&format!("installed runtime dependency: {id}"));
                    0
                }
                Ok(status) => {
                    out.failure(&format!("installer exited with {status}"));
                    1
                }
                Err(e) => {
                    out.failure(&format!("failed to run installer: {e}"));
                    1
                }
            }
        }
        other => {
            out.failure(&format!(
                "unknown runtime dependency '{other}' (expected python, node, dotnet)"
            ));
            2
        }
    }
}

fn print_settings_help(out: &Output) {
    println!("{}", out.cyan("ol settings — manage OmniLauncher settings"));
    println!();
    println!("USAGE");
    println!("  ol settings <COMMAND> [ARGS...]");
    println!();
    println!("COMMANDS");
    println!("  show                         print settings JSON");
    println!("  get <field>                  print one settings field");
    println!("  set <field> <json-or-string> set one field and save settings.json");
    println!("  path                         print settings.json path");
}

fn print_providers_help(out: &Output) {
    println!(
        "{}",
        out.cyan("ol providers — manage LLM providers and models")
    );
    println!();
    println!("USAGE");
    println!("  ol providers <COMMAND> [ARGS...]");
    println!();
    println!("COMMANDS");
    println!("  list                                      list configured providers");
    println!("  active                                    show the active provider");
    println!("  add <name> --kind <kind> [options]        add custom/github-copilot/azure-foundry provider");
    println!(
        "      options: --id ID --base-url URL --api-key KEY --model MODEL --models a,b,c --active"
    );
    println!("  set-active <id>                           switch active provider");
    println!("  set-model <model> [provider-id]           update selected model");
    println!("  remove <id>                               remove provider");
    println!("  caps                                      show provider kind capabilities");
}

fn print_skills_help(out: &Output) {
    println!("{}", out.cyan("ol skills — manage OmniLauncher skills"));
    println!();
    println!("USAGE");
    println!("  ol skills <COMMAND> [ARGS...]");
    println!();
    println!("COMMANDS");
    println!("  list                 list installed skills");
    println!("  view <name>          print one skill's metadata and body");
    println!("  install <source>     install from URL or local SKILL.md");
    println!("  update <name>        update a URL-installed skill");
    println!("  remove <name>        remove a user-installed skill");
    println!("  usage                show usage/pin/archive state");
    println!("  pin|unpin <name>     pin or unpin a skill");
    println!("  curator run          run the rule-based skill curator");
    println!("  reload               verify skills reload cleanly");
    println!("  dir                  print the skills directory");
}

fn print_plugins_help(out: &Output) {
    println!(
        "{}",
        out.cyan("ol plugins — manage OmniLauncher external plugins")
    );
    println!();
    println!("USAGE");
    println!("  ol plugins <COMMAND> [ARGS...]");
    println!();
    println!("COMMANDS");
    println!("  list                                      list installed plugin repos");
    println!("  collections                               list grouped plugin collections");
    println!("  install <source> [--target-dir <dir>]     install from URL or local path");
    println!("  update <repo-dir-name>                    git pull / rebuild one plugin repo");
    println!("  remove <repo-dir-name>                    remove one plugin repo");
    println!(
        "  update-collection --source <src> <dir>... update selected dirs from a collection source"
    );
    println!("  remove-collection <dir>...                remove several plugin repos");
    println!("  runtimes                                  list Python/Node/.NET readiness");
    println!("  install-runtime <python|node|dotnet>      install a runtime dependency");
    println!("  dir                                       print the plugin directory");
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new()
        .expect("failed to create tokio runtime")
        .block_on(future)
}

fn print_result(out: &Output, result: Result<String, String>) -> i32 {
    match result {
        Ok(msg) => {
            if out.json {
                print_json(out, &serde_json::json!({ "ok": true, "message": msg }))
            } else {
                out.success(&msg);
                0
            }
        }
        Err(e) => {
            if out.json {
                let _ = print_json(out, &serde_json::json!({ "ok": false, "error": e }));
            } else {
                out.failure(&e);
            }
            1
        }
    }
}

fn print_json<T: serde::Serialize>(out: &Output, value: &T) -> i32 {
    match serde_json::to_string_pretty(value) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            out.failure(&format!("failed to encode JSON: {e}"));
            1
        }
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn positional_without_flag_values(args: &[String], flags_with_value: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if flags_with_value.contains(&arg.as_str()) {
            skip_next = true;
            continue;
        }
        if !arg.starts_with('-') {
            out.push(arg.clone());
        }
    }
    out
}

fn json_str(value: &serde_json::Value, key: &str) -> String {
    json_str_opt(value, key).unwrap_or_default()
}

fn json_str_opt(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let mut out = s.chars().take(max - 1).collect::<String>();
    out.push('…');
    out
}
