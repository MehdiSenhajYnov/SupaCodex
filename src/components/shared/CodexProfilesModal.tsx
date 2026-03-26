import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, FolderOpen, Plus, UserCircle, X } from "lucide-react";
import { useCodexProfileStore } from "../../stores/codexProfileStore";
import type { CodexProfile } from "../../types";

interface DraftProfile extends CodexProfile {}

function nextProfileId(profiles: DraftProfile[]): string {
  const base = "profile";
  let index = 1;
  while (profiles.some((profile) => profile.id === `${base}-${index}`)) {
    index += 1;
  }
  return `${base}-${index}`;
}

function normalizeDraftProfile(profile: DraftProfile): DraftProfile {
  return {
    ...profile,
    id: profile.id.trim(),
    name: profile.name.trim(),
    codexHome: profile.codexHome.trim(),
  };
}

export function CodexProfilesModal() {
  const openModal = useCodexProfileStore((s) => s.modalOpen);
  const profiles = useCodexProfileStore((s) => s.profiles);
  const activeProfileId = useCodexProfileStore((s) => s.activeProfileId);
  const loading = useCodexProfileStore((s) => s.loading);
  const loadedOnce = useCodexProfileStore((s) => s.loadedOnce);
  const error = useCodexProfileStore((s) => s.error);
  const load = useCodexProfileStore((s) => s.load);
  const saveProfiles = useCodexProfileStore((s) => s.saveProfiles);
  const close = useCodexProfileStore((s) => s.closeModal);

  const [draftProfiles, setDraftProfiles] = useState<DraftProfile[]>([]);
  const [draftActiveProfileId, setDraftActiveProfileId] = useState<string>("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!openModal || loadedOnce || loading) {
      return;
    }
    void load();
  }, [load, loadedOnce, loading, openModal]);

  useEffect(() => {
    if (!openModal) {
      return;
    }

    setDraftProfiles(profiles.map((profile) => ({ ...profile })));
    setDraftActiveProfileId(activeProfileId ?? profiles[0]?.id ?? "");
  }, [activeProfileId, openModal, profiles]);

  const canSave = useMemo(() => {
    if (!draftActiveProfileId || draftProfiles.length === 0) {
      return false;
    }

    const seen = new Set<string>();
    return draftProfiles.every((profile) => {
      const normalized = normalizeDraftProfile(profile);
      if (!normalized.id || !normalized.name || !normalized.codexHome) {
        return false;
      }
      if (seen.has(normalized.id)) {
        return false;
      }
      seen.add(normalized.id);
      return true;
    });
  }, [draftActiveProfileId, draftProfiles]);

  const handleClose = useCallback(() => close(), [close]);

  useEffect(() => {
    if (!openModal) {
      return;
    }

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.stopPropagation();
        handleClose();
      }
    }

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [handleClose, openModal]);

  if (!openModal) {
    return null;
  }

  async function browseProfileHome(index: number) {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: draftProfiles[index]?.codexHome || undefined,
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }

    setDraftProfiles((current) =>
      current.map((profile, currentIndex) =>
        currentIndex === index
          ? {
              ...profile,
              codexHome: selected,
            }
          : profile,
      ),
    );
  }

  function updateDraft(index: number, patch: Partial<DraftProfile>) {
    const currentProfileId = draftProfiles[index]?.id;
    if (currentProfileId === draftActiveProfileId && typeof patch.id === "string") {
      setDraftActiveProfileId(patch.id);
    }

    setDraftProfiles((current) =>
      current.map((profile, currentIndex) =>
        currentIndex === index
          ? {
              ...profile,
              ...patch,
            }
          : profile,
      ),
    );
  }

  function removeDraft(index: number) {
    setDraftProfiles((current) => {
      const target = current[index];
      const next = current.filter((_, currentIndex) => currentIndex !== index);
      if (target?.id === draftActiveProfileId) {
        setDraftActiveProfileId(next[0]?.id ?? "");
      }
      return next;
    });
  }

  function addDraftProfile() {
    setDraftProfiles((current) => {
      const id = nextProfileId(current);
      const next = [
        ...current,
        {
          id,
          name: `Profile ${current.length + 1}`,
          codexHome: "",
          isDefault: false,
        },
      ];
      if (!draftActiveProfileId) {
        setDraftActiveProfileId(id);
      }
      return next;
    });
  }

  async function handleSave() {
    if (!canSave || saving) {
      return;
    }

    setSaving(true);
    try {
      await saveProfiles(
        draftProfiles.map((profile) => normalizeDraftProfile(profile)),
        draftActiveProfileId,
      );
      handleClose();
    } finally {
      setSaving(false);
    }
  }

  return createPortal(
    <div
      className="confirm-dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          handleClose();
        }
      }}
    >
      <div className="ws-modal" style={{ width: "min(760px, calc(100vw - 40px))" }}>
        <div className="ws-header">
          <div className="ws-header-icon">
            <UserCircle size={18} />
          </div>
          <div className="ws-header-text">
            <h2 className="ws-header-title">Codex profiles</h2>
            <p style={{ margin: "3px 0 0", fontSize: 11.5, color: "var(--text-3)" }}>
              Configure multiple `CODEX_HOME` directories and switch accounts safely.
            </p>
          </div>
          <button type="button" className="ws-close" onClick={handleClose} aria-label="Close">
            <X size={16} />
          </button>
        </div>

        <div className="ws-divider" />

        <div
          className="ws-body"
          style={{ display: "flex", flexDirection: "column", gap: 12, paddingTop: 12 }}
        >
          {draftProfiles.map((profile, index) => {
            const isActive = draftActiveProfileId === profile.id;
            return (
              <div
                key={`${profile.id}:${index}`}
                style={{
                  display: "grid",
                  gap: 10,
                  padding: 14,
                  borderRadius: 14,
                  background: "var(--bg-2)",
                  border: `1px solid ${isActive ? "rgba(139, 92, 246, 0.28)" : "var(--border)"}`,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 12,
                  }}
                >
                  <label
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 8,
                      cursor: "pointer",
                      fontSize: 12,
                      fontWeight: 600,
                    }}
                  >
                    <input
                      type="radio"
                      name="codex-profile-active"
                      checked={isActive}
                      onChange={() => setDraftActiveProfileId(profile.id)}
                    />
                    Active profile
                    {isActive ? <CheckCircle2 size={14} style={{ color: "var(--accent)" }} /> : null}
                  </label>
                  <button
                    type="button"
                    className="chat-toolbar-btn"
                    onClick={() => removeDraft(index)}
                    disabled={profile.isDefault}
                    title={profile.isDefault ? "The default profile cannot be removed." : "Remove profile"}
                    style={{ opacity: profile.isDefault ? 0.45 : 1 }}
                  >
                    <X size={12} />
                    Remove
                  </button>
                </div>

                <div
                  style={{
                    display: "grid",
                    gap: 10,
                    gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)",
                  }}
                >
                  <label className="codex-config-field">
                    <span className="codex-config-note">Display name</span>
                    <input
                      className="codex-config-select"
                      value={profile.name}
                      onChange={(event) => updateDraft(index, { name: event.target.value })}
                    />
                  </label>
                  <label className="codex-config-field">
                    <span className="codex-config-note">Profile id</span>
                    <input
                      className="codex-config-select"
                      value={profile.id}
                      onChange={(event) => updateDraft(index, { id: event.target.value })}
                    />
                  </label>
                </div>

                <label className="codex-config-field">
                  <span className="codex-config-note">CODEX_HOME</span>
                  <div
                    style={{
                      display: "grid",
                      gap: 8,
                      gridTemplateColumns: "minmax(0, 1fr) auto",
                    }}
                  >
                    <input
                      className="codex-config-select"
                      value={profile.codexHome}
                      onChange={(event) => updateDraft(index, { codexHome: event.target.value })}
                      placeholder="/home/mehdi/.codex-work"
                    />
                    <button
                      type="button"
                      className="chat-toolbar-btn"
                      onClick={() => void browseProfileHome(index)}
                    >
                      <FolderOpen size={12} />
                      Browse
                    </button>
                  </div>
                </label>
              </div>
            );
          })}

          <button
            type="button"
            className="chat-toolbar-btn"
            onClick={addDraftProfile}
            style={{ alignSelf: "flex-start" }}
          >
            <Plus size={12} />
            Add profile
          </button>

          {error ? (
            <div className="codex-config-error" style={{ marginTop: 0 }}>
              {error}
            </div>
          ) : null}
        </div>

        <div className="ws-divider" />

        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            gap: 12,
            padding: "14px 20px 18px",
          }}
        >
          <div style={{ fontSize: 11, color: "var(--text-3)" }}>
            Switching is blocked while a Codex turn is still active.
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button type="button" className="chat-toolbar-btn" onClick={handleClose}>
              Cancel
            </button>
            <button
              type="button"
              className="chat-toolbar-btn chat-toolbar-btn-active"
              onClick={() => void handleSave()}
              disabled={!canSave || saving || loading}
            >
              {saving || loading ? "Saving..." : "Save profiles"}
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
