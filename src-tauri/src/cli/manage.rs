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
use std::io::IsTerminal;

pub(crate) const SETTINGS_FIELDS: &[&str] = &[
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
    "mcp_servers",
    "capture_selection_on_open",
    "backend_url",
    "a2a_enabled",
    "a2a_bind_lan",
    "a2a_port",
    "a2a_token",
    "a2a_public_url",
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

/// Ask the running backend which MCP servers are connected. Returns `None`
/// when the backend is unreachable or unauthenticated, so callers can present
/// "unavailable" rather than misreporting every server as disconnected.
fn fetch_mcp_statuses(
    settings: &omnilauncher_lib::settings::AppSettings,
) -> Option<std::collections::HashMap<String, omnilauncher_lib::mcp::McpServerStatus>> {
    let token = settings.a2a_token.as_deref().filter(|t| !t.trim().is_empty())?;
    let url = format!("http://127.0.0.1:{}", settings.a2a_port);
    let body = super::process::http_get(&url, "/mcp/status", Some(token)).ok()?;
    serde_json::from_str(&body).ok()
}

/// Dispatch `ol mcp ...`.
pub fn mcp(out: &Output, args: &[String]) -> i32 {
    let settings = omnilauncher_lib::load_settings();
    match args.first().map(String::as_str) {
        Some("add") => mcp_add(out, &args[1..], false),
        Some("update" | "set") => mcp_add(out, &args[1..], true),
        Some("remove" | "rm" | "delete") => mcp_remove(out, args.get(1)),
        Some("show" | "get") => mcp_show(out, args.get(1)),
        Some("list" | "ls") => {
            if settings.mcp_servers.is_empty() {
                out.info("no MCP servers configured");
            } else {
                // Connectivity lives in the backend process, not here. When it
                // is down we show config only rather than implying everything
                // is disconnected.
                let statuses = fetch_mcp_statuses(&settings);
                for (name, config) in &settings.mcp_servers {
                    // stdio servers have no URL; show the command line instead
                    // so `list` identifies every server the same way.
                    let target = if config.transport_type.eq_ignore_ascii_case("stdio") {
                        let mut parts = vec![config.command.clone()];
                        parts.extend(config.args.iter().cloned());
                        parts.join(" ")
                    } else {
                        config.url.clone()
                    };
                    let status = statuses.as_ref().map(|map| match map.get(name) {
                        Some(status) if status.connected => {
                            format!("connected ({} tools)", status.tool_count)
                        }
                        Some(status) if status.needs_login => {
                            "not connected — auth required".to_string()
                        }
                        Some(_) => "not connected".to_string(),
                        // Configured but absent from the backend's map: it has
                        // not attempted this server since the config changed.
                        None => "unknown".to_string(),
                    });
                    println!(
                        "{:<24} {:<6} {}{}{}",
                        name,
                        config.transport_type,
                        target,
                        if config.oauth.is_some() {
                            "  oauth"
                        } else {
                            ""
                        },
                        status
                            .map(|status| format!("  {status}"))
                            .unwrap_or_default(),
                    );
                }
                if statuses.is_none() {
                    out.info("backend not running — connection status unavailable");
                } else {
                    let needs_login: Vec<&String> = settings
                        .mcp_servers
                        .keys()
                        .filter(|name| {
                            statuses
                                .as_ref()
                                .and_then(|map| map.get(*name))
                                .is_some_and(|status| status.needs_login)
                        })
                        .collect();
                    for name in needs_login {
                        out.info(&format!("run `ol mcp login {name}` to authorize"));
                    }
                }
            }
            0
        }
        Some("login") | Some("logout") => {
            let action = args[0].as_str();
            let Some(name) = args.get(1) else {
                out.failure(&format!("usage: ol mcp {action} <server-name>"));
                return 2;
            };
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    out.failure(&format!("failed to start async runtime: {error}"));
                    return 1;
                }
            };
            let result = if action == "login" {
                runtime.block_on(omnilauncher_lib::mcp::login(&settings, name))
            } else {
                runtime.block_on(omnilauncher_lib::mcp::logout(&settings, name))
            };
            match result {
                Ok(message) => {
                    out.success(&message);
                    0
                }
                Err(error) => {
                    out.failure(&error);
                    1
                }
            }
        }
        Some("help" | "--help" | "-h") | None => {
            println!("MCP server management");
            println!("  ol mcp list");
            println!("  ol mcp show <server-name>");
            println!("  ol mcp add <server-name> <url> [--type http] [--header K=V]...");
            println!("                            [--oauth-client-id ID] [--oauth-client-secret-env VAR]");
            println!("                            [--oauth-scope SCOPE]... [--no-oauth]");
            println!("  ol mcp add <server-name> --type stdio --command CMD [--arg VALUE]... [--env K=V]...");
            println!("                            [--remove-env K]...");
            println!("  ol mcp update <server-name> [same flags as add; url optional]");
            println!("  ol mcp remove <server-name>");
            println!("  ol mcp login <server-name>");
            println!("  ol mcp logout <server-name>");
            0
        }
        Some(other) => {
            out.failure(&format!(
                "unknown MCP command '{other}' — run `ol mcp help`"
            ));
            2
        }
    }
}

