import { useState, useEffect, useCallback } from "react";
import { invoke } from "../lib/runtime";
import type { SkillInfo } from "../types/app";

type SkillState = "Active" | "Stale" | "Archived";

interface SkillUsage {
  uses: number;
  last_used: number;
  first_seen: number;
  state: SkillState;
  pinned: boolean;
}

interface UsageStore {
  skills: Record<string, SkillUsage>;
}

interface CuratorReport {
  marked_stale: string[];
  marked_archived: string[];
  seen_new: string[];
  total_tracked: number;
}

type Proposal =
  | {
      kind: "merge";
      primary: string;
      secondary: string;
      rationale: string;
      merged_body: string;
    }
  | {
      kind: "rewrite";
      name: string;
      rationale: string;
      new_body: string;
    }
  | { kind: "archive"; name: string; rationale: string };

interface ApplyOutcome {
  message: string;
  backups: string[];
}

interface SkillManagerProps {
  onClose: () => void;
}

type Status =
  | { type: "idle"; message: "" }
  | { type: "loading"; message: string }
  | { type: "success"; message: string }
  | { type: "error"; message: string };

function formatRelative(unixSecs: number): string {
  if (!unixSecs) return "never";
  const diff = Math.floor(Date.now() / 1000) - unixSecs;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export default function SkillManager({ onClose }: SkillManagerProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [usage, setUsage] = useState<Record<string, SkillUsage>>({});
  const [proposals, setProposals] = useState<Proposal[] | null>(null);
  const [proposing, setProposing] = useState(false);
  const [expandedSkills, setExpandedSkills] = useState<Record<string, boolean>>(
    {},
  );
  const [source, setSource] = useState("");
  const [status, setStatus] = useState<Status>({ type: "idle", message: "" });

  const refreshUsage = useCallback(() => {
    invoke<UsageStore>("list_skill_usage")
      .then((store) => setUsage(store.skills ?? {}))
      .catch(() => setUsage({}));
  }, []);

  const refresh = useCallback(() => {
    invoke<SkillInfo[]>("list_skills")
      .then((list) => setSkills(list))
      .catch(() => setSkills([]));
    refreshUsage();
  }, [refreshUsage]);

  useEffect(() => {
    setExpandedSkills((current) => {
      const next = { ...current };
      for (const skill of skills) {
        if (next[skill.name] === undefined) {
          next[skill.name] = false;
        }
      }
      return next;
    });
  }, [skills]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleInstall = async () => {
    const trimmed = source.trim();
    if (!trimmed) return;
    setStatus({ type: "loading", message: "Installing…" });
    try {
      const message = await invoke<string>("install_skill", {
        source: trimmed,
      });
      setStatus({ type: "success", message: `✓ ${message}` });
      setSource("");
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleUpdate = async (name: string) => {
    setStatus({ type: "loading", message: `Updating "${name}"…` });
    try {
      const message = await invoke<string>("update_skill", { name });
      setStatus({ type: "success", message: `✓ ${message}` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleDelete = async (name: string) => {
    setStatus({ type: "loading", message: `Removing "${name}"…` });
    try {
      const message = await invoke<string>("delete_skill", { name });
      setStatus({ type: "success", message: `✓ ${message}` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handlePin = async (name: string, pinned: boolean) => {
    try {
      await invoke<boolean>("pin_skill", { name, pinned });
      refreshUsage();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleRunCurator = async () => {
    setStatus({ type: "loading", message: "Running curator…" });
    try {
      const r = await invoke<CuratorReport>("run_curator_now");
      const parts: string[] = [];
      if (r.seen_new.length) parts.push(`${r.seen_new.length} new`);
      if (r.marked_stale.length) parts.push(`${r.marked_stale.length} stale`);
      if (r.marked_archived.length)
        parts.push(`${r.marked_archived.length} archived`);
      const summary = parts.length ? parts.join(" · ") : "no changes";
      setStatus({
        type: "success",
        message: `✓ Curator: ${summary} (${r.total_tracked} tracked)`,
      });
      refreshUsage();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleProposeConsolidation = async () => {
    setProposing(true);
    setStatus({
      type: "loading",
      message: "Asking the LLM for consolidation suggestions…",
    });
    try {
      const list = await invoke<Proposal[]>("propose_skill_consolidation");
      setProposals(list);
      setStatus({
        type: "success",
        message:
          list.length === 0
            ? "✓ Library looks healthy — no consolidations suggested."
            : `✓ ${list.length} suggestion${list.length === 1 ? "" : "s"} — review below.`,
      });
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    } finally {
      setProposing(false);
    }
  };

  const handleApplyProposal = async (p: Proposal, idx: number) => {
    const label =
      p.kind === "merge"
        ? `merge "${p.secondary}" into "${p.primary}"`
        : p.kind === "rewrite"
          ? `rewrite "${p.name}"`
          : `archive "${p.name}"`;
    if (
      !window.confirm(
        `Apply: ${label}?\n\nA backup of any modified SKILL.md will be saved before the change.`,
      )
    ) {
      return;
    }
    setStatus({ type: "loading", message: `Applying: ${label}…` });
    try {
      const out = await invoke<ApplyOutcome>("apply_skill_consolidation", {
        proposal: p,
      });
      setStatus({
        type: "success",
        message: `✓ ${out.message}${out.backups.length ? ` (backup: ${out.backups.length})` : ""}`,
      });
      // Drop the applied item from the list and refresh skill data.
      setProposals((cur) => (cur ? cur.filter((_, i) => i !== idx) : cur));
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleRejectProposal = (idx: number) => {
    setProposals((cur) => (cur ? cur.filter((_, i) => i !== idx) : cur));
  };

  const toggleSkill = (name: string) => {
    setExpandedSkills((current) => ({
      ...current,
      [name]: !current[name],
    }));
  };

  const installDisabled = status.type === "loading" || !source.trim();

  return (
    <div className="skill-panel">
      {/* ── Header ── */}
      <div className="skill-panel__header">
        <span className="skill-panel__title">
          🧠 Skill Manager
          <span className="skill-panel__count">
            {skills.length} skill{skills.length === 1 ? "" : "s"}
          </span>
        </span>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button
            type="button"
            className="omni-btn omni-btn--sm"
            onClick={handleRunCurator}
            disabled={status.type === "loading"}
            title="Re-evaluate skill usage and lifecycle states now"
          >
            ↻ Run curator
          </button>
          <button
            type="button"
            className="omni-btn omni-btn--sm"
            onClick={handleProposeConsolidation}
            disabled={status.type === "loading" || proposing}
            title="Ask the LLM for consolidation suggestions (read-only — nothing is applied without your approval)"
          >
            🪄 Consolidate…
          </button>
          <button
            className="omni-titlebar__close"
            onClick={onClose}
            title="Close"
            aria-label="Close"
          >
            ×
          </button>
        </div>
      </div>

      {/* ── Install row ── */}
      <div className="skill-panel__install-row">
        <input
          type="text"
          className="omni-input"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="URL or local path to SKILL.md…"
        />
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

      {/* ── Status message ── */}
      {status.type !== "idle" && (
        <div
          className={
            "skill-panel__status" +
            (status.type === "success"
              ? " skill-panel__status--success"
              : status.type === "error"
                ? " skill-panel__status--error"
                : "")
          }
        >
          {status.message}
        </div>
      )}

      {/* ── Consolidation proposals ── */}
      {proposals && proposals.length > 0 && (
        <div
          className="skill-panel__proposals"
          style={{
            border: "1px solid #b08a45",
            borderRadius: 6,
            padding: 12,
            margin: "8px 0",
            background: "rgba(176, 138, 69, 0.08)",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: 8,
            }}
          >
            <strong>🪄 LLM consolidation suggestions</strong>
            <button
              type="button"
              className="omni-btn omni-btn--xs"
              onClick={() => setProposals(null)}
              title="Dismiss all suggestions"
            >
              Dismiss all
            </button>
          </div>
          <div style={{ fontSize: 12, opacity: 0.8, marginBottom: 8 }}>
            Each suggestion is read-only until you approve. Backups are written
            before any file change.
          </div>
          {proposals.map((p, i) => {
            const title =
              p.kind === "merge"
                ? `Merge "${p.secondary}" → "${p.primary}"`
                : p.kind === "rewrite"
                  ? `Rewrite "${p.name}"`
                  : `Archive "${p.name}"`;
            const bodyPreview =
              p.kind === "merge"
                ? p.merged_body.slice(0, 320)
                : p.kind === "rewrite"
                  ? p.new_body.slice(0, 320)
                  : "";
            return (
              <div
                key={i}
                style={{
                  borderTop:
                    i === 0 ? "none" : "1px solid rgba(176,138,69,0.3)",
                  padding: "8px 0",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    gap: 8,
                  }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 600 }}>
                      <span className="skill-card__kind">
                        {p.kind.toUpperCase()}
                      </span>{" "}
                      {title}
                    </div>
                    <div style={{ fontSize: 12, opacity: 0.85, marginTop: 4 }}>
                      {p.rationale}
                    </div>
                    {bodyPreview && (
                      <details style={{ marginTop: 6 }}>
                        <summary style={{ cursor: "pointer", fontSize: 12 }}>
                          Preview new body ({bodyPreview.length}+ chars)
                        </summary>
                        <pre
                          style={{
                            fontSize: 11,
                            background: "rgba(0,0,0,0.15)",
                            padding: 6,
                            borderRadius: 4,
                            maxHeight: 180,
                            overflow: "auto",
                            whiteSpace: "pre-wrap",
                          }}
                        >
                          {bodyPreview}
                          {bodyPreview.length >= 320 ? "…" : ""}
                        </pre>
                      </details>
                    )}
                  </div>
                  <div
                    style={{
                      display: "flex",
                      gap: 6,
                      alignItems: "flex-start",
                    }}
                  >
                    <button
                      type="button"
                      className="omni-btn omni-btn--primary omni-btn--xs"
                      onClick={() => handleApplyProposal(p, i)}
                      disabled={status.type === "loading"}
                    >
                      Approve
                    </button>
                    <button
                      type="button"
                      className="omni-btn omni-btn--xs"
                      onClick={() => handleRejectProposal(i)}
                    >
                      Reject
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* ── Skill list ── */}
      <div className="skill-panel__list">
        {skills.length === 0 ? (
          <div className="skill-panel__empty">
            No skills installed yet.
            <br />
            <span className="skill-panel__empty-hint">
              Paste a URL or local path above to install one. GitHub URLs use{" "}
              <code>gh</code> when authenticated (private &amp; GHE supported).
            </span>
          </div>
        ) : (
          skills.map((skill) => {
            const expanded = expandedSkills[skill.name] ?? false;
            const u = usage[skill.name];
            const state: SkillState = u?.state ?? "Active";
            const pinned = u?.pinned ?? false;
            const stateColor =
              state === "Archived"
                ? "#a36363"
                : state === "Stale"
                  ? "#b08a45"
                  : "#5e8a5e";
            return (
              <div key={skill.name} className="skill-card">
                {/* ── Card header row ── */}
                <div
                  className="skill-card__head"
                  onClick={() => toggleSkill(skill.name)}
                >
                  <button
                    type="button"
                    className={
                      "omni-btn omni-btn--ghost omni-btn--xs skill-card__expand" +
                      (expanded ? " is-active" : "")
                    }
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleSkill(skill.name);
                    }}
                    aria-label={
                      expanded
                        ? "Collapse skill details"
                        : "Expand skill details"
                    }
                    aria-expanded={expanded}
                    title={expanded ? "Collapse details" : "Expand details"}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>

                  <div className="skill-card__main">
                    <div className="skill-card__title-row">
                      <span className="skill-card__kind">SKILL</span>
                      <span className="skill-card__name">{skill.name}</span>
                      {skill.version && (
                        <span className="skill-card__version">
                          v{skill.version}
                        </span>
                      )}
                      <span
                        className="skill-card__tag"
                        style={{ color: stateColor, borderColor: stateColor }}
                        title={`Lifecycle state: ${state.toLowerCase()}`}
                      >
                        {state.toLowerCase()}
                      </span>
                      {u && (
                        <span
                          className="skill-card__tag"
                          title={`Last used ${formatRelative(u.last_used)}`}
                        >
                          {u.uses} use{u.uses === 1 ? "" : "s"} ·{" "}
                          {formatRelative(u.last_used)}
                        </span>
                      )}
                      {skill.tags.slice(0, 2).map((tag) => (
                        <span key={tag} className="skill-card__tag">
                          {tag}
                        </span>
                      ))}
                    </div>
                    {skill.description && (
                      <div className="skill-card__desc">
                        {skill.description}
                      </div>
                    )}
                  </div>

                  <div
                    className="skill-card__actions"
                    style={{ display: "contents" }}
                  >
                    <button
                      type="button"
                      className={
                        "omni-btn omni-btn--xs" +
                        (pinned ? " omni-btn--primary" : "")
                      }
                      onClick={(e) => {
                        e.stopPropagation();
                        handlePin(skill.name, !pinned);
                      }}
                      title={
                        pinned
                          ? "Unpin (allow auto-archive)"
                          : "Pin (exempt from auto-stale / auto-archive)"
                      }
                    >
                      {pinned ? "📌 Pinned" : "📌 Pin"}
                    </button>
                    <button
                      type="button"
                      className="omni-btn omni-btn--sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleUpdate(skill.name);
                      }}
                      disabled={status.type === "loading"}
                      aria-disabled={status.type === "loading"}
                      title="Update this skill"
                    >
                      Update
                    </button>
                    <button
                      type="button"
                      className="omni-btn omni-btn--danger omni-btn--xs"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(skill.name);
                      }}
                      title="Remove skill"
                    >
                      Remove
                    </button>
                  </div>
                </div>

                {/* ── Expanded metadata grid ── */}
                {expanded && (
                  <div className="skill-card__details">
                    <div className="skill-card__meta-grid">
                      {skill.triggers.length > 0 && (
                        <>
                          <div className="skill-card__meta-label">Triggers</div>
                          <div className="skill-card__chip-row">
                            {skill.triggers.map((t) => (
                              <span key={t} className="skill-card__chip">
                                {t}
                              </span>
                            ))}
                          </div>
                        </>
                      )}

                      {skill.tools_hint.length > 0 && (
                        <>
                          <div className="skill-card__meta-label">Tools</div>
                          <div className="skill-card__chip-row">
                            {skill.tools_hint.map((t) => (
                              <span
                                key={t}
                                className="skill-card__chip skill-card__chip--accent"
                              >
                                {t}
                              </span>
                            ))}
                          </div>
                        </>
                      )}

                      {u && (
                        <>
                          <div className="skill-card__meta-label">Usage</div>
                          <div className="skill-card__chip-row">
                            <span className="skill-card__chip">
                              {u.uses} use{u.uses === 1 ? "" : "s"}
                            </span>
                            <span className="skill-card__chip">
                              last: {formatRelative(u.last_used)}
                            </span>
                            <span className="skill-card__chip">
                              first seen: {formatRelative(u.first_seen)}
                            </span>
                            <span
                              className="skill-card__chip"
                              style={{
                                color: stateColor,
                                borderColor: stateColor,
                              }}
                            >
                              state: {state.toLowerCase()}
                            </span>
                            {pinned && (
                              <span className="skill-card__chip skill-card__chip--accent">
                                pinned
                              </span>
                            )}
                          </div>
                        </>
                      )}

                      {skill.path && (
                        <>
                          <div className="skill-card__meta-label">Path</div>
                          <div className="skill-card__path">{skill.path}</div>
                        </>
                      )}
                    </div>
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
