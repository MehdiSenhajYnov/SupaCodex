import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Keyboard, RotateCcw, Slash, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  SHORTCUT_ACTION_DEFINITIONS,
  type ShortcutActionId,
  type ShortcutGroupId,
  eventToShortcutBinding,
  formatShortcutBinding,
  getEffectiveShortcutBinding,
} from "../../lib/shortcutBindings";
import { useShortcutStore } from "../../stores/shortcutStore";

const SHORTCUT_GROUP_ORDER: ShortcutGroupId[] = ["navigation", "panels", "search"];

export function ShortcutSettingsModal() {
  const { t } = useTranslation("app");
  const open = useShortcutStore((state) => state.modalOpen);
  const overrides = useShortcutStore((state) => state.overrides);
  const close = useShortcutStore((state) => state.closeModal);
  const setActionShortcut = useShortcutStore((state) => state.setActionShortcut);
  const clearActionShortcut = useShortcutStore((state) => state.clearActionShortcut);
  const resetActionShortcut = useShortcutStore((state) => state.resetActionShortcut);
  const resetAll = useShortcutStore((state) => state.resetAll);
  const [recordingActionId, setRecordingActionId] = useState<ShortcutActionId | null>(null);
  const [resolvedConflictIds, setResolvedConflictIds] = useState<ShortcutActionId[]>([]);

  const handleClose = useCallback(() => {
    setRecordingActionId(null);
    setResolvedConflictIds([]);
    close();
  }, [close]);

  const groupedActions = useMemo(
    () =>
      SHORTCUT_GROUP_ORDER.map((groupId) => ({
        id: groupId,
        actions: SHORTCUT_ACTION_DEFINITIONS.filter((definition) => definition.group === groupId),
      })),
    [],
  );

  useEffect(() => {
    if (!open) {
      return;
    }

    function onKeyDown(event: KeyboardEvent) {
      if (recordingActionId) {
        event.preventDefault();
        event.stopPropagation();

        if (event.key === "Escape") {
          setRecordingActionId(null);
          return;
        }

        const binding = eventToShortcutBinding(event);
        if (!binding) {
          return;
        }

        const clearedConflicts = setActionShortcut(recordingActionId, binding);
        setResolvedConflictIds(clearedConflicts);
        setRecordingActionId(null);
        return;
      }

      if (event.key === "Escape") {
        event.stopPropagation();
        handleClose();
      }
    }

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [handleClose, open, recordingActionId, setActionShortcut]);

  if (!open) {
    return null;
  }

  const resolvedConflictSummary =
    resolvedConflictIds.length > 0
      ? t("shortcutSettings.conflictsResolved", {
          actions: resolvedConflictIds
            .map((actionId) => t(`shortcutSettings.actions.${actionId}.label`))
            .join(", "),
        })
      : "";

  return createPortal(
    <div
      className="confirm-dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          handleClose();
        }
      }}
    >
      <div
        className="ws-modal shortcut-settings-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcut-settings-title"
      >
        <div className="ws-header">
          <div className="ws-header-icon">
            <Keyboard size={18} />
          </div>
          <div className="ws-header-text">
            <h2 id="shortcut-settings-title" className="ws-header-title">
              {t("shortcutSettings.title")}
            </h2>
            <p className="shortcut-settings-header-copy">
              {t("shortcutSettings.description")}
            </p>
          </div>
          <button type="button" className="ws-close" onClick={handleClose} aria-label={t("shortcutSettings.close")}>
            <X size={16} />
          </button>
        </div>

        <div className="ws-divider" />

        <div className="ws-body shortcut-settings-body">
          <div className="shortcut-settings-intro-card">
            <div className="shortcut-settings-intro-title">
              {recordingActionId
                ? t("shortcutSettings.recordingTitle", {
                    action: t(`shortcutSettings.actions.${recordingActionId}.label`),
                  })
                : t("shortcutSettings.introTitle")}
            </div>
            <div className="shortcut-settings-intro-copy">
              {recordingActionId
                ? t("shortcutSettings.recordingHint")
                : t("shortcutSettings.introDescription")}
            </div>
            {resolvedConflictSummary ? (
              <div className="shortcut-settings-conflict-banner">
                {resolvedConflictSummary}
              </div>
            ) : null}
          </div>

          {groupedActions.map((group) => (
            <section key={group.id} className="shortcut-settings-section">
              <div className="shortcut-settings-section-header">
                <div className="shortcut-settings-section-title">
                  {t(`shortcutSettings.groups.${group.id}.label`)}
                </div>
                <div className="shortcut-settings-section-copy">
                  {t(`shortcutSettings.groups.${group.id}.description`)}
                </div>
              </div>

              <div className="shortcut-settings-list">
                {group.actions.map((action) => {
                  const effectiveBinding = getEffectiveShortcutBinding(action.id, overrides);
                  const formattedBinding = formatShortcutBinding(effectiveBinding);
                  const isRecording = recordingActionId === action.id;
                  const isOverridden = overrides[action.id] !== undefined;
                  const isDisabled = effectiveBinding == null;
                  const statusKey = isDisabled
                    ? "disabled"
                    : isOverridden
                      ? "custom"
                      : "default";

                  return (
                    <div key={action.id} className="shortcut-settings-row">
                      <div className="shortcut-settings-row-main">
                        <div className="shortcut-settings-row-heading">
                          <div className="shortcut-settings-row-title">
                            {t(`shortcutSettings.actions.${action.id}.label`)}
                          </div>
                          <span
                            className={`shortcut-settings-status-badge shortcut-settings-status-${statusKey}`}
                          >
                            {t(`shortcutSettings.status.${statusKey}`)}
                          </span>
                        </div>
                        <div className="shortcut-settings-row-copy">
                          {t(`shortcutSettings.actions.${action.id}.description`)}
                        </div>
                      </div>

                      <div className="shortcut-settings-row-controls">
                        <button
                          type="button"
                          className={`shortcut-settings-binding-btn${isRecording ? " shortcut-settings-binding-btn-recording" : ""}${isDisabled ? " shortcut-settings-binding-btn-empty" : ""}`}
                          onClick={() => {
                            setResolvedConflictIds([]);
                            setRecordingActionId(action.id);
                          }}
                          aria-pressed={isRecording}
                        >
                          {isRecording
                            ? t("shortcutSettings.recording")
                            : (formattedBinding || t("shortcutSettings.unassigned"))}
                        </button>
                        <button
                          type="button"
                          className="shortcut-settings-inline-btn"
                          disabled={!isOverridden}
                          onClick={() => {
                            setResolvedConflictIds([]);
                            setRecordingActionId(null);
                            resetActionShortcut(action.id);
                          }}
                        >
                          <RotateCcw size={12} />
                          {t("shortcutSettings.resetAction")}
                        </button>
                        <button
                          type="button"
                          className="shortcut-settings-inline-btn"
                          disabled={isDisabled}
                          onClick={() => {
                            setResolvedConflictIds([]);
                            setRecordingActionId(null);
                            clearActionShortcut(action.id);
                          }}
                        >
                          <Slash size={12} />
                          {t("shortcutSettings.clearAction")}
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        </div>

        <div className="ws-divider" />

        <div className="shortcut-settings-footer">
          <div className="shortcut-settings-footer-copy">
            {t("shortcutSettings.footer")}
          </div>
          <button
            type="button"
            className="chat-toolbar-btn"
            onClick={() => {
              setResolvedConflictIds([]);
              setRecordingActionId(null);
              resetAll();
            }}
          >
            <RotateCcw size={12} />
            {t("shortcutSettings.resetAll")}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