/// Create or update one entry in `settings.mcp_servers`.
///
/// `update` mode keeps every field that the caller did not mention, so a
/// single flag can be changed without re-declaring the whole server.
fn mcp_add(out: &Output, args: &[String], update: bool) -> i32 {
    let verb = if update { "update" } else { "add" };
    let Some(name) = args.first().filter(|n| !n.starts_with('-')).cloned() else {
        out.failure(&format!("usage: ol mcp {verb} <server-name> ..."));
        return 2;
    };

    let mut settings = omnilauncher_lib::load_settings();
    let exists = settings.mcp_servers.contains_key(&name);
    if update && !exists {
        out.failure(&format!("MCP server '{name}' is not configured"));
        return 1;
    }
    if !update && exists {
        out.failure(&format!(
            "MCP server '{name}' already exists — use `ol mcp update {name}`"
        ));
        return 1;
    }

    let mut config = settings.mcp_servers.get(&name).cloned().unwrap_or_default();
    let mut rest = &args[1..];

    // In add mode the URL is a required positional argument for http servers.
    // stdio servers have no URL, so the requirement is checked after flags are
    // parsed and the transport is known.
    if let Some(first) = rest.first() {
        if !first.starts_with('-') {
            config.url = first.clone();
            rest = &rest[1..];
        }
    }

    let mut oauth = config.oauth.clone();
    let mut index = 0usize;
    while index < rest.len() {
        let flag = rest[index].as_str();
        let take_value = |out: &Output, index: &mut usize| -> Option<String> {
            let value = rest.get(*index + 1).cloned();
            if value.is_none() {
                out.failure(&format!("{flag} requires a value"));
            }
            *index += 2;
            value
        };
        match flag {
            "--url" => match take_value(out, &mut index) {
                Some(value) => config.url = value,
                None => return 2,
            },
            "--type" => match take_value(out, &mut index) {
                Some(value) => config.transport_type = value,
                None => return 2,
            },
            "--header" => match take_value(out, &mut index) {
                Some(value) => match value.split_once('=') {
                    Some((key, header_value)) => {
                        config
                            .headers
                            .insert(key.trim().to_string(), header_value.to_string());
                    }
                    None => {
                        out.failure("--header expects KEY=VALUE");
                        return 2;
                    }
                },
                None => return 2,
            },
            "--remove-header" => match take_value(out, &mut index) {
                Some(value) => {
                    config.headers.remove(value.trim());
                }
                None => return 2,
            },
            "--command" => match take_value(out, &mut index) {
                Some(value) => config.command = value,
                None => return 2,
            },
            "--arg" => match take_value(out, &mut index) {
                Some(value) => config.args.push(value),
                None => return 2,
            },
            "--clear-args" => {
                config.args.clear();
                index += 1;
            }
            "--env" => match take_value(out, &mut index) {
                Some(value) => match value.split_once('=') {
                    Some((key, env_value)) => {
                        config
                            .env
                            .insert(key.trim().to_string(), env_value.to_string());
                    }
                    None => {
                        out.failure("--env expects KEY=VALUE");
                        return 2;
                    }
                },
                None => return 2,
            },
            "--remove-env" => match take_value(out, &mut index) {
                Some(value) => {
                    config.env.remove(value.trim());
                }
                None => return 2,
            },
            "--oauth-client-id" => match take_value(out, &mut index) {
                Some(value) => oauth.get_or_insert_with(Default::default).client_id = value,
                None => return 2,
            },
            "--oauth-client-secret" => match take_value(out, &mut index) {
                Some(value) => oauth.get_or_insert_with(Default::default).client_secret = value,
                None => return 2,
            },
            "--oauth-client-secret-env" => match take_value(out, &mut index) {
                Some(value) => oauth.get_or_insert_with(Default::default).client_secret_env = value,
                None => return 2,
            },
            "--oauth-scope" => match take_value(out, &mut index) {
                Some(value) => oauth
                    .get_or_insert_with(Default::default)
                    .scopes
                    .push(value),
                None => return 2,
            },
            "--no-oauth" => {
                oauth = None;
                index += 1;
            }
            other => {
                out.failure(&format!("unknown flag '{other}' — run `ol mcp help`"));
                return 2;
            }
        }
    }
    config.oauth = oauth;

    // Required fields depend on the transport, so validate once it is known.
    if config.transport_type.eq_ignore_ascii_case("stdio") {
        if config.command.trim().is_empty() {
            out.failure(&format!(
                "usage: ol mcp {verb} {name} --type stdio --command <executable> [--arg VALUE]..."
            ));
            return 2;
        }
    } else if config.url.is_empty() {
        out.failure(&format!("usage: ol mcp {verb} {name} <url> [flags]"));
        return 2;
    }

    settings.mcp_servers.insert(name.clone(), config);
    if !omnilauncher_lib::save_settings(&settings) {
        out.failure("failed to write settings");
        return 1;
    }
    let past_tense = if update { "updated" } else { "added" };
    out.success(&format!("MCP server '{name}' {past_tense}"));
    0
}

