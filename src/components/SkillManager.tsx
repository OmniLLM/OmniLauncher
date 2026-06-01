import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SkillInfo {
  name: string;
  description: string;
  version: string;
  triggers: string[];
  tags: string[];
  tools_hint: string[];
  path: string;
}

interface SkillManagerProps {
  onClose: () => void;
}

type Status =
  | { type: "idle"; message: "" }
  | { type: "loading"; message: string }
  | { type: "success"; message: string }
  | { type: "error"; message: string };

export default function SkillManager({ onClose }: SkillManagerProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [expandedSkills, setExpandedSkills] = useState<Record<string, boolean>>({});
  const [source, setSource] = useState("");
  const [status, setStatus] = useState<Status>({ type: "idle", message: "" });

  const refresh = useCallback(() => {
    invoke<SkillInfo[]>("list_skills")
      .then((list) => setSkills(list))
      .catch(() => setSkills([]));
  }, []);

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
      const message = await invoke<string>("install_skill", { source: trimmed });
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
        <button
          className="omni-titlebar__close"
          onClick={onClose}
          title="Close"
          aria-label="Close"
        >
          ×
        </button>
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

      {/* ── Skill list ── */}
      <div className="skill-panel__list">
        {skills.length === 0 ? (
          <div className="skill-panel__empty">
            No skills installed yet.
            <br />
            <span className="skill-panel__empty-hint">
              Paste a URL or local path above to install one.
              GitHub URLs use <code>gh</code> when authenticated (private &amp; GHE supported).
            </span>
          </div>
        ) : (
          skills.map((skill) => {
            const expanded = expandedSkills[skill.name] ?? false;
            return (
              <div key={skill.name} className="skill-card">
                {/* ── Card header row ── */}
                <div
                  className="skill-card__head"
                  onClick={() => toggleSkill(skill.name)}
                >
                  {/* Expand toggle — pill shaped */}
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
                    aria-label={expanded ? "Collapse skill details" : "Expand skill details"}
                    aria-expanded={expanded}
                    title={expanded ? "Collapse details" : "Expand details"}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>

                  {/* Name + badges */}
                  <div className="skill-card__main">
                    <div className="skill-card__title-row">
                      <span className="skill-card__kind">SKILL</span>
                      <span className="skill-card__name">{skill.name}</span>
                      {skill.version && (
                        <span className="skill-card__version">v{skill.version}</span>
                      )}
                      {skill.tags.slice(0, 3).map((tag) => (
                        <span key={tag} className="skill-card__tag">
                          {tag}
                        </span>
                      ))}
                    </div>
                    {skill.description && (
                      <div className="skill-card__desc">{skill.description}</div>
                    )}
                  </div>

                  {/* Action buttons */}
                  <div className="skill-card__actions" style={{ display: "contents" }}>
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
