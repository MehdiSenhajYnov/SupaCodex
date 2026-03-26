import { useCallback, useEffect } from "react";
import { createPortal } from "react-dom";
import { Monitor, RotateCcw, X } from "lucide-react";
import { useAppearanceStore } from "../../stores/appearanceStore";

function SliderRow({
  label,
  value,
  min,
  max,
  onChange,
  formatValue,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  formatValue?: (value: number) => string;
}) {
  return (
    <label
      style={{
        display: "grid",
        gap: 8,
        padding: 12,
        borderRadius: 12,
        background: "var(--bg-2)",
        border: "1px solid var(--border)",
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
        <span style={{ fontSize: 12, fontWeight: 600 }}>{label}</span>
        <span style={{ fontSize: 11, color: "var(--text-3)" }}>
          {formatValue ? formatValue(value) : `${value}px`}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        value={value}
        onChange={(event) => onChange(Number.parseInt(event.target.value, 10))}
      />
    </label>
  );
}

function ToggleRow({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div
      className="ntf-row"
      style={{
        borderRadius: 12,
        background: "var(--bg-2)",
        border: "1px solid var(--border)",
        paddingInline: 12,
      }}
    >
      <div className="ntf-row-left">
        <div>
          <div className="ntf-row-title">{label}</div>
          <div className="ntf-row-desc">{description}</div>
        </div>
      </div>
      <label className="ws-toggle">
        <input
          type="checkbox"
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="ws-toggle-track" />
        <span className="ws-toggle-thumb" />
      </label>
    </div>
  );
}

export function AppearanceSettingsModal() {
  const open = useAppearanceStore((s) => s.modalOpen);
  const close = useAppearanceStore((s) => s.closeModal);
  const patchSettings = useAppearanceStore((s) => s.patchSettings);
  const reset = useAppearanceStore((s) => s.reset);
  const interfaceZoom = useAppearanceStore((s) => s.interfaceZoom);
  const windowRadius = useAppearanceStore((s) => s.windowRadius);
  const windowGap = useAppearanceStore((s) => s.windowGap);
  const surfaceBlur = useAppearanceStore((s) => s.surfaceBlur);
  const transparentSidebar = useAppearanceStore((s) => s.transparentSidebar);
  const transparentContent = useAppearanceStore((s) => s.transparentContent);
  const transparentTerminal = useAppearanceStore((s) => s.transparentTerminal);

  const handleClose = useCallback(() => close(), [close]);

  useEffect(() => {
    if (!open) {
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
  }, [handleClose, open]);

  if (!open) {
    return null;
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
      <div className="ws-modal" style={{ width: "min(620px, calc(100vw - 40px))" }}>
        <div className="ws-header">
          <div className="ws-header-icon">
            <Monitor size={18} />
          </div>
          <div className="ws-header-text">
            <h2 className="ws-header-title">Appearance</h2>
            <p style={{ margin: "3px 0 0", fontSize: 11.5, color: "var(--text-3)" }}>
              Tune interface zoom, the real window corner clip, the optional surface inset, and transparent panels for Linux blur setups.
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
          <SliderRow
            label="Interface zoom"
            value={interfaceZoom}
            min={80}
            max={160}
            onChange={(value) => patchSettings({ interfaceZoom: value })}
            formatValue={(value) => `${value}%`}
          />
          <SliderRow
            label="Window corner radius"
            value={windowRadius}
            min={0}
            max={36}
            onChange={(value) => patchSettings({ windowRadius: value })}
          />
          <SliderRow
            label="Surface inset"
            value={windowGap}
            min={0}
            max={24}
            onChange={(value) => patchSettings({ windowGap: value })}
          />
          <SliderRow
            label="Surface blur"
            value={surfaceBlur}
            min={0}
            max={40}
            onChange={(value) => patchSettings({ surfaceBlur: value })}
          />

          <ToggleRow
            label="Transparent sidebar"
            description="Makes the sidebar and custom titlebar translucent."
            checked={transparentSidebar}
            onChange={(checked) => patchSettings({ transparentSidebar: checked })}
          />
          <ToggleRow
            label="Transparent content panels"
            description="Makes the chat and git content surfaces translucent."
            checked={transparentContent}
            onChange={(checked) => patchSettings({ transparentContent: checked })}
          />
          <ToggleRow
            label="Transparent terminal"
            description="Makes terminal chrome and terminal background translucent."
            checked={transparentTerminal}
            onChange={(checked) => patchSettings({ transparentTerminal: checked })}
          />
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
          <button
            type="button"
            className="chat-toolbar-btn"
            onClick={reset}
            title="Reset appearance settings"
          >
            <RotateCcw size={12} />
            Reset
          </button>
          <div style={{ fontSize: 11, color: "var(--text-3)", textAlign: "right" }}>
            Maximized and fullscreen windows automatically disable the window clip radius and surface inset.
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