/// Delete one entry from `settings.mcp_servers`.
fn mcp_remove(out: &Output, name: Option<&String>) -> i32 {
    let Some(name) = name else {
        out.failure("usage: ol mcp remove <server-name>");
        return 2;
    };
    let mut settings = omnilauncher_lib::load_settings();
    if settings.mcp_servers.remove(name).is_none() {
        out.failure(&format!("MCP server '{name}' is not configured"));
        return 1;
    }
    if !omnilauncher_lib::save_settings(&settings) {
        out.failure("failed to write settings");
        return 1;
    }
    out.success(&format!("MCP server '{name}' removed"));
    0
}

/// Print one server's config with secrets masked.
fn mcp_show(out: &Output, name: Option<&String>) -> i32 {
    let Some(name) = name else {
        out.failure("usage: ol mcp show <server-name>");
        return 2;
    };
    let settings = omnilauncher_lib::load_settings();
    let Some(config) = settings.mcp_servers.get(name) else {
        out.failure(&format!("MCP server '{name}' is not configured"));
        return 1;
    };
    match serde_json::to_string_pretty(&config.masked_for_display()) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(error) => {
            out.failure(&format!("failed to render config: {error}"));
            1
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
        "login" | "auth" => providers_login(out, args.get(1).map(String::as_str)),
        "logout" | "signout" => providers_logout(out, args.get(1).map(String::as_str)),
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
    let name =
        arg_value(args, "--name").or_else(|| args.first().filter(|a| !a.starts_with('-')).cloned());
    // Interactive mode: explicitly requested with -i/--interactive, or implied
    // when no name is given on an interactive terminal (and not --json).
    let wants_interactive = args.iter().any(|a| a == "-i" || a == "--interactive");
    if (wants_interactive || name.is_none()) && !out.json && std::io::stdin().is_terminal() {
        return providers_add_interactive(out, name);
    }
    let Some(name) = name else {
        out.failure("usage: ol providers add <name> --kind <custom|github-copilot|azure-foundry> [--base-url URL] [--api-key KEY] [--model MODEL] [--models a,b,c]\n       ol providers add -i    (interactive)");
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

/// Interactively prompt for the fields needed to add a provider, respecting
/// each kind's capabilities (only ask for the fields that apply). Returns a
/// process exit code. Used when `ol providers add` is run on a TTY without
/// enough flags, or with `-i` / `--interactive`.
fn providers_add_interactive(out: &Output, prefilled_name: Option<String>) -> i32 {
    out.info(&out.cyan("Add a provider (press Ctrl-C to cancel)"));

    // Name
    let name = match prefilled_name {
        Some(n) => n,
        None => match prompt_line(out, "Name", None) {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => {
                out.failure("name is required");
                return 2;
            }
        },
    };

    // Kind
    let kind = loop {
        let raw = prompt_line(
            out,
            "Kind [custom | github-copilot | azure-foundry]",
            Some("custom"),
        )
        .unwrap_or_default();
        match raw.parse::<ProviderKind>() {
            Ok(kind) => break kind,
            Err(e) => out.failure(&e),
        }
    };
    let caps = provider_caps(kind);

    let mut settings = omnilauncher_lib::load_settings();

    // Id (default derived from name)
    let default_id = provider_id_from_name(&name, &settings);
    let id = loop {
        let candidate = prompt_line(out, "Id", Some(&default_id))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_id.clone());
        if settings.providers.iter().any(|p| p.id == candidate) {
            out.failure(&format!("provider id '{candidate}' already exists"));
        } else {
            break candidate;
        }
    };

    // Base URL (all kinds use it except github-copilot, which manages its own).
    let base_url = if caps.uses_copilot_auth {
        String::new()
    } else {
        prompt_line(out, "Base URL", None)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    // API key (hidden input) only when the kind requires it.
    let api_key = if caps.requires_api_key {
        prompt_secret(out, "API key").unwrap_or_default()
    } else {
        String::new()
    };

    // Models: manual list for kinds without auto-listing; single model otherwise.
    let models: Vec<String> = if caps.manual_models {
        prompt_line(out, "Models (comma-separated)", None)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect()
    } else {
        Vec::new()
    };
    let default_model = models
        .first()
        .cloned()
        .unwrap_or_else(|| "auto".to_string());
    let model = prompt_line(out, "Model", Some(&default_model))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(default_model);

    // Make active?
    let make_active = settings.active_provider_id.is_empty()
        || prompt_yes_no(out, "Make this the active provider?", false);

    let provider = Provider {
        id: id.clone(),
        name,
        kind,
        base_url,
        api_key,
        model,
        models,
        ..Provider::default()
    };
    settings.providers.push(provider);
    if make_active {
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

/// Prompt for a single line of input, showing an optional default that is
/// returned when the user just presses Enter. Returns `None` on EOF.
fn prompt_line(out: &Output, label: &str, default: Option<&str>) -> Option<String> {
    use std::io::Write;
    let suffix = match default {
        Some(d) if !d.is_empty() => format!(" [{d}]"),
        _ => String::new(),
    };
    print!("{}{}: ", out.cyan(label), out.dim(&suffix));
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => None, // EOF
        Ok(_) => {
            let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
            if trimmed.is_empty() {
                default.map(ToString::to_string).or(Some(String::new()))
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

/// Prompt for a secret without echoing it to the terminal when possible.
/// Falls back to plain input if the terminal cannot be put in raw mode.
fn prompt_secret(out: &Output, label: &str) -> Option<String> {
    use std::io::Write;
    print!("{}: ", out.cyan(label));
    let _ = std::io::stdout().flush();
    match read_hidden_line() {
        Some(secret) => {
            println!(); // move past the (silent) input line
            Some(secret)
        }
        None => {
            // Could not disable echo; fall back to visible input.
            let mut line = String::new();
            match std::io::stdin().read_line(&mut line) {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(line.trim_end_matches(['\n', '\r']).to_string()),
            }
        }
    }
}

/// Prompt for a yes/no answer with a default. Returns the default on empty
/// input or EOF.
fn prompt_yes_no(out: &Output, label: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    match prompt_line(out, &format!("{label} [{hint}]"), None) {
        Some(ans) => match ans.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => true,
            "n" | "no" => false,
            _ => default_yes,
        },
        None => default_yes,
    }
}

/// Read a line from stdin with terminal echo disabled (Unix). On non-Unix or
/// when the terminal cannot be reconfigured, returns `None` so the caller can
/// fall back to visible input.
#[cfg(unix)]
fn read_hidden_line() -> Option<String> {
    use std::io::BufRead;
    use std::os::unix::io::AsRawFd;

    let fd = std::io::stdin().as_raw_fd();
    // SAFETY: termios is a POD struct; we zero it then let tcgetattr fill it.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut term) } != 0 {
        return None;
    }
    let original = term;
    term.c_lflag &= !libc::ECHO;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) } != 0 {
        return None;
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    // Always restore the original terminal attributes.
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &original) };
    match read {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim_end_matches(['\n', '\r']).to_string()),
    }
}

#[cfg(not(unix))]
fn read_hidden_line() -> Option<String> {
    None
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
        notify_backend_reload();
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
        notify_backend_reload();
        out.success(&format!("provider {id} model: {model}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

/// Best-effort: tell a running backend to reload `settings.json` so a
/// provider/model switch takes effect immediately instead of only after a
/// restart. The CLI persists settings to disk, but the live app holds its
/// provider in memory; without this nudge it keeps routing to the previously
/// active provider (yielding 401s against the stale endpoint).
///
/// This is intentionally silent about failures: if the backend is not running
/// (the common case for a pure CLI user), there is nothing to reload and the
/// disk write alone is sufficient — the next launch picks up the change.
fn notify_backend_reload() {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let settings = omnilauncher_lib::load_settings();
    let token = omnilauncher_lib::settings::resolve_backend_auth_token(&settings);
    let port = crate::cli::ops::server_port();

    let addr = match (std::net::Ipv4Addr::LOCALHOST, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut it| it.next())
    {
        Some(a) => a,
        None => return,
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(400)) else {
        return; // backend not running — disk write is enough.
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    let req = format!(
        "POST /api/settings/reload HTTP/1.0\r\n\
         Host: 127.0.0.1:{port}\r\n\
         X-OmniLauncher-Token: {token}\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\r\n"
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    // Response is ignored deliberately: a 200 means the live app switched, any
    // other outcome still leaves settings.json correct on disk.
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

struct CopilotLoginOutcome {
    github_token: String,
    copilot_token: String,
    copilot_expiry: i64,
    name: String,
    github_refresh_token: String,
    github_expiry: i64,
    github_refresh_expiry: i64,
}

fn providers_login(out: &Output, id: Option<&str>) -> i32 {
    use omnilauncher_lib::ai::copilot_auth;

    let mut settings = omnilauncher_lib::load_settings();

    // Resolve the target provider: explicit id, else the active provider.
    let target_id = match id {
        Some(id) => id.to_string(),
        None => settings.active_provider().id,
    };
    let Some(idx) = settings.providers.iter().position(|p| p.id == target_id) else {
        out.failure(&format!("provider '{target_id}' not found"));
        return 1;
    };
    if settings.providers[idx].kind != ProviderKind::GithubCopilot {
        out.failure(&format!(
            "provider '{target_id}' is kind '{}'; login only applies to github-copilot providers",
            settings.providers[idx].kind
        ));
        return 2;
    }

    // The CLI dispatch path is synchronous; build a dedicated runtime for the
    // async OAuth flow.
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            out.failure(&format!("failed to start async runtime: {e}"));
            return 1;
        }
    };

    let enterprise = settings.providers[idx].copilot_enterprise_url.clone();

    let outcome: Result<CopilotLoginOutcome, String> = rt.block_on(async {
        let device = copilot_auth::get_device_code().await?;

        out.info(&format!(
            "Visit {} and enter code: {}",
            out.cyan(&device.verification_uri),
            out.cyan(&device.user_code)
        ));
        // Best-effort: open the browser for the user. Ignore failures (headless).
        let _ =
            omnilauncher_lib::plugins::url_opener::open_url_in_browser(&device.verification_uri);
        out.info("Waiting for authorization...");

        let token = copilot_auth::poll_access_token(&device).await?;
        let github_token = token.access_token;
        // When the GitHub App issues expiring user tokens, capture the refresh
        // token + absolute expiry so the outer token can be rotated later
        // without another device-code login. Empty/zero when non-expiring.
        let github_refresh_token = token.refresh_token;
        let github_expiry = if token.expires_in > 0 {
            copilot_auth::now_unix() + token.expires_in
        } else {
            0
        };
        let github_refresh_expiry = if token.refresh_token_expires_in > 0 {
            copilot_auth::now_unix() + token.refresh_token_expires_in
        } else {
            0
        };

        // Fetch a friendly display name (best effort).
        let name = match copilot_auth::get_user(&github_token).await {
            Ok(user) => copilot_auth::copilot_provider_name(&user),
            Err(e) => {
                log::warn!("copilot login: failed to fetch user info: {e}");
                String::new()
            }
        };

        // Exchange for the initial short-lived Copilot token.
        let copilot = copilot_auth::get_copilot_token(&github_token, &enterprise).await?;
        Ok(CopilotLoginOutcome {
            github_token,
            copilot_token: copilot.token,
            copilot_expiry: copilot.expires_at,
            name,
            github_refresh_token,
            github_expiry,
            github_refresh_expiry,
        })
    });

    let outcome = match outcome {
        Ok(v) => v,
        Err(e) => {
            out.failure(&format!("login failed: {e}"));
            return 1;
        }
    };

    let provider = &mut settings.providers[idx];
    provider.copilot_github_token = outcome.github_token;
    provider.copilot_github_refresh_token = outcome.github_refresh_token;
    provider.copilot_github_token_expiry = outcome.github_expiry;
    provider.copilot_github_refresh_token_expiry = outcome.github_refresh_expiry;
    provider.copilot_token = outcome.copilot_token;
    provider.copilot_token_expiry = outcome.copilot_expiry;
    if !outcome.name.is_empty() {
        provider.name = outcome.name.clone();
    }

    settings.sync_legacy_ai_fields_from_active_provider();
    if omnilauncher_lib::save_settings(&settings) {
        let who = if outcome.name.is_empty() {
            target_id
        } else {
            outcome.name
        };
        out.success(&format!("logged in to GitHub Copilot as {who}"));
        0
    } else {
        out.failure("failed to save settings");
        1
    }
}

/// Wipe any stored Copilot/GitHub OAuth tokens for a provider (or all github-copilot
/// providers when `id` is `None`). Legacy top-level `ai_api_key` is also cleared so
/// stale credentials never leak back into a fresh OAuth. The device-code flow can
/// then be re-run from scratch via `ol providers login`.
fn providers_logout(out: &Output, id: Option<&str>) -> i32 {
    let mut settings = omnilauncher_lib::load_settings();

    let mut cleared: Vec<String> = Vec::new();
    for p in settings.providers.iter_mut() {
        let target = match id {
            Some(want) => p.id == want,
            None => p.kind == ProviderKind::GithubCopilot,
        };
        if !target {
            continue;
        }
        let had_any = !p.copilot_github_token.is_empty()
            || !p.copilot_github_refresh_token.is_empty()
            || !p.copilot_token.is_empty()
            || p.copilot_github_token_expiry != 0
            || p.copilot_github_refresh_token_expiry != 0
            || p.copilot_token_expiry != 0;
        p.copilot_github_token.clear();
        p.copilot_github_refresh_token.clear();
        p.copilot_github_token_expiry = 0;
        p.copilot_github_refresh_token_expiry = 0;
        p.copilot_token.clear();
        p.copilot_token_expiry = 0;
        if had_any {
            cleared.push(p.id.clone());
        } else {
            // Still record it so the user gets confirmation the slot is empty.
            cleared.push(format!("{} (already empty)", p.id));
        }
    }

    if cleared.is_empty() {
        if let Some(want) = id {
            out.failure(&format!("provider '{want}' not found"));
        } else {
            out.info("no github-copilot providers configured");
        }
        return 1;
    }

    // Legacy leak-guard: the top-level ai_api_key was historically reused as a
    // bearer for Copilot before per-provider fields existed. Blank it so the
    // next OAuth truly starts from scratch.
    settings.ai_api_key.clear();

    settings.sync_legacy_ai_fields_from_active_provider();
    if !omnilauncher_lib::save_settings(&settings) {
        out.failure("failed to save settings");
        return 1;
    }

    out.success(&format!(
        "cleared Copilot tokens for: {}",
        cleared.join(", ")
    ));
    out.info("re-authenticate with: ol providers login [id]");
    0
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
    let settings = omnilauncher_lib::load_settings().masked_for_display();
    print_json(out, &settings)
}

fn settings_get(out: &Output, field: &str) -> i32 {
    let settings = omnilauncher_lib::load_settings().masked_for_display();
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
    println!("  add -i | --interactive                    add a provider via interactive prompts");
    println!("  set-active <id>                           switch active provider");
    println!("  set-model <model> [provider-id]           update selected model");
    println!("  remove <id>                               remove provider");
    println!("  login [id]                                GitHub Copilot device-code OAuth login");
    println!("  logout [id]                               clear stored Copilot tokens (all copilot providers if id omitted)");
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
