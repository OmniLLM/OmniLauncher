import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface PluginInfo {
  name: string;
  description: string;
  version: string;
  keyword?: string;
  icon?: string;
  entry: string;
  dir_name: string;
  is_git_repo?: boolean;
  git_remote?: string;
  git_branch?: string;
  git_clean?: boolean;
  git_ahead?: number;
  git_behind?: number;
}

interface PluginRepo {
  dir_name: string;
  collection_name?: string;
  collection_key?: string;
  collection_source?: string;
  plugins: PluginInfo[];
  is_git_repo?: boolean;
  git_remote?: string;
  git_branch?: string;
  git_clean?: boolean;
  git_ahead?: number;
  git_behind?: number;
}

interface GroupedPlugin extends PluginInfo {
  repo_dir_name: string;
  repo_is_git_repo?: boolean;
  repo_git_remote?: string;
}

interface PluginCollection {
  key: string;
  name: string;
  repos: PluginRepo[];
  plugins: GroupedPlugin[];
  hasGitRepo: boolean;
  collectionSource?: string;
}

function normalizeGitRemote(remote?: string): string | undefined {
  if (!remote) return undefined;

  const trimmed = remote.trim().replace(/\.git$/i, "");
  const sshMatch = trimmed.match(/^[^@]+@[^:]+:(.+)$/);
  if (sshMatch) {
    const segments = sshMatch[1].split("/").filter(Boolean);
    if (segments.length >= 2) {
      return `${segments[segments.length - 2]}/${segments[segments.length - 1]}`;
    }
  }

  try {
    const url = new URL(trimmed);
    const segments = url.pathname.split("/").filter(Boolean);
    if (segments.length >= 2) {
      return `${segments[segments.length - 2]}/${segments[segments.length - 1]}`;
    }
  } catch {
    const segments = trimmed.split(/[\\/]/).filter(Boolean);
    if (segments.length >= 2) {
      return `${segments[segments.length - 2]}/${segments[segments.length - 1]}`;
    }
  }

  return undefined;
}

function collectionDisplayName(repo: PluginRepo): string {
  return normalizeGitRemote(repo.git_remote) || repo.collection_name || repo.dir_name;
}

function collectionGroupKey(repo: PluginRepo): string {
  const remoteName = normalizeGitRemote(repo.git_remote);
  if (remoteName) {
    return `remote:${remoteName}`;
  }
  return repo.collection_key || repo.collection_name || repo.dir_name;
}

interface AppSettings {
  plugin_dirs: string[];
  [key: string]: unknown;
}

interface RuntimeDependency {
  id: string;
  label: string;
  installed: boolean;
  installable: boolean;
  install_command?: string | null;
  detail: string;
}

interface RuntimeProgressEvent {
  id: string;
  label: string;
  message: string;
}

interface PluginManagerProps {
  colors: {
    bg: string;
    surface: string;
    surface2: string;
    text: string;
    accent: string;
    accentDim: string;
    sub: string;
  };
  onClose: () => void;
}

const DEFAULT_DIR = "~/.omnilauncher/plugins (default)";

