import { useState, useEffect, useCallback } from "react";
import { invoke, listen } from "../lib/runtime";
import type { RuntimeDependency, RuntimeProgressEvent } from "../types/app";

interface PluginInfo {
  name: string;
  description: string;
  version: string;
  keyword?: string;
  icon?: string;
  entry: string;
  dir_name: string;
}

/** A plugin tagged with the repo it belongs to (from the backend grouping). */
interface GroupedPlugin extends PluginInfo {
  repo_dir_name: string;
  repo_is_git_repo?: boolean;
  repo_git_remote?: string;
}

/** Raw repo entry inside a collection (git metadata + dir_name). */
interface PluginRepo {
  dir_name: string;
  collection_name?: string;
  collection_key?: string;
  collection_source?: string;
  is_git_repo?: boolean;
  git_remote?: string;
  git_branch?: string;
  git_clean?: boolean;
  git_ahead?: number;
  git_behind?: number;
}

/**
 * A collection of plugin repos, as grouped by the backend
 * (`list_plugin_collections`). Git-remote normalization and collection keying
 * now live in Rust — the UI just renders this.
 */
interface PluginCollection {
  key: string;
  name: string;
  has_git_repo: boolean;
  collection_source?: string;
  repos: PluginRepo[];
  plugins: GroupedPlugin[];
}

/** Result of a backend collection-wide update/remove operation. */
interface CollectionOpResult {
  updated: string[];
  failed: string[];
  message: string;
}

interface AppSettings {
  plugin_dirs: string[];
  [key: string]: unknown;
}

interface PluginManagerProps {
  onClose: () => void;
}

const DEFAULT_DIR = "~/.omnilauncher/plugins (default)";

