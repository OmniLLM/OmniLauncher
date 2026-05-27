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
  const [selectedSkill, setSelectedSkill] = useState<SkillInfo | null>(null);
  const [filter, setFilter] = useState("");
  const [status, setStatus] = useState<{
    type: "idle" | "loading" | "success" | "error";
    message: string;
  }>({ type: "idle", message: "" });
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

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
      const msg = await invoke<string>("install_skill", { source: trimmed });
      setStatus({ type: "success", message: `✓ ${msg}` });
      setSource("");
      refresh();
    } catch (e) {
      setStatus({ type: "error", message: `✗ ${e}` });
    }
  };

  const handleDelete = async (name: string) => {
    setConfirmDelete(null);
    setStatus({ type: "loading", message: `Deleting "${name}"…` });
    try {
      const msg = await invoke<string>("delete_skill", { name });
      setStatus({ type: "success", message: `✓ ${msg}` });
      if (selectedSkill?.name === name) setSelectedSkill(null);
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

  const filteredSkills = skills.filter(
    (s) =>
      !filter ||
      s.name.toLowerCase().includes(filter.toLowerCase()) ||
      s.description.toLowerCase().includes(filter.toLowerCase()) ||
      s.tags.some((t) => t.toLowerCase().includes(filter.toLowerCase())),
  );

  const statusColor =
    status.type === "success"
      ? "#4ade80"
      : status.type === "error"
        ? "#f87171"
        : status.type === "loading"
          ? colors.accent
          : colors.sub;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        backgroundColor: colors.bg,
        color: colors.text,
        fontFamily: "inherit",
        overflow: "hidden",
      }}
    >
      {/* ── Header ── */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "12px 16px 10px",
          borderBottom: `1px solid ${colors.surface2}`,
          flexShrink: 0,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 18 }}>🧠</span>
          <span style={{ fontWeight: 700, fontSize: 15, letterSpacing: 0.3 }}>
            Skill Manager
          </span>
          <span
            style={{
              background: colors.surface2,
              color: colors.sub,
              borderRadius: 10,
              fontSize: 11,
              padding: "1px 7px",
              fontWeight: 600,
            }}
          >
            {skills.length}
          </span>
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <button
            onClick={handleReload}
            title="Reload all skills"
            style={{
              background: colors.surface,
              border: `1px solid ${colors.surface2}`,
              borderRadius: 6,
              color: colors.sub,
              cursor: "pointer",
              fontSize: 12,
              padding: "4px 10px",
              display: "flex",
              alignItems: "center",
              gap: 4,
            }}
          >
            ↻ Reload
          </button>
          <button
            onClick={onClose}
            style={{
              background: "transparent",
              border: "none",
              color: colors.sub,
              cursor: "pointer",
              fontSize: 18,
              lineHeight: 1,
              padding: "2px 4px",
            }}
          >
            ×
          </button>
        </div>
      </div>

      {/* ── Install bar ── */}
      <div
        style={{
          padding: "10px 16px",
          borderBottom: `1px solid ${colors.surface2}`,
          flexShrink: 0,
        }}
      >
        <div style={{ display: "flex", gap: 8 }}>
          <input
            value={source}
            onChange={(e) => setSource(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleInstall()}
            placeholder="Install from URL or local path  (e.g. https://…/SKILL.md  or  ~/my-skill/SKILL.md)"
            style={{
              flex: 1,
              background: colors.surface,
              border: `1px solid ${colors.surface2}`,
              borderRadius: 7,
              color: colors.text,
              fontSize: 12,
              outline: "none",
              padding: "7px 12px",
            }}
          />
          <button
            onClick={handleInstall}
            disabled={!source.trim() || status.type === "loading"}
            style={{
              background:
                source.trim() && status.type !== "loading"
                  ? colors.accent
                  : colors.surface2,
              border: "none",
              borderRadius: 7,
              color:
                source.trim() && status.type !== "loading"
                  ? "#fff"
                  : colors.sub,
              cursor:
                source.trim() && status.type !== "loading"
                  ? "pointer"
                  : "default",
              fontSize: 12,
              fontWeight: 600,
              padding: "7px 16px",
              transition: "background 0.15s",
              whiteSpace: "nowrap",
            }}
          >
            + Install
          </button>
        </div>
        {status.type !== "idle" && (
          <div
            style={{
              color: statusColor,
              fontSize: 12,
              marginTop: 6,
              fontStyle: status.type === "loading" ? "italic" : undefined,
            }}
          >
            {status.message}
          </div>
        )}
      </div>

      {/* ── Body: list + detail ── */}
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        {/* Skill list */}
        <div
          style={{
            width: 220,
            borderRight: `1px solid ${colors.surface2}`,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
            flexShrink: 0,
          }}
        >
          {/* Filter */}
          <div
            style={{
              padding: "8px 10px",
              borderBottom: `1px solid ${colors.surface2}`,
            }}
          >
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="🔍 Filter skills…"
              style={{
                width: "100%",
                background: colors.surface,
                border: `1px solid ${colors.surface2}`,
                borderRadius: 5,
                color: colors.text,
                fontSize: 12,
                outline: "none",
                padding: "5px 8px",
                boxSizing: "border-box",
              }}
            />
          </div>

          {/* List */}
          <div style={{ overflowY: "auto", flex: 1 }}>
            {filteredSkills.length === 0 ? (
              <div
                style={{
                  color: colors.sub,
                  fontSize: 12,
                  padding: "20px 14px",
                  textAlign: "center",
                }}
              >
                {filter ? "No matching skills" : "No skills installed"}
              </div>
            ) : (
              filteredSkills.map((skill) => (
                <div
                  key={skill.name}
                  onClick={() => setSelectedSkill(skill)}
                  style={{
                    padding: "9px 12px",
                    cursor: "pointer",
                    background:
                      selectedSkill?.name === skill.name
                        ? colors.surface2
                        : "transparent",
                    borderLeft:
                      selectedSkill?.name === skill.name
                        ? `3px solid ${colors.accent}`
                        : "3px solid transparent",
                    transition: "background 0.1s",
                  }}
                  onMouseEnter={(e) => {
                    if (selectedSkill?.name !== skill.name)
                      (e.currentTarget as HTMLDivElement).style.background =
                        colors.surface;
                  }}
                  onMouseLeave={(e) => {
                    if (selectedSkill?.name !== skill.name)
                      (e.currentTarget as HTMLDivElement).style.background =
                        "transparent";
                  }}
                >
                  <div
                    style={{
                      fontWeight: 600,
                      fontSize: 13,
                      color: colors.text,
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                  >
                    {skill.name}
                  </div>
                  {skill.version && (
                    <div
                      style={{
                        fontSize: 10,
                        color: colors.sub,
                        marginTop: 1,
                      }}
                    >
                      v{skill.version}
                    </div>
                  )}
                  {skill.tags.length > 0 && (
                    <div
                      style={{
                        display: "flex",
                        gap: 3,
                        flexWrap: "wrap",
                        marginTop: 4,
                      }}
                    >
                      {skill.tags.slice(0, 3).map((tag) => (
                        <span
                          key={tag}
                          style={{
                            background: colors.accentDim,
                            color: colors.accent,
                            borderRadius: 4,
                            fontSize: 9,
                            padding: "1px 5px",
                            fontWeight: 600,
                            letterSpacing: 0.2,
                          }}
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              ))
            )}
          </div>
        </div>

        {/* Detail pane */}
        <div style={{ flex: 1, overflow: "auto", padding: "16px 18px" }}>
          {selectedSkill ? (
            <SkillDetail
              skill={selectedSkill}
              colors={colors}
              onDelete={() => setConfirmDelete(selectedSkill.name)}
            />
          ) : (
            <EmptyState colors={colors} skillCount={skills.length} />
          )}
        </div>
      </div>

      {/* ── Confirm delete modal ── */}
      {confirmDelete && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: "rgba(0,0,0,0.55)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
          onClick={() => setConfirmDelete(null)}
        >
          <div
            style={{
              background: colors.surface,
              border: `1px solid ${colors.surface2}`,
              borderRadius: 10,
              padding: "22px 26px",
              minWidth: 280,
              textAlign: "center",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div style={{ fontSize: 28, marginBottom: 10 }}>🗑️</div>
            <div
              style={{ fontWeight: 700, fontSize: 14, marginBottom: 6 }}
            >
              Delete Skill?
            </div>
            <div
              style={{
                color: colors.sub,
                fontSize: 12,
                marginBottom: 18,
                lineHeight: 1.5,
              }}
            >
              This will permanently remove{" "}
              <strong style={{ color: colors.text }}>{confirmDelete}</strong>{" "}
              from your skills directory.
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "center" }}>
              <button
                onClick={() => setConfirmDelete(null)}
                style={{
                  background: colors.surface2,
                  border: "none",
                  borderRadius: 6,
                  color: colors.text,
                  cursor: "pointer",
                  fontSize: 12,
                  fontWeight: 600,
                  padding: "7px 18px",
                }}
              >
                Cancel
              </button>
              <button
                onClick={() => handleDelete(confirmDelete)}
                style={{
                  background: "#ef4444",
                  border: "none",
                  borderRadius: 6,
                  color: "#fff",
                  cursor: "pointer",
                  fontSize: 12,
                  fontWeight: 700,
                  padding: "7px 18px",
                }}
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ─── Skill detail ─────────────────────────────────────────────────────────────

function SkillDetail({
  skill,
  colors,
  onDelete,
}: {
  skill: SkillInfo;
  colors: SkillManagerProps["colors"];
  onDelete: () => void;
}) {
  return (
    <div>
      {/* Title row */}
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          justifyContent: "space-between",
          gap: 12,
          marginBottom: 14,
        }}
      >
        <div>
          <div
            style={{ fontSize: 18, fontWeight: 800, letterSpacing: -0.3 }}
          >
            {skill.name}
          </div>
          {skill.version && (
            <div style={{ fontSize: 11, color: colors.sub, marginTop: 2 }}>
              v{skill.version}
            </div>
          )}
        </div>
        <button
          onClick={onDelete}
          title="Delete skill"
          style={{
            background: "transparent",
            border: `1px solid #ef4444`,
            borderRadius: 6,
            color: "#ef4444",
            cursor: "pointer",
            fontSize: 12,
            fontWeight: 600,
            padding: "5px 12px",
            flexShrink: 0,
          }}
        >
          🗑 Delete
        </button>
      </div>

      {/* Description */}
      {skill.description && (
        <p
          style={{
            color: colors.text,
            fontSize: 13,
            lineHeight: 1.6,
            margin: "0 0 16px",
          }}
        >
          {skill.description}
        </p>
      )}

      {/* Tags */}
      {skill.tags.length > 0 && (
        <DetailRow label="Tags" colors={colors}>
          <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
            {skill.tags.map((tag) => (
              <span
                key={tag}
                style={{
                  background: colors.accentDim,
                  color: colors.accent,
                  borderRadius: 5,
                  fontSize: 11,
                  padding: "2px 8px",
                  fontWeight: 600,
                }}
              >
                {tag}
              </span>
            ))}
          </div>
        </DetailRow>
      )}

      {/* Triggers */}
      {skill.triggers.length > 0 && (
        <DetailRow label="Triggers" colors={colors}>
          <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
            {skill.triggers.map((t) => (
              <span
                key={t}
                style={{
                  background: colors.surface2,
                  color: colors.sub,
                  borderRadius: 5,
                  fontSize: 11,
                  padding: "2px 8px",
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
          <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
            {skill.tools_hint.map((t) => (
              <span
                key={t}
                style={{
                  background: colors.surface,
                  border: `1px solid ${colors.surface2}`,
                  color: colors.text,
                  borderRadius: 5,
                  fontSize: 11,
                  padding: "2px 8px",
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
            color: colors.sub,
            fontSize: 11,
            fontFamily: "monospace",
            wordBreak: "break-all",
          }}
        >
          {skill.path}
        </span>
      </DetailRow>
    </div>
  );
}

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
    <div style={{ marginBottom: 14 }}>
      <div
        style={{
          color: colors.sub,
          fontSize: 10,
          fontWeight: 700,
          letterSpacing: 0.8,
          textTransform: "uppercase",
          marginBottom: 6,
        }}
      >
        {label}
      </div>
      {children}
    </div>
  );
}

function EmptyState({
  colors,
  skillCount,
}: {
  colors: SkillManagerProps["colors"];
  skillCount: number;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        color: colors.sub,
        textAlign: "center",
        padding: 24,
      }}
    >
      <div style={{ fontSize: 36, marginBottom: 12 }}>🧠</div>
      {skillCount === 0 ? (
        <>
          <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 6 }}>
            No skills installed
          </div>
          <div style={{ fontSize: 12, lineHeight: 1.6 }}>
            Install a skill from a URL or local path above.
            <br />
            Skills dir: <code>~/.omnilauncher/skills/</code>
          </div>
        </>
      ) : (
        <>
          <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 6 }}>
            Select a skill to view details
          </div>
          <div style={{ fontSize: 12 }}>
            {skillCount} skill{skillCount !== 1 ? "s" : ""} installed
          </div>
        </>
      )}
    </div>
  );
}