export default function PluginManager({ colors, onClose }: PluginManagerProps) {
  const [repos, setRepos] = useState<PluginRepo[]>([]);
  const [expandedCollections, setExpandedCollections] = useState<Record<string, boolean>>({});
  const [source, setSource] = useState("");
  const [targetDir, setTargetDir] = useState<string>(""); // "" = default
  const [extraDirs, setExtraDirs] = useState<string[]>([]);
  const [runtimeDeps, setRuntimeDeps] = useState<RuntimeDependency[]>([]);
  const [runtimeInstalling, setRuntimeInstalling] = useState<string | null>(null);
  const [runtimeProgress, setRuntimeProgress] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<{
    type: "idle" | "loading" | "success" | "error";
    message: string;
  }>({ type: "idle", message: "" });

  const refresh = useCallback(() => {
    invoke<PluginRepo[]>("list_plugins")
      .then((list) => setRepos(list))
      .catch(() => setRepos([]));
  }, []);

  const refreshRuntimeDeps = useCallback(() => {
    invoke<RuntimeDependency[]>("list_plugin_runtime_dependencies")
      .then((deps) => setRuntimeDeps(deps))
      .catch(() => setRuntimeDeps([]));
  }, []);

  const collections = useMemo<PluginCollection[]>(() => {
    const grouped = new Map<string, PluginCollection>();

    for (const repo of repos) {
      const key = collectionGroupKey(repo);
      const name = collectionDisplayName(repo);

      if (!grouped.has(key)) {
        grouped.set(key, {
          key,
          name,
          repos: [],
          plugins: [],
          hasGitRepo: false,
          collectionSource: repo.collection_source,
        });
      }

      const collection = grouped.get(key)!;
      collection.collectionSource = collection.collectionSource || repo.collection_source;
      collection.repos.push(repo);
      collection.hasGitRepo = collection.hasGitRepo || !!repo.is_git_repo;

      for (const plugin of repo.plugins) {
        collection.plugins.push({
          ...plugin,
          repo_dir_name: repo.dir_name,
          repo_is_git_repo: repo.is_git_repo,
          repo_git_remote: repo.git_remote,
        });
      }
    }

    return Array.from(grouped.values());
  }, [repos]);

  useEffect(() => {
    setExpandedCollections((current) => {
      const next = { ...current };
      for (const collection of collections) {
        if (next[collection.key] === undefined) {
          next[collection.key] = false;
        }
      }
      return next;
    });
  }, [collections]);

  useEffect(() => {
    refresh();
    refreshRuntimeDeps();
    // Load extra plugin_dirs from settings
    invoke<AppSettings>("get_settings")
      .then((s) => setExtraDirs(s.plugin_dirs ?? []))
      .catch(() => {});
  }, [refresh, refreshRuntimeDeps]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listen<RuntimeProgressEvent>("omnilauncher://plugin-runtime-progress", (event) => {
      const { id, label, message } = event.payload;
      setRuntimeProgress((current) => ({ ...current, [id]: message }));
      setStatus({ type: "loading", message: `${label}: ${message}` });
    }).then((dispose) => {
      if (disposed) {
        dispose();
      } else {
        unlisten = dispose;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const handleInstall = async () => {
    const trimmed = source.trim();
    if (!trimmed) return;
    setStatus({ type: "loading", message: "Installing…" });
    try {
      const message = await invoke<string>("install_plugin", {
        source: trimmed,
        targetDir: targetDir || null,
      });
      setStatus({ type: "success", message: `✓ ${message}` });
      setSource("");
      refresh();
      refreshRuntimeDeps();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleInstallRuntime = async (dep: RuntimeDependency) => {
    setRuntimeInstalling(dep.id);
    setRuntimeProgress((current) => ({ ...current, [dep.id]: "Starting…" }));
    setStatus({ type: "loading", message: `Installing ${dep.label}…` });
    try {
      const message = await invoke<string>("install_plugin_runtime_dependency", { id: dep.id });
      setStatus({ type: "success", message: `✓ ${message}` });
      setRuntimeProgress((current) => {
        const next = { ...current };
        delete next[dep.id];
        return next;
      });
      refreshRuntimeDeps();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    } finally {
      setRuntimeInstalling(null);
    }
  };

  const handleUpdateRepo = async (dirName: string) => {
    setStatus({ type: "loading", message: `Updating repo "${dirName}"…` });
    try {
      const message = await invoke<string>("update_plugin", { name: dirName });
      setStatus({ type: "success", message: `✓ ${message}` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleUpdateCollection = async (collection: PluginCollection) => {
    const updatableRepos = collection.repos.filter((repo) => repo.is_git_repo);
    if (updatableRepos.length === 0 && !collection.collectionSource) {
      setStatus({
        type: "error",
        message: `✗ Collection "${collection.name}" has no git repositories to update.`,
      });
      return;
    }

    if (updatableRepos.length === 0 && collection.collectionSource) {
      const pluginDirs = collection.plugins.map((plugin) => plugin.repo_dir_name);
      setStatus({
        type: "loading",
        message: `Updating collection "${collection.name}"…`,
      });
      try {
        const message = await invoke<string>("update_plugin_collection", {
          source: collection.collectionSource,
          pluginDirs,
        });
        setStatus({ type: "success", message: `✓ ${message}` });
        refresh();
      } catch (e) {
        setStatus({ type: "error", message: `✗ ${e}` });
      }
      return;
    }

    if (updatableRepos.length === 1) {
      await handleUpdateRepo(updatableRepos[0].dir_name);
      return;
    }

    setStatus({
      type: "loading",
      message: `Updating collection "${collection.name}" (${updatableRepos.length} repos)…`,
    });

    const updated: string[] = [];
    const failed: string[] = [];

    for (const repo of updatableRepos) {
      try {
        await invoke<string>("update_plugin", { name: repo.dir_name });
        updated.push(repo.dir_name);
      } catch {
        failed.push(repo.dir_name);
      }
    }

    if (failed.length > 0) {
      setStatus({
        type: "error",
        message: `✗ Collection "${collection.name}": updated ${updated.length}, failed ${failed.length} (${failed.join(", ")})`,
      });
    } else {
      setStatus({
        type: "success",
        message: `✓ Collection "${collection.name}" updated (${updated.length} repos).`,
      });
    }

    refresh();
  };

  const handleRemoveCollection = async (collection: PluginCollection) => {
    setStatus({
      type: "loading",
      message: `Removing collection "${collection.name}"…`,
    });

    const removed: string[] = [];
    const failed: string[] = [];
    for (const repo of collection.repos) {
      try {
        await invoke("remove_plugin", { name: repo.dir_name });
        removed.push(repo.dir_name);
      } catch {
        failed.push(repo.dir_name);
      }
    }

    if (failed.length > 0) {
      setStatus({
        type: "error",
        message: `✗ Collection "${collection.name}": removed ${removed.length}, failed ${failed.length} (${failed.join(", ")})`,
      });
    } else {
      setStatus({
        type: "success",
        message: `✓ Removed collection "${collection.name}"`,
      });
    }

    refresh();
  };

  const toggleCollection = (collectionKey: string) => {
    setExpandedCollections((current) => ({
      ...current,
      [collectionKey]: !current[collectionKey],
    }));
  };

  const statusColor =
    status.type === "success"
      ? "#a6e3a1"
      : status.type === "error"
        ? "#f38ba8"
        : colors.sub;

  const selectStyle: React.CSSProperties = {
    background: colors.surface,
    border: `1px solid ${colors.surface2}`,
    borderRadius: "8px",
    padding: "7px 10px",
    color: extraDirs.length === 0 ? colors.sub : colors.text,
    fontSize: "12px",
    outline: "none",
    cursor: "pointer",
    flexShrink: 0,
    maxWidth: "180px",
  };

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        padding: "14px 16px 10px",
        gap: "12px",
        overflowY: "auto",
        scrollbarWidth: "thin",
        scrollbarColor: `${colors.surface2} transparent`,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span
          style={{
            fontSize: "13px",
            fontWeight: 700,
            color: colors.accent,
            letterSpacing: "0.04em",
            display: "flex",
            alignItems: "center",
            gap: "6px",
          }}
        >
          🔌 Plugin Manager
        </span>
        <button
          onClick={onClose}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            color: colors.sub,
            fontSize: "16px",
            lineHeight: 1,
            padding: "2px 4px",
          }}
          title="Close"
        >
          ×
        </button>
      </div>

      {/* Install row */}
      <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
        <input
          type="text"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="Git URL or local path…"
          style={{
            flex: 1,
            minWidth: "180px",
            background: colors.surface,
            border: `1px solid ${colors.surface2}`,
            borderRadius: "8px",
            padding: "7px 12px",
            color: colors.text,
            fontSize: "13px",
            outline: "none",
          }}
        />
        {/* Install-to selector — only shown when there are extra dirs */}
        {extraDirs.length > 0 && (
          <select
            value={targetDir}
            onChange={(e) => setTargetDir(e.target.value)}
            style={selectStyle}
            title="Install into…"
          >
            <option value="">{DEFAULT_DIR}</option>
            {extraDirs.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        )}
        <button
          onClick={handleInstall}
          disabled={status.type === "loading" || !source.trim()}
          style={{
            background: colors.accent,
            border: "none",
            borderRadius: "8px",
            padding: "7px 14px",
            color: "#FFFFFF",
            fontSize: "13px",
            fontWeight: 600,
            cursor: source.trim() ? "pointer" : "default",
            opacity: source.trim() ? 1 : 0.5,
            transition: "opacity 150ms",
          }}
        >
          Install
        </button>
      </div>

      {/* Status message */}
      {status.type !== "idle" && (
        <div
          style={{
            fontSize: "12px",
            color: statusColor,
            minHeight: "18px",
          }}
        >
          {status.message}
        </div>
      )}

      {runtimeDeps.length > 0 && (
        <div
          style={{
            display: "grid",
            gap: "6px",
            background: colors.surface,
            border: `1px solid ${colors.surface2}`,
            borderRadius: "8px",
            padding: "9px 10px",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: "8px",
            }}
          >
            <span
              style={{
                fontSize: "11px",
                color: colors.sub,
                fontWeight: 700,
                letterSpacing: "0.04em",
                textTransform: "uppercase",
              }}
            >
              Runtimes
            </span>
            <button
              type="button"
              onClick={refreshRuntimeDeps}
              disabled={status.type === "loading"}
              style={{
                background: "none",
                border: `1px solid ${colors.surface2}`,
                borderRadius: "6px",
                padding: "3px 8px",
                color: colors.sub,
                fontSize: "11px",
                cursor: status.type === "loading" ? "default" : "pointer",
              }}
              title="Refresh runtime checks"
            >
              Refresh
            </button>
          </div>
          <div style={{ display: "grid", gap: "5px" }}>
            {runtimeDeps.map((dep) => {
              const busy = runtimeInstalling === dep.id;
              const progress = runtimeProgress[dep.id];
              return (
                <div
                  key={dep.id}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    minWidth: 0,
                  }}
                >
                  <span
                    style={{
                      fontSize: "11px",
                      color: dep.installed ? "#a6e3a1" : "#f9e2af",
                      border: `1px solid ${dep.installed ? "#a6e3a155" : "#f9e2af66"}`,
                      borderRadius: "999px",
                      padding: "1px 7px",
                      flexShrink: 0,
                    }}
                  >
                    {dep.installed ? "READY" : "MISSING"}
                  </span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: "12px", color: colors.text, fontWeight: 600 }}>
                      {dep.label}
                    </div>
                    <div
                      style={{
                        fontSize: "11px",
                        color: colors.sub,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                      title={dep.install_command || dep.detail}
                    >
                      {progress || (dep.installed ? dep.detail : dep.install_command || dep.detail)}
                    </div>
                  </div>
                  {!dep.installed && (
                    <button
                      type="button"
                      onClick={() => handleInstallRuntime(dep)}
                      disabled={status.type === "loading" || busy}
                      style={{
                        background: dep.installable ? colors.accent : "none",
                        border: `1px solid ${dep.installable ? colors.accent : colors.surface2}`,
                        borderRadius: "7px",
                        padding: "5px 9px",
                        color: dep.installable ? "#FFFFFF" : colors.sub,
                        fontSize: "11px",
                        fontWeight: 700,
                        cursor: status.type === "loading" || busy ? "default" : "pointer",
                        opacity: status.type === "loading" || busy ? 0.65 : 1,
                        flexShrink: 0,
                      }}
                      title={dep.installable ? `Install ${dep.label}` : dep.install_command || dep.detail}
                    >
                      {busy ? "Installing" : dep.installable ? "Install" : "Details"}
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Repo list */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "6px",
        }}
      >
        {collections.length === 0 ? (
          <div
            style={{
              fontSize: "13px",
              color: colors.sub,
              textAlign: "center",
              padding: "18px 0",
            }}
          >
            No external plugin repos installed yet.
            <br />
            <span style={{ fontSize: "12px", opacity: 0.7 }}>
              Paste a Git URL or local path above to install one.
            </span>
          </div>
        ) : (
          collections.map((collection) => {
            const expanded = expandedCollections[collection.key] ?? false;
            const pluginCount = collection.plugins.length;
            const repoCount = collection.repos.length;
            return (
              <div
                key={collection.key}
                style={{
                  background: colors.surface,
                  borderRadius: "10px",
                  border: `1px solid ${colors.surface2}`,
                  overflow: "hidden",
                }}
              >
                <div
                  onClick={() => toggleCollection(collection.key)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "10px",
                    padding: "10px 12px",
                    cursor: "pointer",
                  }}
                >
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleCollection(collection.key);
                    }}
                    aria-label={expanded ? "Collapse collection" : "Expand collection"}
                    style={{
                      width: "26px",
                      height: "26px",
                      borderRadius: "6px",
                      border: `1px solid ${colors.surface2}`,
                      background: colors.bg,
                      color: colors.text,
                      cursor: "pointer",
                      flexShrink: 0,
                    }}
                    title={expanded ? "Collapse collection plugins" : "Expand collection plugins"}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>

                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div
                      style={{
                        fontSize: "13px",
                        fontWeight: 700,
                        color: colors.text,
                        display: "flex",
                        flexWrap: "wrap",
                        alignItems: "center",
                        gap: "6px",
                      }}
                    >
                      <span
                        style={{
                          fontSize: "10px",
                          fontWeight: 700,
                          letterSpacing: "0.04em",
                          color: colors.accent,
                          border: `1px solid ${colors.accent}55`,
                          borderRadius: "999px",
                          padding: "1px 6px",
                        }}
                      >
                        COLLECTION
                      </span>
                      <span>{collection.name}</span>
                      <span
                        style={{
                          fontSize: "11px",
                          color: colors.sub,
                          fontWeight: 500,
                          border: `1px solid ${colors.surface2}`,
                          borderRadius: "999px",
                          padding: "1px 7px",
                        }}
                      >
                        {pluginCount} plugin{pluginCount === 1 ? "" : "s"}
                      </span>
                      {repoCount > 1 && (
                        <span
                          style={{
                            fontSize: "11px",
                            color: colors.sub,
                            fontWeight: 500,
                            border: `1px solid ${colors.surface2}`,
                            borderRadius: "999px",
                            padding: "1px 7px",
                          }}
                        >
                          {repoCount} repos
                        </span>
                      )}
                      {collection.hasGitRepo && (
                        <span
                          style={{
                            fontSize: "11px",
                            background: `${colors.accent}22`,
                            border: `1px solid ${colors.accent}44`,
                            borderRadius: "999px",
                            padding: "1px 7px",
                            color: colors.accent,
                          }}
                        >
                          git
                        </span>
                      )}
                    </div>
                    <div
                      style={{
                        fontSize: "12px",
                        color: colors.sub,
                        marginTop: "2px",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {normalizeGitRemote(collection.repos[0]?.git_remote)
                        ? collection.repos[0]!.git_remote
                        : collection.repos.length === 1
                          ? "Local plugin collection"
                          : `Contains ${collection.repos.length} plugin repositories`}
                    </div>
                  </div>

                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleUpdateCollection(collection);
                    }}
                    disabled={status.type === "loading" || (!collection.hasGitRepo && !collection.collectionSource)}
                    style={{
                      background: "none",
                      border: `1px solid ${colors.surface2}`,
                      borderRadius: "8px",
                      padding: "6px 12px",
                      color: collection.hasGitRepo || collection.collectionSource ? colors.text : colors.sub,
                      fontSize: "12px",
                      fontWeight: 600,
                      cursor: collection.hasGitRepo || collection.collectionSource ? "pointer" : "default",
                      opacity: collection.hasGitRepo || collection.collectionSource ? 1 : 0.5,
                      transition: "opacity 150ms, border-color 150ms, color 150ms",
                    }}
                    title={
                      collection.hasGitRepo || collection.collectionSource
                        ? "Update this collection and all of its plugins"
                        : "This collection has no git repositories"
                    }
                  >
                    Update
                  </button>

                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleRemoveCollection(collection);
                    }}
                    title="Remove collection"
                    style={{
                      background: "none",
                      border: `1px solid ${colors.surface2}`,
                      borderRadius: "6px",
                      padding: "4px 10px",
                      color: colors.sub,
                      fontSize: "12px",
                      cursor: "pointer",
                      flexShrink: 0,
                      transition: "color 150ms, border-color 150ms",
                    }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.color = "#f38ba8";
                      e.currentTarget.style.borderColor = "#f38ba8";
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.color = colors.sub;
                      e.currentTarget.style.borderColor = colors.surface2;
                    }}
                  >
                    Remove
                  </button>
                </div>

                {expanded && (
                  <div
                    style={{
                      display: "grid",
                      gap: "6px",
                      padding: "0 12px 12px 48px",
                      borderLeft: `1px solid ${colors.surface2}`,
                      marginLeft: "24px",
                    }}
                  >
                    <div
                      style={{
                        fontSize: "11px",
                        color: colors.sub,
                        fontWeight: 600,
                        letterSpacing: "0.03em",
                        textTransform: "uppercase",
                        padding: "2px 0",
                      }}
                    >
                      Plugins in this collection
                    </div>
                    {collection.plugins.map((plugin) => (
                      <div
                        key={`${plugin.repo_dir_name}:${plugin.name}`}
                        style={{
                          background: colors.bg,
                          border: `1px solid ${colors.surface2}`,
                          borderRadius: "8px",
                          padding: "8px 10px",
                        }}
                      >
                        <div
                          style={{
                            display: "flex",
                            alignItems: "center",
                            gap: "10px",
                          }}
                        >
                          <div style={{ flex: 1, minWidth: 0 }}>
                            <div
                              style={{
                                fontSize: "12px",
                                fontWeight: 600,
                                color: colors.text,
                                display: "flex",
                                alignItems: "center",
                                gap: "6px",
                                flexWrap: "wrap",
                              }}
                            >
                              <span
                                style={{
                                  fontSize: "10px",
                                  fontWeight: 700,
                                  letterSpacing: "0.04em",
                                  color: colors.sub,
                                  border: `1px solid ${colors.surface2}`,
                                  borderRadius: "999px",
                                  padding: "1px 6px",
                                }}
                              >
                                PLUGIN
                              </span>
                              <span>{plugin.icon ?? "🔌"}</span>
                              <span>{plugin.name}</span>
                              <span style={{ color: colors.sub, fontWeight: 400 }}>
                                v{plugin.version}
                              </span>
                              {plugin.keyword && (
                                <span
                                  style={{
                                    fontSize: "10px",
                                    background: `${colors.accent}22`,
                                    border: `1px solid ${colors.accent}44`,
                                    borderRadius: "999px",
                                    padding: "1px 6px",
                                    color: colors.accent,
                                  }}
                                >
                                  {plugin.keyword}
                                </span>
                              )}
                            </div>
                            <div
                              style={{
                                fontSize: "12px",
                                color: colors.sub,
                                marginTop: "2px",
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                                whiteSpace: "nowrap",
                              }}
                            >
                              {plugin.description}
                            </div>
                          </div>

                          <button
                            onClick={() => handleUpdateRepo(plugin.repo_dir_name)}
                            disabled={status.type === "loading" || !plugin.repo_is_git_repo}
                            style={{
                              background: "none",
                              border: `1px solid ${colors.surface2}`,
                              borderRadius: "8px",
                              padding: "5px 10px",
                              color: plugin.repo_is_git_repo ? colors.text : colors.sub,
                              fontSize: "11px",
                              fontWeight: 600,
                              cursor: plugin.repo_is_git_repo ? "pointer" : "default",
                              opacity: plugin.repo_is_git_repo ? 1 : 0.5,
                              transition: "opacity 150ms, border-color 150ms, color 150ms",
                              flexShrink: 0,
                            }}
                            title={
                              plugin.repo_is_git_repo
                                ? "Update this plugin"
                                : "This plugin repo is not a git repository"
                            }
                          >
                            Update
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