export default function PluginManager({ onClose }: PluginManagerProps) {
  const [collections, setCollections] = useState<PluginCollection[]>([]);
  const [expandedCollections, setExpandedCollections] = useState<
    Record<string, boolean>
  >({});
  const [source, setSource] = useState("");
  const [targetDir, setTargetDir] = useState<string>(""); // "" = default
  const [extraDirs, setExtraDirs] = useState<string[]>([]);
  const [runtimeDeps, setRuntimeDeps] = useState<RuntimeDependency[]>([]);
  const [runtimeInstalling, setRuntimeInstalling] = useState<string | null>(
    null,
  );
  const [runtimeProgress, setRuntimeProgress] = useState<
    Record<string, string>
  >({});
  const [status, setStatus] = useState<{
    type: "idle" | "loading" | "success" | "error";
    message: string;
  }>({ type: "idle", message: "" });

  // Collections are grouped by the backend (`list_plugin_collections`) — the UI
  // no longer reshapes raw plugin JSON.
  const refresh = useCallback(() => {
    invoke<PluginCollection[]>("list_plugin_collections")
      .then((list) => setCollections(list))
      .catch(() => setCollections([]));
  }, []);

  const refreshRuntimeDeps = useCallback(() => {
    invoke<RuntimeDependency[]>("list_plugin_runtime_dependencies")
      .then((deps) => setRuntimeDeps(deps))
      .catch(() => setRuntimeDeps([]));
  }, []);

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
    invoke<AppSettings>("get_settings")
      .then((s) => setExtraDirs(s.plugin_dirs ?? []))
      .catch(() => {});
  }, [refresh, refreshRuntimeDeps]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listen<RuntimeProgressEvent>(
      "omnilauncher://plugin-runtime-progress",
      (event) => {
        const { id, label, message } = event.payload;
        setRuntimeProgress((current) => ({ ...current, [id]: message }));
        setStatus({ type: "loading", message: `${label}: ${message}` });
      },
    ).then((dispose) => {
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
      const message = await invoke<string>(
        "install_plugin_runtime_dependency",
        { id: dep.id },
      );
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
    if (!collection.has_git_repo && !collection.collection_source) {
      setStatus({
        type: "error",
        message: `✗ Collection "${collection.name}" has no git repositories to update.`,
      });
      return;
    }
    setStatus({
      type: "loading",
      message: `Updating collection "${collection.name}"…`,
    });
    // The backend performs the per-repo update loop and partial-failure
    // aggregation, returning a single summary result.
    const gitRepoDirs = collection.repos
      .filter((repo) => repo.is_git_repo)
      .map((repo) => repo.dir_name);
    const repoDirs = collection.repos.map((repo) => repo.dir_name);
    try {
      const result = await invoke<CollectionOpResult>(
        "update_plugin_collection_all",
        {
          collectionSource: collection.collection_source ?? null,
          repoDirs,
          gitRepoDirs,
        },
      );
      setStatus({
        type: result.failed.length > 0 ? "error" : "success",
        message: `${result.failed.length > 0 ? "✗" : "✓"} Collection "${collection.name}": ${result.message}`,
      });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleRemoveCollection = async (collection: PluginCollection) => {
    setStatus({
      type: "loading",
      message: `Removing collection "${collection.name}"…`,
    });
    const repoDirs = collection.repos.map((repo) => repo.dir_name);
    try {
      const result = await invoke<CollectionOpResult>(
        "remove_plugin_collection",
        { repoDirs },
      );
      setStatus({
        type: result.failed.length > 0 ? "error" : "success",
        message: `${result.failed.length > 0 ? "✗" : "✓"} Collection "${collection.name}": ${result.message}`,
      });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const toggleCollection = (collectionKey: string) => {
    setExpandedCollections((current) => ({
      ...current,
      [collectionKey]: !current[collectionKey],
    }));
  };

  const installDisabled = status.type === "loading" || !source.trim();
  const statusClass =
    status.type === "success"
      ? "plugin-panel__status plugin-panel__status--success"
      : status.type === "error"
        ? "plugin-panel__status plugin-panel__status--error"
        : "plugin-panel__status";

  return (
    <div className="plugin-panel">
      {/* Header */}
      <div className="plugin-panel__header">
        <span className="plugin-panel__title">🔌 Plugin Manager</span>
        <button
          className="omni-titlebar__close"
          onClick={onClose}
          title="Close"
          aria-label="Close"
        >
          ×
        </button>
      </div>

      {/* Install row */}
      <div className="plugin-panel__install-row">
        <input
          type="text"
          className="omni-input"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="Git URL, owner/repo, or local path…"
        />
        {extraDirs.length > 0 && (
          <select
            value={targetDir}
            onChange={(e) => setTargetDir(e.target.value)}
            className={
              "plugin-panel__target-select" +
              (targetDir === ""
                ? " plugin-panel__target-select--placeholder"
                : "")
            }
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
          type="button"
          className="omni-btn omni-btn--primary"
          onClick={handleInstall}
          disabled={installDisabled}
          aria-disabled={installDisabled}
        >
          {status.type === "loading" ? "Installing…" : "Install"}
        </button>
      </div>

      {/* Status message */}
      {status.type !== "idle" && (
        <div className={statusClass}>{status.message}</div>
      )}

      {/* Runtimes */}
      {runtimeDeps.length > 0 && (
        <div className="plugin-runtimes">
          <div className="plugin-runtimes__head">
            <span className="plugin-runtimes__title">Runtimes</span>
            <button
              type="button"
              className="omni-btn omni-btn--ghost omni-btn--xs"
              onClick={refreshRuntimeDeps}
              disabled={status.type === "loading"}
              title="Refresh runtime checks"
            >
              Refresh
            </button>
          </div>
          <div className="plugin-runtimes__list">
            {runtimeDeps.map((dep) => {
              const busy = runtimeInstalling === dep.id;
              const progress = runtimeProgress[dep.id];
              return (
                <div key={dep.id} className="plugin-runtimes__row">
                  <span
                    className={
                      "omni-pill plugin-runtimes__pill " +
                      (dep.installed
                        ? "omni-pill--success"
                        : "omni-pill--warning")
                    }
                  >
                    {dep.installed ? "READY" : "MISSING"}
                  </span>
                  <div className="plugin-runtimes__main">
                    <div className="plugin-runtimes__label">{dep.label}</div>
                    <div
                      className="plugin-runtimes__detail"
                      title={dep.install_command || dep.detail}
                    >
                      {progress ||
                        (dep.installed
                          ? dep.detail
                          : dep.install_command || dep.detail)}
                    </div>
                  </div>
                  {!dep.installed && (
                    <button
                      type="button"
                      className={
                        "omni-btn omni-btn--xs plugin-runtimes__action" +
                        (dep.installable ? " omni-btn--primary" : "")
                      }
                      onClick={() => handleInstallRuntime(dep)}
                      disabled={status.type === "loading" || busy}
                      aria-disabled={status.type === "loading" || busy}
                      title={
                        dep.installable
                          ? `Install ${dep.label}`
                          : dep.install_command || dep.detail
                      }
                    >
                      {busy
                        ? "Installing…"
                        : dep.installable
                          ? "Install"
                          : "Details"}
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Collection list */}
      <div className="plugin-panel__list">
        {collections.length === 0 ? (
          <div className="plugin-panel__empty">
            No external plugin repos installed yet.
            <br />
            <span className="plugin-panel__empty-hint">
              Paste a Git URL, GitHub <code>owner/repo</code>, or local path
              above to install one. GitHub repos use <code>gh</code> when
              authenticated (private &amp; GHE supported).
            </span>
          </div>
        ) : (
          collections.map((collection) => {
            const expanded = expandedCollections[collection.key] ?? false;
            const pluginCount = collection.plugins.length;
            const repoCount = collection.repos.length;
            const updateDisabled =
              status.type === "loading" ||
              (!collection.has_git_repo && !collection.collection_source);
            return (
              <div key={collection.key} className="collection-card">
                <div
                  className="collection-card__head"
                  onClick={() => toggleCollection(collection.key)}
                >
                  <button
                    type="button"
                    className={
                      "omni-btn omni-btn--ghost omni-btn--xs collection-card__expand" +
                      (expanded ? " is-active" : "")
                    }
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleCollection(collection.key);
                    }}
                    aria-label={
                      expanded ? "Collapse collection" : "Expand collection"
                    }
                    aria-expanded={expanded}
                    title={
                      expanded
                        ? "Collapse collection plugins"
                        : "Expand collection plugins"
                    }
                  >
                    {expanded ? "▾" : "▸"}
                  </button>

                  <div className="collection-card__main">
                    <div className="collection-card__title-row">
                      <span className="collection-card__kind">COLLECTION</span>
                      <span>{collection.name}</span>
                      <span className="collection-card__count">
                        {pluginCount} plugin{pluginCount === 1 ? "" : "s"}
                      </span>
                      {repoCount > 1 && (
                        <span className="collection-card__count">
                          {repoCount} repos
                        </span>
                      )}
                      {collection.has_git_repo && (
                        <span className="collection-card__git-badge">git</span>
                      )}
                    </div>
                    <div className="collection-card__subtitle">
                      {collection.repos[0]?.git_remote
                        ? collection.repos[0]!.git_remote
                        : collection.repos.length === 1
                          ? "Local plugin collection"
                          : `Contains ${collection.repos.length} plugin repositories`}
                    </div>
                  </div>

                  <button
                    type="button"
                    className="omni-btn omni-btn--sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleUpdateCollection(collection);
                    }}
                    disabled={updateDisabled}
                    aria-disabled={updateDisabled}
                    title={
                      collection.has_git_repo || collection.collection_source
                        ? "Update this collection and all of its plugins"
                        : "This collection has no git repositories"
                    }
                  >
                    Update
                  </button>

                  <button
                    type="button"
                    className="omni-btn omni-btn--danger omni-btn--xs"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleRemoveCollection(collection);
                    }}
                    title="Remove collection"
                  >
                    Remove
                  </button>
                </div>

                {expanded && (
                  <div className="collection-card__body">
                    <div className="collection-card__body-title">
                      Plugins in this collection
                    </div>
                    {collection.plugins.map((plugin) => {
                      const updatePluginDisabled =
                        status.type === "loading" || !plugin.repo_is_git_repo;
                      return (
                        <div
                          key={`${plugin.repo_dir_name}:${plugin.name}`}
                          className="plugin-row"
                        >
                          <div className="plugin-row__inner">
                            <div className="plugin-row__main">
                              <div className="plugin-row__title-row">
                                <span className="plugin-row__kind">PLUGIN</span>
                                <span>{plugin.icon ?? "🔌"}</span>
                                <span>{plugin.name}</span>
                                <span className="plugin-row__version">
                                  v{plugin.version}
                                </span>
                                {plugin.keyword && (
                                  <span className="plugin-row__keyword">
                                    {plugin.keyword}
                                  </span>
                                )}
                              </div>
                              <div className="plugin-row__desc">
                                {plugin.description}
                              </div>
                            </div>

                            <button
                              type="button"
                              className="omni-btn omni-btn--xs plugin-row__action"
                              onClick={() =>
                                handleUpdateRepo(plugin.repo_dir_name)
                              }
                              disabled={updatePluginDisabled}
                              aria-disabled={updatePluginDisabled}
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
                      );
                    })}
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
