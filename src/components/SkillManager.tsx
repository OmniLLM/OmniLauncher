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
    refresh();
  }, [refresh]);

  // Auto-expand newly installed skills
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

  const handleDelete = async (name: string) => {
    setStatus({ type: "loading", message: `Removing skill "${name}"…` });
    try {
      const message = await invoke<string>("delete_skill", { name });
      setStatus({ type: "success", message: `✓ ${message}` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleReload = async () => {
    setStatus({ type: "loading", message: "Reloading skills…" });
    try {
      await invoke<boolean>("reload_skills");
      setStatus({ type: "success", message: "✓ Skills reloaded" });
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
      ? "#a6e3a1"
      : status.type === "error"
        ? "#f38ba8"
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
            fontSize: "13px",
            fontWeight: 700,
            color: colors.accent,
            letterSpacing: "0.04em",
            display: "flex",
            alignItems: "center",
            gap: "6px",
          }}
        >
          🧠 Skill Manager
          <span
            style={{
              fontSize: "11px",
              fontWeight: 500,
              color: colors.sub,
              border: `1px solid ${colors.surface2}`,
              borderRadius: "999px",
              padding: "1px 7px",
            }}
          >
            {skills.length} skill{skills.length === 1 ? "" : "s"}
          </span>
        </span>

        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          <button
            onClick={handleReload}
            disabled={status.type === "loading"}
            title="Hot-reload all skills"
            style={{
              background: "none",
              border: `1px solid ${colors.surface2}`,
              borderRadius: "8px",
              padding: "4px 10px",
              color: colors.text,
              fontSize: "12px",
              fontWeight: 600,
              cursor: "pointer",
              opacity: status.type === "loading" ? 0.5 : 1,
              transition: "opacity 150ms",
            }}
          >
            ↻ Reload
          </button>
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
      </div>

      {/* ── Install row ── */}
      <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
        <input
          type="text"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="URL or local path to SKILL.md…"
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

      {/* ── Status message ── */}
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

      {/* ── Skill list ── */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "6px",
        }}
      >
        {skills.length === 0 ? (
          <div
            style={{
              fontSize: "13px",
              color: colors.sub,
              textAlign: "center",
              padding: "18px 0",
            }}
          >
            No skills installed yet.
            <br />
            <span style={{ fontSize: "12px", opacity: 0.7 }}>
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
                  overflow: "hidden",
                }}
              >
                {/* ── Skill row ── */}
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
                  {/* Expand toggle */}
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleSkill(skill.name);
                    }}
                    aria-label={expanded ? "Collapse skill" : "Expand skill"}
                    style={{
                      width: "26px",
                      height: "26px",
                      borderRadius: "6px",
                      border: `1px solid ${colors.surface2}`,
                      background: colors.bg,
                      color: colors.text,
                      cursor: "pointer",
                      flexShrink: 0,
                      fontSize: "12px",
                    }}
                    title={expanded ? "Collapse" : "Expand details"}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>

                  {/* Skill info */}
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
                      {/* SKILL badge */}
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
                        SKILL
                      </span>

                      <span>🧠</span>
                      <span>{skill.name}</span>

                      {skill.version && (
                        <span style={{ color: colors.sub, fontWeight: 400 }}>
                          v{skill.version}
                        </span>
                      )}

                      {/* Tags */}
                      {skill.tags.slice(0, 3).map((tag) => (
                        <span
                          key={tag}
                          style={{
                            fontSize: "10px",
                            background: `${colors.accent}22`,
                            border: `1px solid ${colors.accent}44`,
                            borderRadius: "999px",
                            padding: "1px 6px",
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
                          marginTop: "2px",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {skill.description}
                      </div>
                    )}
                  </div>

                  {/* Delete button */}
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(skill.name);
                    }}
                    disabled={status.type === "loading"}
                    title="Remove skill"
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

                {/* ── Expanded detail ── */}
                {expanded && (
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: "8px",
                      padding: "0 12px 12px 48px",
                      borderTop: `1px solid ${colors.surface2}`,
                      paddingTop: "10px",
                      marginLeft: "24px",
                      borderLeft: `1px solid ${colors.surface2}`,
                    }}
                  >
                    {/* Triggers */}
                    {skill.triggers.length > 0 && (
                      <DetailRow label="Triggers" colors={colors}>
                        <div style={{ display: "flex", gap: "5px", flexWrap: "wrap" }}>
                          {skill.triggers.map((t) => (
                            <span
                              key={t}
                              style={{
                                background: colors.bg,
                                border: `1px solid ${colors.surface2}`,
                                borderRadius: "5px",
                                fontSize: "11px",
                                padding: "2px 7px",
                                color: colors.text,
                                fontFamily: "monospace",
                              }}
                            >
                              {t}
                            </span>
                          ))}
                        </div>
                      </DetailRow>
                    )}

                    {/* Tools hint */}
                    {skill.tools_hint.length > 0 && (
                      <DetailRow label="Tools" colors={colors}>
                        <div style={{ display: "flex", gap: "5px", flexWrap: "wrap" }}>
                          {skill.tools_hint.map((t) => (
                            <span
                              key={t}
                              style={{
                                background: `${colors.accent}15`,
                                border: `1px solid ${colors.accent}44`,
                                borderRadius: "5px",
                                fontSize: "11px",
                                padding: "2px 7px",
                                color: colors.accent,
                                fontFamily: "monospace",
                              }}
                            >
                              {t}
                            </span>
                          ))}
                        </div>
                      </DetailRow>
                    )}

                    {/* Path */}
                    <DetailRow label="Path" colors={colors}>
                      <span
                        style={{
                          fontSize: "11px",
                          color: colors.sub,
                          fontFamily: "monospace",
                          wordBreak: "break-all",
                        }}
                      >
                        {skill.path}
                      </span>
                    </DetailRow>
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

// ── Detail row ────────────────────────────────────────────────────────────────

function DetailRow({
  label,
  children,
  colors,
}: {
  label: string;
  children: React.ReactNode;
  colors: SkillManagerProps["colors"];
}) {
  return (
    <div>
      <div
        style={{
          color: colors.sub,
          fontSize: "10px",
          fontWeight: 700,
          letterSpacing: "0.05em",
          textTransform: "uppercase",
          marginBottom: "4px",
        }}
      >
        {label}
      </div>
      {children}
    </div>
  );
}
