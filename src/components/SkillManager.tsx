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

export default function SkillManager({ colors, onClose }: SkillManagerProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [expandedSkills, setExpandedSkills] = useState<Record<string, boolean>>({});
  const [source, setSource] = useState("");
  const [status, setStatus] = useState<{
    type: "idle" | "loading" | "success" | "error";
    message: string;
  }>({ type: "idle", message: "" });

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

  const statusColor =
    status.type === "success"
      ? "var(--success)"
      : status.type === "error"
        ? "var(--danger)"
        : colors.sub;

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
      {/* ── Header ── */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span
          style={{
            fontSize: "14px",
            fontWeight: 800,
            color: colors.accent,
            letterSpacing: "0.02em",
            display: "flex",
            alignItems: "center",
            gap: "8px",
          }}
        >
          🧠 Skill Manager
          <span
            style={{
              fontSize: "11px",
              fontWeight: 500,
              color: colors.sub,
              background: `${colors.surface2}88`,
              border: `1px solid ${colors.surface2}`,
              borderRadius: "999px",
              padding: "1px 8px",
            }}
          >
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
      <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
        <input
          type="text"
          className="omni-input"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="URL or local path to SKILL.md…"
          style={{ flex: 1, minWidth: 180, width: "auto" }}
        />
        <button
          type="button"
          className="omni-btn omni-btn--primary"
          onClick={handleInstall}
          disabled={status.type === "loading" || !source.trim()}
          aria-disabled={status.type === "loading" || !source.trim()}
        >
          {status.type === "loading" ? "Installing…" : "Install"}
        </button>
      </div>

      {/* ── Status message ── */}
      {status.type !== "idle" && (
        <div
          style={{
            fontSize: "13px",
            color: statusColor,
            minHeight: "20px",
            borderLeft: `3px solid ${statusColor}`,
            paddingLeft: "10px",
            opacity: 0.95,
          }}
        >
          {status.message}
        </div>
      )}

      {/* ── Skill list ── */}
      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        {skills.length === 0 ? (
          <div
            style={{
              fontSize: "13px",
              color: colors.sub,
              textAlign: "center",
              padding: "24px 0",
            }}
          >
            No skills installed yet.
            <br />
            <span style={{ fontSize: "12px", opacity: 0.65 }}>
              Paste a URL or local path above to install one.
            </span>
          </div>
        ) : (
          skills.map((skill) => {
            const expanded = expandedSkills[skill.name] ?? false;
            return (
              <div
                key={skill.name}
                style={{
                  background: colors.surface,
                  borderRadius: "10px",
                  border: `1px solid ${colors.surface2}`,
                  boxShadow: `0 1px 4px color-mix(in srgb, var(--accent) 8%, transparent), 0 1px 0 var(--border) inset`,
                  overflow: "hidden",
                  transition: "box-shadow 150ms",
                }}
              >
                {/* ── Card header row ── */}
                <div
                  onClick={() => toggleSkill(skill.name)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "10px",
                    padding: "10px 12px",
                    cursor: "pointer",
                  }}
                >
                  {/* Expand toggle — pill shaped */}
                  <button
                    type="button"
                    className={`omni-btn omni-btn--ghost omni-btn--xs${expanded ? " is-active" : ""}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleSkill(skill.name);
                    }}
                    aria-label={expanded ? "Collapse skill details" : "Expand skill details"}
                    aria-expanded={expanded}
                    style={{
                      width: 24,
                      height: 24,
                      padding: 0,
                      borderRadius: 999,
                      flexShrink: 0,
                    }}
                    title={expanded ? "Collapse details" : "Expand details"}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>

                  {/* Name + badges */}
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div
                      style={{
                        display: "flex",
                        flexWrap: "wrap",
                        alignItems: "center",
                        gap: "6px",
                        marginBottom: "2px",
                      }}
                    >
                      {/* SKILL badge */}
                      <span
                        style={{
                          fontSize: "9px",
                          fontWeight: 700,
                          letterSpacing: "0.06em",
                          color: colors.accent,
                          background: `${colors.accent}18`,
                          border: `1px solid ${colors.accent}44`,
                          borderRadius: "4px",
                          padding: "1px 5px",
                          textTransform: "uppercase",
                        }}
                      >
                        SKILL
                      </span>

                      {/* Name */}
                      <span
                        style={{
                          fontSize: "13px",
                          fontWeight: 700,
                          color: colors.text,
                        }}
                      >
                        {skill.name}
                      </span>

                      {/* Version — monospace, muted */}
                      {skill.version && (
                        <span
                          style={{
                            fontSize: "10px",
                            color: colors.sub,
                            fontFamily: "monospace",
                            opacity: 0.8,
                          }}
                        >
                          v{skill.version}
                        </span>
                      )}

                      {/* Tags */}
                      {skill.tags.slice(0, 3).map((tag) => (
                        <span
                          key={tag}
                          style={{
                            fontSize: "10px",
                            background: `${colors.accent}20`,
                            border: `1px solid ${colors.accent}40`,
                            borderRadius: "999px",
                            padding: "1px 7px",
                            color: colors.accent,
                          }}
                        >
                          {tag}
                        </span>
                      ))}
                    </div>

                    {/* Description */}
                    {skill.description && (
                      <div
                        style={{
                          fontSize: "12px",
                          color: colors.sub,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {skill.description}
                      </div>
                    )}
                  </div>

                  {/* Update button */}
                  <button
                    type="button"
                    className="omni-btn omni-btn--sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleUpdate(skill.name);
                    }}
                    disabled={status.type === "loading"}
                    aria-disabled={status.type === "loading"}
                    style={{ flexShrink: 0 }}
                    title="Update this skill"
                  >
                    Update
                  </button>

                  {/* Remove button */}
                  <button
                    type="button"
                    className="omni-btn omni-btn--danger omni-btn--xs"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(skill.name);
                    }}
                    title="Remove skill"
                    style={{ flexShrink: 0 }}
                  >
                    Remove
                  </button>
                </div>

                {/* ── Expanded metadata grid ── */}
                {expanded && (
                  <div
                    style={{
                      borderTop: `1px solid ${colors.surface2}`,
                      background: `${colors.bg}cc`,
                      padding: "10px 12px 12px 48px",
                    }}
                  >
                    <div
                      style={{
                        display: "grid",
                        gridTemplateColumns: "auto 1fr",
                        rowGap: "8px",
                        columnGap: "14px",
                        alignItems: "start",
                      }}
                    >
                      {/* Triggers row */}
                      {skill.triggers.length > 0 && (
                        <>
                          <div
                            style={{
                              fontSize: "10px",
                              fontWeight: 700,
                              letterSpacing: "0.05em",
                              color: colors.sub,
                              textTransform: "uppercase",
                              paddingTop: "2px",
                              whiteSpace: "nowrap",
                            }}
                          >
                            Triggers
                          </div>
                          <div style={{ display: "flex", gap: "4px", flexWrap: "wrap" }}>
                            {skill.triggers.map((t) => (
                              <span
                                key={t}
                                style={{
                                  background: `${colors.surface2}66`,
                                  border: `1px solid ${colors.surface2}`,
                                  borderRadius: "4px",
                                  fontSize: "11px",
                                  padding: "1px 7px",
                                  color: colors.text,
                                  fontFamily: "monospace",
                                }}
                              >
                                {t}
                              </span>
                            ))}
                          </div>
                        </>
                      )}

                      {/* Tools row */}
                      {skill.tools_hint.length > 0 && (
                        <>
                          <div
                            style={{
                              fontSize: "10px",
                              fontWeight: 700,
                              letterSpacing: "0.05em",
                              color: colors.sub,
                              textTransform: "uppercase",
                              paddingTop: "2px",
                              whiteSpace: "nowrap",
                            }}
                          >
                            Tools
                          </div>
                          <div style={{ display: "flex", gap: "4px", flexWrap: "wrap" }}>
                            {skill.tools_hint.map((t) => (
                              <span
                                key={t}
                                style={{
                                  background: `${colors.accent}12`,
                                  border: `1px solid ${colors.accent}38`,
                                  borderRadius: "4px",
                                  fontSize: "11px",
                                  padding: "1px 7px",
                                  color: colors.accent,
                                  fontFamily: "monospace",
                                }}
                              >
                                {t}
                              </span>
                            ))}
                          </div>
                        </>
                      )}

                      {/* Path row */}
                      {skill.path && (
                        <>
                          <div
                            style={{
                              fontSize: "10px",
                              fontWeight: 700,
                              letterSpacing: "0.05em",
                              color: colors.sub,
                              textTransform: "uppercase",
                              paddingTop: "2px",
                              whiteSpace: "nowrap",
                            }}
                          >
                            Path
                          </div>
                          <div
                            style={{
                              fontSize: "11px",
                              color: colors.sub,
                              fontFamily: "monospace",
                              wordBreak: "break-all",
                              opacity: 0.8,
                            }}
                          >
                            {skill.path}
                          </div>
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
