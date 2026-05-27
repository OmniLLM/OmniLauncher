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
  const [source, setSource] = useState("");
  const [status, setStatus] = useState<{
    type: "idle" | "loading" | "success" | "error";
    message: string;
  }>({ type: "idle", message: "" });
  // Per-skill busy state
  const [busySkill, setBusySkill] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<SkillInfo[]>("list_skills")
      .then((list) => setSkills(list))
      .catch(() => setSkills([]));
  }, []);

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
    setBusySkill(name);
    setStatus({ type: "loading", message: `Updating "${name}"…` });
    try {
      const message = await invoke<string>("update_skill", { name });
      setStatus({ type: "success", message: `✓ ${message}` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    } finally {
      setBusySkill(null);
    }
  };

  const handleDelete = async (name: string) => {
    setBusySkill(name);
    setStatus({ type: "loading", message: `Removing "${name}"…` });
    try {
      const message = await invoke<string>("delete_skill", { name });
      setStatus({ type: "success", message: `✓ ${message}` });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    } finally {
      setBusySkill(null);
    }
  };

  const handleReload = async () => {
    setStatus({ type: "loading", message: "Reloading…" });
    try {
      await invoke<boolean>("reload_skills");
      setStatus({ type: "success", message: "✓ Skills reloaded" });
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const statusColor =
    status.type === "success"
      ? "#a6e3a1"
      : status.type === "error"
        ? "#f38ba8"
        : colors.sub;

  const btn = (
    label: string,
    onClick: () => void,
    opts: { danger?: boolean; disabled?: boolean; primary?: boolean } = {}
  ) => (
    <button
      onClick={onClick}
      disabled={opts.disabled}
      style={{
        background: opts.primary ? colors.accent : "none",
        border: `1px solid ${opts.danger ? "transparent" : colors.surface2}`,
        borderRadius: "7px",
        padding: "4px 11px",
        color: opts.primary ? "#fff" : opts.danger ? colors.sub : colors.text,
        fontSize: "12px",
        fontWeight: 600,
        cursor: opts.disabled ? "default" : "pointer",
        opacity: opts.disabled ? 0.45 : 1,
        transition: "color 130ms, border-color 130ms, opacity 130ms",
        flexShrink: 0,
      }}
      onMouseEnter={(e) => {
        if (opts.disabled) return;
        if (opts.danger) {
          e.currentTarget.style.color = "#f38ba8";
          e.currentTarget.style.borderColor = "#f38ba8";
        }
      }}
      onMouseLeave={(e) => {
        if (opts.danger) {
          e.currentTarget.style.color = colors.sub;
          e.currentTarget.style.borderColor = "transparent";
        }
      }}
    >
      {label}
    </button>
  );

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
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
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
              borderRadius: "7px",
              padding: "4px 10px",
              color: colors.text,
              fontSize: "12px",
              fontWeight: 600,
              cursor: "pointer",
              opacity: status.type === "loading" ? 0.5 : 1,
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
              fontSize: "18px",
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
      <div style={{ display: "flex", gap: "8px" }}>
        <input
          type="text"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleInstall()}
          placeholder="URL or local path to SKILL.md…"
          style={{
            flex: 1,
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
            padding: "7px 16px",
            color: "#fff",
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

      {/* ── Status ── */}
      {status.type !== "idle" && (
        <div style={{ fontSize: "12px", color: statusColor, minHeight: "18px" }}>
          {status.message}
        </div>
      )}

      {/* ── Skill cards ── */}
      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
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
            <span style={{ fontSize: "12px", opacity: 0.7 }}>
              Paste a URL or local path above to install one.
            </span>
          </div>
        ) : (
          skills.map((skill) => {
            const busy = busySkill === skill.name;
            return (
              <div
                key={skill.name}
                style={{
                  background: colors.surface,
                  border: `1px solid ${colors.surface2}`,
                  borderRadius: "10px",
                  padding: "12px 14px",
                  display: "flex",
                  flexDirection: "column",
                  gap: "8px",
                }}
              >
                {/* ── Card header row ── */}
                <div style={{ display: "flex", alignItems: "flex-start", gap: "10px" }}>
                  {/* Left: name + badges */}
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: "6px",
                        marginBottom: "3px",
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

                      {/* Name */}
                      <span
                        style={{
                          fontSize: "13px",
                          fontWeight: 700,
                          color: colors.text,
                        }}
                      >
                        🧠 {skill.name}
                      </span>

                      {/* Version */}
                      {skill.version && (
                        <span style={{ fontSize: "11px", color: colors.sub }}>
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
                      <div style={{ fontSize: "12px", color: colors.sub }}>
                        {skill.description}
                      </div>
                    )}
                  </div>

                  {/* Right: action buttons */}
                  <div style={{ display: "flex", gap: "6px", flexShrink: 0, paddingTop: "1px" }}>
                    {btn("Update", () => handleUpdate(skill.name), { disabled: busy })}
                    {btn("Remove", () => handleDelete(skill.name), { danger: true, disabled: busy })}
                  </div>
                </div>

                {/* ── Card detail row ── */}
                <div
                  style={{
                    display: "flex",
                    gap: "16px",
                    flexWrap: "wrap",
                    borderTop: `1px solid ${colors.surface2}`,
                    paddingTop: "8px",
                  }}
                >
                  {skill.triggers.length > 0 && (
                    <div>
                      <div
                        style={{
                          fontSize: "10px",
                          fontWeight: 700,
                          letterSpacing: "0.04em",
                          color: colors.sub,
                          textTransform: "uppercase",
                          marginBottom: "4px",
                        }}
                      >
                        Triggers
                      </div>
                      <div style={{ display: "flex", gap: "4px", flexWrap: "wrap" }}>
                        {skill.triggers.map((t) => (
                          <span
                            key={t}
                            style={{
                              background: colors.bg,
                              border: `1px solid ${colors.surface2}`,
                              borderRadius: "4px",
                              fontSize: "11px",
                              padding: "1px 6px",
                              color: colors.text,
                              fontFamily: "monospace",
                            }}
                          >
                            {t}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {skill.tools_hint.length > 0 && (
                    <div>
                      <div
                        style={{
                          fontSize: "10px",
                          fontWeight: 700,
                          letterSpacing: "0.04em",
                          color: colors.sub,
                          textTransform: "uppercase",
                          marginBottom: "4px",
                        }}
                      >
                        Tools
                      </div>
                      <div style={{ display: "flex", gap: "4px", flexWrap: "wrap" }}>
                        {skill.tools_hint.map((t) => (
                          <span
                            key={t}
                            style={{
                              background: `${colors.accent}15`,
                              border: `1px solid ${colors.accent}40`,
                              borderRadius: "4px",
                              fontSize: "11px",
                              padding: "1px 6px",
                              color: colors.accent,
                              fontFamily: "monospace",
                            }}
                          >
                            {t}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  <div style={{ flex: 1, minWidth: "120px" }}>
                    <div
                      style={{
                        fontSize: "10px",
                        fontWeight: 700,
                        letterSpacing: "0.04em",
                        color: colors.sub,
                        textTransform: "uppercase",
                        marginBottom: "4px",
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
                      }}
                    >
                      {skill.path}
                    </div>
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
