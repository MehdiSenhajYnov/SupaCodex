import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import {
  Archive,
  Loader2,
  MoreHorizontal,
  Settings2,
} from "lucide-react";
import {
  normalizeClientRectForFixedPosition,
  readFixedViewportSize,
} from "../shared/anchoredPopoverPosition";
import type { Workspace } from "../../types";

interface WorkspaceMoreMenuProps {
  workspace?: Workspace | null;
  onResolveWorkspace?: () => Promise<Workspace | null>;
  onOpenSettings: (workspace: Workspace) => void;
  onArchive: (workspace: Workspace) => void;
}

export function WorkspaceMoreMenu({
  workspace,
  onResolveWorkspace,
  onOpenSettings,
  onArchive,
}: WorkspaceMoreMenuProps) {
  const { t } = useTranslation("workspace");
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPos, setMenuPos] = useState({ top: 0, left: 0 });
  const [resolvedWorkspace, setResolvedWorkspace] = useState<Workspace | null>(workspace ?? null);
  const [resolvingWorkspace, setResolvingWorkspace] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const closeMenu = useCallback(() => setMenuOpen(false), []);
  const currentWorkspace = workspace ?? resolvedWorkspace;

  useEffect(() => {
    setResolvedWorkspace(workspace ?? null);
  }, [workspace]);

  useEffect(() => {
    if (!menuOpen) return;
    function onPointerDown(e: PointerEvent) {
      const target = e.target as Node;
      if (
        triggerRef.current?.contains(target) ||
        menuRef.current?.contains(target)
      )
        return;
      closeMenu();
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") closeMenu();
    }
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [menuOpen, closeMenu]);

  async function ensureWorkspace(): Promise<Workspace | null> {
    if (currentWorkspace) {
      return currentWorkspace;
    }
    if (!onResolveWorkspace || resolvingWorkspace) {
      return null;
    }

    setResolvingWorkspace(true);
    try {
      const nextWorkspace = await onResolveWorkspace();
      if (nextWorkspace) {
        setResolvedWorkspace(nextWorkspace);
      }
      return nextWorkspace;
    } finally {
      setResolvingWorkspace(false);
    }
  }

  async function handleTriggerClick(e: React.MouseEvent) {
    e.stopPropagation();
    if (menuOpen) {
      closeMenu();
      return;
    }

    const nextWorkspace = await ensureWorkspace();
    if (!nextWorkspace) {
      return;
    }

    const rawRect = triggerRef.current?.getBoundingClientRect();
    if (rawRect) {
      const rect = normalizeClientRectForFixedPosition(rawRect);
      const viewport = readFixedViewportSize();
      setMenuPos({
        top: rect.bottom + 4,
        left: Math.max(8, Math.min(rect.right - 180, viewport.width - 188)),
      });
    }
    setMenuOpen(true);
  }

  function handleItem(action: (workspace: Workspace) => void) {
    const nextWorkspace = workspace ?? resolvedWorkspace;
    if (!nextWorkspace) {
      return;
    }
    closeMenu();
    action(nextWorkspace);
  }

  return (
    <>
      <button
        type="button"
        ref={triggerRef}
        aria-label={t("more.options")}
        title={t("more.options")}
        className={`sb-project-more${menuOpen ? " sb-project-more-active" : ""}`}
        disabled={resolvingWorkspace}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(event) => {
          void handleTriggerClick(event);
        }}
      >
        {resolvingWorkspace ? <Loader2 size={12} className="git-spin" /> : <MoreHorizontal size={12} />}
      </button>

      {menuOpen &&
        createPortal(
          <div
            ref={menuRef}
            className="git-action-menu"
            style={{
              position: "fixed",
              top: menuPos.top,
              left: menuPos.left,
              minWidth: 180,
            }}
          >
            <button
              type="button"
              className="git-action-menu-item"
              onClick={() => handleItem(onOpenSettings)}
            >
              <Settings2 size={13} />
              {t("more.settings")}
            </button>
            <div style={{ height: 1, margin: "4px 0", background: "var(--border)" }} />
            <button
              type="button"
              className="git-action-menu-item git-action-menu-item-danger"
              onClick={() => handleItem(onArchive)}
            >
              <Archive size={13} />
              {t("more.archive")}
            </button>
          </div>,
          document.body,
        )}
    </>
  );
}
