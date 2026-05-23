import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  Plus,
  FolderGit2,
  MessageSquare,
  ChevronDown,
  ChevronRight,
  Archive,
  MoreHorizontal,
  RotateCcw,
  Settings,
  Terminal,
  Check,
  Rocket,
  RefreshCw,
  PillBottle,
  BellRing,
  Globe,
  Monitor,
  Keyboard,
  Pin,
  Trash2,
  UserCircle,
  X,
} from "lucide-react";
import { useChatStore } from "../../stores/chatStore";
import {
  useCodexProfileStore,
} from "../../stores/codexProfileStore";
import { useEngineStore } from "../../stores/engineStore";
import { useThreadStore } from "../../stores/threadStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useUiStore } from "../../stores/uiStore";
import { useOnboardingStore } from "../../stores/onboardingStore";
import { useUpdateStore } from "../../stores/updateStore";
import { canToggleKeepAwake, useKeepAwakeStore } from "../../stores/keepAwakeStore";
import { useAppearanceStore } from "../../stores/appearanceStore";
import { useTerminalNotificationSettingsStore } from "../../stores/terminalNotificationSettingsStore";
import { useProjectTabsStore } from "../../stores/projectTabsStore";
import { useShortcutStore } from "../../stores/shortcutStore";
import { toast } from "../../stores/toastStore";
import { ipc } from "../../lib/ipc";
import { formatRelativeTime } from "../../lib/formatters";
import { resolvePreferredOnboardingChatSelection } from "../../lib/onboarding";
import {
  emitTerminalAcceleratedRenderingChanged,
  getTerminalAcceleratedRenderingPreferenceVersion,
} from "../../lib/terminalRenderingSettings";
import {
  normalizeAppLocale,
  SUPPORTED_APP_LOCALES,
  type AppLocale,
} from "../../lib/locale";
import { handleDragMouseDown } from "../../lib/windowDrag";
import { UpdateDialog } from "../onboarding/UpdateDialog";
import { ConfirmDialog } from "../shared/ConfirmDialog";
import { WorkspaceMoreMenu } from "../workspace/WorkspaceMoreMenu";
import {
  normalizeClientRectForFixedPosition,
  readFixedViewportSize,
} from "../shared/anchoredPopoverPosition";
import {
  buildSidebarProjectEntries,
  isProjectPinned,
  isThreadPinned,
  type ProjectGroup,
  type SidebarPinnedProjects,
  type UnifiedProjectGroup,
} from "./sidebarProjectEntries";
import type { CodexDetectedProject, Thread, Workspace } from "../../types";

interface SidebarConversationRowProps {
  label: string;
  timeLabel?: string;
  active?: boolean;
  disabled?: boolean;
  animationDelayMs: number;
  opacity?: number;
  className?: string;
  onClick: () => void;
  onMiddleClick?: () => void;
  leadingAction?: SidebarRowAction;
  metaAction?: SidebarRowAction;
  trailing?: ReactNode;
}

interface SidebarProjectRowProps {
  label: string;
  count?: number;
  active?: boolean;
  collapsed?: boolean;
  disabled?: boolean;
  icon: ReactNode;
  wrapperClassName?: string;
  leadingAction?: SidebarRowAction;
  metaAction?: SidebarRowAction;
  onClick: () => void;
  onMiddleClick?: () => void;
  trailing?: ReactNode;
}

interface SidebarSectionHeaderProps {
  label: string;
  count?: number;
  expanded: boolean;
  controlsId: string;
  toggleTitle: string;
  onToggle: (event: ReactMouseEvent<HTMLButtonElement>) => void;
  action?: ReactNode;
}

const MAX_VISIBLE_THREADS = 8;
const LEGACY_SCAN_DEPTH_STORAGE_KEY = "supacodex.workspace.scanDepth";
const SIDEBAR_PERSISTENCE_STORAGE_KEY = "supacodex:sidebar:v2";
const LEGACY_SCAN_DEPTH_MIN = 0;
const LEGACY_SCAN_DEPTH_MAX = 12;
const DEFAULT_CODEX_MODEL = "gpt-5.3-codex";

interface SidebarPersistentState {
  projectLibraryCollapsed: boolean;
  pinnedProjects: SidebarPinnedProjects;
}

interface SidebarRowAction {
  title: string;
  icon: ReactNode;
  active?: boolean;
  onClick: () => void;
}

interface SidebarMenuAction {
  key: string;
  label: string;
  icon: ReactNode;
  danger?: boolean;
  onSelect: () => void;
}

function SidebarProjectRow({
  label,
  count,
  active = false,
  collapsed = false,
  disabled = false,
  icon,
  wrapperClassName,
  leadingAction,
  metaAction,
  onClick,
  onMiddleClick,
  trailing,
}: SidebarProjectRowProps) {
  const hasCount = typeof count === "number" && count > 0;

  return (
    <div
      className={`sb-project-row ${active ? "sb-project-row-active" : ""}${leadingAction?.active ? " sb-project-row-pinned" : ""}${disabled ? " sb-project-row-disabled" : ""}${wrapperClassName ? ` ${wrapperClassName}` : ""}`}
    >
      <div className="sb-project-leading-slot">
        {leadingAction ? (
          <button
            type="button"
            aria-label={leadingAction.title}
            title={leadingAction.title}
            className={`sb-project-icon-action sb-project-pin${leadingAction.active ? " sb-project-icon-action-active" : ""}`}
            disabled={disabled}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              leadingAction.onClick();
            }}
          >
            {leadingAction.icon}
          </button>
        ) : (
          <span className="sb-project-action-placeholder" aria-hidden="true" />
        )}
      </div>
      <button
        type="button"
        className="sb-project"
        disabled={disabled}
        onClick={onClick}
        onMouseDown={(event) => {
          if (event.button === 1 && onMiddleClick) {
            event.preventDefault();
          }
        }}
        onAuxClick={(event) => {
          if (event.button === 1 && onMiddleClick) {
            event.preventDefault();
            onMiddleClick();
          }
        }}
      >
        <span className="sb-project-prefix" aria-hidden="true">
          {collapsed ? (
            <ChevronRight size={12} className="sb-project-caret" />
          ) : (
            <ChevronDown size={12} className="sb-project-caret" />
          )}
          <span className="sb-project-folder-icon">{icon}</span>
        </span>
        <span className="sb-project-name">{label}</span>
      </button>
      <div className="sb-project-row-meta">
        {hasCount ? (
          <span className={`sb-project-count${metaAction ? " sb-project-count-swap" : ""}`}>
            {count}
          </span>
        ) : (
          <span className="sb-project-count-placeholder" aria-hidden="true" />
        )}
        {metaAction ? (
          <button
            type="button"
            aria-label={metaAction.title}
            title={metaAction.title}
            className={`sb-project-meta-action${metaAction.active ? " sb-project-meta-action-active" : ""}`}
            disabled={disabled}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              metaAction.onClick();
            }}
          >
            {metaAction.icon}
          </button>
        ) : null}
      </div>
      <div className="sb-project-row-action">
        {trailing ?? <span className="sb-project-row-action-placeholder" aria-hidden="true" />}
      </div>
    </div>
  );
}

function SidebarConversationRow({
  label,
  timeLabel,
  active = false,
  disabled = false,
  animationDelayMs,
  opacity,
  className,
  onClick,
  onMiddleClick,
  leadingAction,
  metaAction,
  trailing,
}: SidebarConversationRowProps) {
  return (
    <div
      className={`sb-thread sb-thread-animate ${active ? "sb-thread-active" : ""}${leadingAction?.active ? " sb-thread-pinned" : ""}${disabled ? " sb-thread-disabled" : ""}${className ? ` ${className}` : ""}`}
      style={{
        animationDelay: `${animationDelayMs}ms`,
        ...(opacity === undefined ? {} : { opacity }),
      }}
    >
      <div className="sb-thread-leading-slot">
        {leadingAction ? (
          <button
            type="button"
            aria-label={leadingAction.title}
            title={leadingAction.title}
            className={`sb-thread-icon-action sb-thread-pin${leadingAction.active ? " sb-thread-icon-action-active" : ""}`}
            disabled={disabled}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              leadingAction.onClick();
            }}
          >
            {leadingAction.icon}
          </button>
        ) : (
          <span className="sb-thread-action-placeholder" aria-hidden="true" />
        )}
      </div>
      <button
        type="button"
        className="sb-thread-trigger"
        disabled={disabled}
        onClick={onClick}
        onMouseDown={(event) => {
          if (event.button === 1 && onMiddleClick) {
            event.preventDefault();
          }
        }}
        onAuxClick={(event) => {
          if (event.button === 1 && onMiddleClick) {
            event.preventDefault();
            onMiddleClick();
          }
        }}
      >
        <span className="sb-thread-title">{label}</span>
      </button>
      <div className="sb-thread-row-meta">
        {metaAction ? (
          <button
            type="button"
            aria-label={metaAction.title}
            title={metaAction.title}
            className={`sb-thread-meta-action sb-thread-archive${metaAction.active ? " sb-thread-meta-action-active" : ""}`}
            disabled={disabled}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              metaAction.onClick();
            }}
          >
            {metaAction.icon}
          </button>
        ) : (
          <span className="sb-thread-meta-action-placeholder" aria-hidden="true" />
        )}
      </div>
      <div className="sb-thread-row-action">
        <span className={`sb-thread-time-slot${timeLabel ? "" : " sb-thread-time-slot-empty"}${trailing ? " sb-thread-time-slot-swap" : ""}`}>
          {timeLabel ?? ""}
        </span>
        {trailing ?? <span className="sb-thread-row-action-placeholder" aria-hidden="true" />}
      </div>
    </div>
  );
}

function SidebarConversationMoreMenu({
  actions,
  disabled = false,
}: {
  actions: SidebarMenuAction[];
  disabled?: boolean;
}) {
  const { t } = useTranslation("app");
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPos, setMenuPos] = useState({ top: 0, left: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const closeMenu = useCallback(() => setMenuOpen(false), []);

  useEffect(() => {
    if (!menuOpen) {
      return;
    }

    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) {
        return;
      }
      closeMenu();
    }

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        closeMenu();
      }
    }

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [closeMenu, menuOpen]);

  if (actions.length === 0) {
    return <span className="sb-thread-row-action-placeholder" aria-hidden="true" />;
  }

  return (
    <>
      <button
        type="button"
        ref={triggerRef}
        aria-label={t("sidebar.conversationOptions")}
        title={t("sidebar.conversationOptions")}
        className={`sb-project-more sb-thread-more${menuOpen ? " sb-project-more-active sb-thread-more-active" : ""}`}
        disabled={disabled}
        onMouseDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation();
          if (menuOpen) {
            closeMenu();
            return;
          }

          const rawRect = triggerRef.current?.getBoundingClientRect();
          if (!rawRect) {
            return;
          }
          const rect = normalizeClientRectForFixedPosition(rawRect);
          const viewport = readFixedViewportSize();

          setMenuPos({
            top: rect.bottom + 4,
            left: Math.max(8, Math.min(rect.right - 196, viewport.width - 204)),
          });
          setMenuOpen(true);
        }}
      >
        <MoreHorizontal size={12} />
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
              minWidth: 196,
            }}
          >
            {actions.map((action) => (
              <button
                key={action.key}
                type="button"
                className={`git-action-menu-item${action.danger ? " git-action-menu-item-danger" : ""}`}
                onClick={() => {
                  closeMenu();
                  action.onSelect();
                }}
              >
                {action.icon}
                {action.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}

function SidebarSectionHeader({
  label,
  count,
  expanded,
  controlsId,
  toggleTitle,
  onToggle,
  action,
}: SidebarSectionHeaderProps) {
  const hasCount = typeof count === "number" && count > 0;

  return (
    <div className="sb-section-label">
      <button
        type="button"
        className="sb-section-toggle"
        aria-expanded={expanded}
        aria-controls={controlsId}
        onClick={(event) => onToggle(event)}
        title={toggleTitle}
      >
        {expanded ? (
          <ChevronDown size={11} style={{ flexShrink: 0, opacity: 0.6 }} />
        ) : (
          <ChevronRight size={11} style={{ flexShrink: 0, opacity: 0.6 }} />
        )}
        <span>{label}</span>
      </button>
      <div className="sb-section-meta">
        {hasCount ? (
          <span className="sb-project-count">{count}</span>
        ) : (
          <span className="sb-project-count-placeholder" aria-hidden="true" />
        )}
      </div>
      <div className="sb-section-action">
        {action ?? <span className="sb-section-action-placeholder" aria-hidden="true" />}
      </div>
    </div>
  );
}

function readLegacyDefaultScanDepth(): number | undefined {
  const stored = window.localStorage.getItem(LEGACY_SCAN_DEPTH_STORAGE_KEY);
  if (!stored) return undefined;
  const parsed = Number.parseInt(stored, 10);
  if (!Number.isFinite(parsed)) return undefined;
  if (parsed < LEGACY_SCAN_DEPTH_MIN || parsed > LEGACY_SCAN_DEPTH_MAX) {
    return undefined;
  }
  return parsed;
}

function normalizePinnedProjects(
  input: unknown,
): SidebarPinnedProjects {
  if (!input || typeof input !== "object") {
    return {};
  }

  return Object.entries(input as Record<string, unknown>).reduce<SidebarPinnedProjects>(
    (acc, [projectKey, pinnedAt]) => {
      if (typeof pinnedAt !== "string") {
        return acc;
      }

      const normalizedKey = projectKey.trim();
      if (!normalizedKey) {
        return acc;
      }

      const parsed = Date.parse(pinnedAt);
      if (!Number.isFinite(parsed)) {
        return acc;
      }

      acc[normalizedKey] = new Date(parsed).toISOString();
      return acc;
    },
    {},
  );
}

function readSidebarPersistentState(): SidebarPersistentState {
  try {
    const raw = window.localStorage.getItem(SIDEBAR_PERSISTENCE_STORAGE_KEY);
    if (!raw) {
      return {
        projectLibraryCollapsed: false,
        pinnedProjects: {},
      };
    }

    const parsed = JSON.parse(raw) as Partial<SidebarPersistentState> | null;
    if (!parsed || typeof parsed !== "object") {
      throw new Error("invalid sidebar persistent state");
    }

    return {
      projectLibraryCollapsed: parsed.projectLibraryCollapsed === true,
      pinnedProjects: normalizePinnedProjects(parsed.pinnedProjects),
    };
  } catch {
    return {
      projectLibraryCollapsed: false,
      pinnedProjects: {},
    };
  }
}

function writeSidebarPersistentState(state: SidebarPersistentState): void {
  try {
    window.localStorage.setItem(SIDEBAR_PERSISTENCE_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Ignore persistence failures.
  }
}

function SidebarContent() {
  const { t, i18n } = useTranslation(["app", "common", "chat"]);
  const {
    workspaces,
    archivedWorkspaces,
    activeWorkspaceId,
    setActiveRepo,
    openWorkspace,
    removeWorkspace,
    restoreWorkspace,
    refreshArchivedWorkspaces,
    error,
  } = useWorkspaceStore();
  const {
    threads,
    archivedThreadsByWorkspace,
    activeThreadId,
    setActiveThread,
    removeThread,
    deleteThread,
    restoreThread,
    createThread,
    refreshArchivedThreads,
    attachCodexRemoteThread,
    setThreadPinned,
  } = useThreadStore();
  const openOnboarding = useOnboardingStore((state) => state.openOnboarding);
  const selectedChatEngines = useOnboardingStore((state) => state.selectedChatEngines);
  const activeView = useUiStore((state) => state.activeView);
  const setActiveView = useUiStore((state) => state.setActiveView);
  const openWorkspaceSettings = useUiStore((state) => state.openWorkspaceSettings);
  const bindChatThread = useChatStore((s) => s.setActiveThread);
  const engines = useEngineStore((s) => s.engines);
  const updateStatus = useUpdateStore((s) => s.status);
  const updateSnoozed = useUpdateStore((s) => s.snoozed);
  const keepAwakeState = useKeepAwakeStore((s) => s.state);
  const keepAwakeLoading = useKeepAwakeStore((s) => s.loading);
  const toggleKeepAwake = useKeepAwakeStore((s) => s.toggle);
  const openPowerSettings = useKeepAwakeStore((s) => s.openPowerSettings);
  const openAppearanceSettings = useAppearanceStore((s) => s.openModal);
  const openShortcutSettings = useShortcutStore((s) => s.openModal);
  const terminalNotificationSettings = useTerminalNotificationSettingsStore((s) => s.settings);
  const terminalNotificationLoading = useTerminalNotificationSettingsStore((s) => s.loading);
  const terminalNotificationLoadedOnce = useTerminalNotificationSettingsStore((s) => s.loadedOnce);
  const terminalNotificationUpdatingChatEnabled = useTerminalNotificationSettingsStore((s) => s.updatingChatEnabled);
  const terminalNotificationUpdatingTerminalEnabled = useTerminalNotificationSettingsStore((s) => s.updatingTerminalEnabled);
  const toggleTerminalNotifications = useTerminalNotificationSettingsStore((s) => s.toggle);
  const openTerminalNotificationSettings = useTerminalNotificationSettingsStore((s) => s.openModal);
  const detectedCodexProjects = useCodexProfileStore((s) => s.detectedProjects);
  const refreshDetectedCodexProjects = useCodexProfileStore((s) => s.refreshDetectedProjects);
  const ensureActiveCodexProfile = useCodexProfileStore((s) => s.ensureActiveProfile);
  const openCodexProfilesModal = useCodexProfileStore((s) => s.openModal);
  const switchToWorkspaceTabs = useProjectTabsStore((s) => s.switchToWorkspace);
  const switchToThreadTabs = useProjectTabsStore((s) => s.switchToThread);
  const hasUpdate = updateStatus === "available" && !updateSnoozed;
  const keepAwakeAvailable = canToggleKeepAwake(keepAwakeState);
  const preferredOnboardingChatSelection = useMemo(
    () => resolvePreferredOnboardingChatSelection(selectedChatEngines, engines),
    [engines, selectedChatEngines],
  );
  const [sidebarPersistence, setSidebarPersistence] = useState<SidebarPersistentState>(() =>
    readSidebarPersistentState(),
  );

  const workspaceProjects = useMemo<ProjectGroup[]>(
    () =>
      workspaces.map((ws) => ({
        workspace: ws,
        threads: threads.filter((t) => t.workspaceId === ws.id),
      })),
    [workspaces, threads],
  );
  const projectEntries = useMemo<UnifiedProjectGroup[]>(() => {
    const entries = buildSidebarProjectEntries(
      workspaceProjects,
      detectedCodexProjects,
      sidebarPersistence.pinnedProjects,
    );

    return entries.map((entry) =>
      entry.workspace
        ? {
            ...entry,
            name: getWorkspaceLabel(entry.workspace),
          }
        : entry,
    );
  }, [
    detectedCodexProjects,
    sidebarPersistence.pinnedProjects,
    threads,
    workspaceProjects,
  ]);

  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [showAll, setShowAll] = useState<Record<string, boolean>>({});
  const [pendingActionKeys, setPendingActionKeys] = useState<Record<string, true>>({});
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [archiveWorkspacePrompt, setArchiveWorkspacePrompt] = useState<{
    workspace: Workspace;
  } | null>(null);
  const [archiveThreadPrompt, setArchiveThreadPrompt] = useState<{
    thread: Thread;
  } | null>(null);
  const [deleteThreadPrompt, setDeleteThreadPrompt] = useState<{
    thread: Thread;
  } | null>(null);
  const [settingsMenuOpen, setSettingsMenuOpen] = useState(false);
  const [settingsMenuPos, setSettingsMenuPos] = useState({ bottom: 0, left: 0 });
  const [terminalAcceleratedRendering, setTerminalAcceleratedRendering] = useState(true);
  const pendingActionKeysRef = useRef<Set<string>>(new Set());
  const settingsMenuRef = useRef<HTMLDivElement>(null);
  const settingsTriggerRef = useRef<HTMLButtonElement>(null);
  const activeLocale = normalizeAppLocale(i18n.language);

  const closeSettingsMenu = useCallback(() => setSettingsMenuOpen(false), []);
  const isSidebarActionPending = useCallback(
    (actionKey: string) => pendingActionKeys[actionKey] === true,
    [pendingActionKeys],
  );
  const runWithSidebarActionLock = useCallback(
    async <T,>(actionKey: string, action: () => Promise<T>): Promise<T | undefined> => {
      if (pendingActionKeysRef.current.has(actionKey)) {
        return undefined;
      }

      pendingActionKeysRef.current.add(actionKey);
      setPendingActionKeys((prev) => {
        if (prev[actionKey]) {
          return prev;
        }
        return {
          ...prev,
          [actionKey]: true,
        };
      });

      try {
        return await action();
      } finally {
        pendingActionKeysRef.current.delete(actionKey);
        setPendingActionKeys((prev) => {
          if (!prev[actionKey]) {
            return prev;
          }
          const next = { ...prev };
          delete next[actionKey];
          return next;
        });
      }
    },
    [],
  );

  useEffect(() => {
    writeSidebarPersistentState(sidebarPersistence);
  }, [sidebarPersistence]);

  useEffect(() => {
    setCollapsed((prev) => {
      let changed = false;
      const next = { ...prev };
      for (const project of projectEntries) {
        if (Object.prototype.hasOwnProperty.call(next, project.key)) {
          continue;
        }
        next[project.key] = true;
        changed = true;
      }
      return changed ? next : prev;
    });
  }, [projectEntries]);

  useEffect(() => {
    if (!settingsMenuOpen) return;
    function onPointerDown(e: PointerEvent) {
      const target = e.target as Node;
      if (
        settingsMenuRef.current?.contains(target) ||
        settingsTriggerRef.current?.contains(target)
      )
        return;
      closeSettingsMenu();
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") closeSettingsMenu();
    }
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [settingsMenuOpen, closeSettingsMenu]);

  useEffect(() => {
    let cancelled = false;
    const requestVersion = getTerminalAcceleratedRenderingPreferenceVersion();
    ipc
      .getTerminalAcceleratedRendering()
      .then((enabled) => {
        if (
          !cancelled &&
          getTerminalAcceleratedRenderingPreferenceVersion() === requestVersion
        ) {
          setTerminalAcceleratedRendering(enabled);
        }
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, []);

  const archivedThreads = useMemo(
    () =>
      Object.values(archivedThreadsByWorkspace)
        .flat()
        .sort((left, right) =>
          new Date(right.lastActivityAt).getTime() - new Date(left.lastActivityAt).getTime(),
        ),
    [archivedThreadsByWorkspace],
  );
  const projectLibraryCollapsed = sidebarPersistence.projectLibraryCollapsed;
  const projectLibrarySectionId = "sidebar-project-library";

  const toggleCollapse = (projectKey: string) =>
    setCollapsed((prev) => ({ ...prev, [projectKey]: !(prev[projectKey] ?? true) }));

  function getProjectOpenActionKey(project: UnifiedProjectGroup) {
    return `sidebar:open-project:${project.key}`;
  }

  function getCreateThreadActionKey(workspaceId: string) {
    return `sidebar:create-thread:${workspaceId}`;
  }

  function getDetectedConversationActionKey(
    project: UnifiedProjectGroup,
    detectedThread: CodexDetectedProject["threads"][number],
  ) {
    return `sidebar:attach-thread:${project.key}:${detectedThread.profileId}:${detectedThread.engineThreadId}`;
  }

  function getTogglePinnedActionKey(threadId: string) {
    return `sidebar:toggle-pin:${threadId}`;
  }

  function onToggleProjectPinned(project: UnifiedProjectGroup) {
    setSidebarPersistence((prev) => {
      const nextPinnedProjects = { ...prev.pinnedProjects };
      if (isProjectPinned(project.key, prev.pinnedProjects)) {
        delete nextPinnedProjects[project.key];
      } else {
        nextPinnedProjects[project.key] = new Date().toISOString();
      }
      return {
        ...prev,
        pinnedProjects: nextPinnedProjects,
      };
    });
  }

  useEffect(() => {
    void refreshArchivedWorkspaces();
  }, [refreshArchivedWorkspaces]);

  useEffect(() => {
    workspaces.forEach((workspace) => {
      void refreshArchivedThreads(workspace.id);
    });
  }, [refreshArchivedThreads, workspaces]);

  async function onOpenFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) return;
    const workspace = await openWorkspace(selected, readLegacyDefaultScanDepth());
    if (workspace) {
      setCollapsed((prev) => ({ ...prev, [workspace.id]: false }));
    }
  }

  async function onSelectThread(thread: Thread) {
    await switchToThreadTabs(thread);
  }

  async function ensureLocalThreadForDetectedConversation(
    project: UnifiedProjectGroup,
    detectedThread: CodexDetectedProject["threads"][number],
    options?: {
      activate?: boolean;
      revealProject?: boolean;
    },
  ): Promise<Thread | null> {
    const activate = options?.activate !== false;
    const revealProject = options?.revealProject !== false;

    try {
      const attached = await runWithSidebarActionLock(
        getDetectedConversationActionKey(project, detectedThread),
        async () => {
          await ensureActiveCodexProfile(detectedThread.profileId);

          const workspace = await ensureWorkspaceForProject(project);
          if (!workspace) {
            return null;
          }

          if (revealProject) {
            setCollapsed((prev) => ({ ...prev, [workspace.id]: false }));
          }

          const existingThread = useThreadStore
            .getState()
            .threads.find(
              (thread) =>
                thread.workspaceId === workspace.id &&
                thread.engineId === "codex" &&
                thread.engineThreadId === detectedThread.engineThreadId,
            );
          if (existingThread) {
            if (activate) {
              await switchToThreadTabs(existingThread);
            }
            return existingThread;
          }

          const attachedThread = await attachCodexRemoteThread(
            workspace.id,
            detectedThread.engineThreadId,
            preferredCodexModelId,
            { activate },
          );
          if (!attachedThread) {
            throw new Error("Failed to resume Codex CLI conversation.");
          }

          if (activate) {
            await switchToThreadTabs(attachedThread);
          }
          await refreshDetectedCodexProjects();
          return attachedThread;
        },
      );
      return attached ?? null;
    } catch (error) {
      toast.error(String(error));
      return null;
    }
  }

  async function ensureWorkspaceForProject(
    project: UnifiedProjectGroup,
    options?: {
      activate?: boolean;
    },
  ): Promise<Workspace | null> {
    const activate = options?.activate !== false;
    const existingWorkspace =
      project.workspace
      ?? useWorkspaceStore.getState().workspaces.find((workspace) => workspace.rootPath === project.path)
      ?? null;
    if (existingWorkspace) {
      return existingWorkspace;
    }

    const workspace = await openWorkspace(project.path, readLegacyDefaultScanDepth(), {
      activate,
    });
    if (!workspace) {
      return null;
    }

    await refreshDetectedCodexProjects();
    return workspace;
  }

  async function onArchiveProject(project: UnifiedProjectGroup) {
    const workspace = await ensureWorkspaceForProject(project, { activate: false });
    if (!workspace) {
      return;
    }

    onDeleteWorkspace(workspace);
  }

  async function onSelectProject(project: UnifiedProjectGroup) {
    const localWorkspace = project.workspace;
    if (localWorkspace && localWorkspace.id === activeWorkspaceId) {
      setCollapsed((prev) => ({ ...prev, [localWorkspace.id]: !prev[localWorkspace.id] }));
      return;
    }

    await runWithSidebarActionLock(getProjectOpenActionKey(project), async () => {
      if (activeView !== "chat") setActiveView("chat");

      const workspace = await ensureWorkspaceForProject(project);
      if (!workspace) {
        return;
      }

      setCollapsed((prev) => ({
        ...prev,
        [workspace.id]: false,
      }));
      await switchToWorkspaceTabs(workspace.id);
    });
  }

  async function onCreateProjectThread(project: Workspace) {
    await runWithSidebarActionLock(getCreateThreadActionKey(project.id), async () => {
      if (project.id !== activeWorkspaceId) {
        await switchToWorkspaceTabs(project.id);
      }
      setActiveRepo(null, { remember: false });
      const createdThreadId = await createThread({
        workspaceId: project.id,
        repoId: null,
        engineId: preferredOnboardingChatSelection?.engineId,
        modelId: preferredOnboardingChatSelection?.modelId,
        title: t("app:sidebar.newThreadTitle"),
      });
      if (!createdThreadId) return;
      setCollapsed((prev) => ({ ...prev, [project.id]: false }));
      const createdThread =
        useThreadStore
          .getState()
          .threads.find((thread) => thread.id === createdThreadId && thread.workspaceId === project.id)
          ?? null;
      if (createdThread) {
        await switchToThreadTabs(createdThread);
        return;
      }
      await bindChatThread(createdThreadId);
    });
  }

  function onDeleteWorkspace(project: Workspace) {
    setArchiveWorkspacePrompt({ workspace: project });
  }

  async function executeArchiveWorkspace(project: Workspace) {
    setArchiveWorkspacePrompt(null);
    const wasActive = project.id === activeWorkspaceId;
    await removeWorkspace(project.id);
    if (wasActive) {
      setActiveThread(null);
      await bindChatThread(null);
    }
  }

  function onArchiveThread(thread: Thread) {
    setArchiveThreadPrompt({ thread });
  }

  async function executeArchiveThread(thread: Thread) {
    setArchiveThreadPrompt(null);
    const wasActive = thread.id === activeThreadId;
    await removeThread(thread.id);
    await refreshDetectedCodexProjects();
    if (wasActive) {
      setActiveThread(null);
      await bindChatThread(null);
    }
  }

  function onDeleteThreadPermanently(thread: Thread) {
    setDeleteThreadPrompt({ thread });
  }

  async function executeDeleteThread(thread: Thread) {
    setDeleteThreadPrompt(null);
    const wasActive = thread.id === activeThreadId;
    await deleteThread(thread.id);
    await refreshDetectedCodexProjects();
    if (wasActive) {
      setActiveThread(null);
      await bindChatThread(null);
    }
  }

  async function onToggleThreadPinned(thread: Thread) {
    const nextPinned = !isThreadPinned(thread);
    const updated = await runWithSidebarActionLock(getTogglePinnedActionKey(thread.id), () =>
      setThreadPinned(thread.id, nextPinned),
    );
    if (updated === null) {
      toast.error(t("app:sidebar.updateThreadPinFailed"));
    }
  }

  async function onToggleDetectedThreadPinned(
    project: UnifiedProjectGroup,
    detectedThread: CodexDetectedProject["threads"][number],
  ) {
    const localThread = await ensureLocalThreadForDetectedConversation(project, detectedThread, {
      activate: false,
      revealProject: true,
    });
    if (!localThread) {
      return;
    }

    await onToggleThreadPinned(localThread);
  }

  async function onArchiveDetectedThread(
    project: UnifiedProjectGroup,
    detectedThread: CodexDetectedProject["threads"][number],
  ) {
    const localThread = await ensureLocalThreadForDetectedConversation(project, detectedThread, {
      activate: false,
      revealProject: true,
    });
    if (!localThread) {
      return;
    }

    onArchiveThread(localThread);
  }

  async function onRestoreWorkspace(workspace: Workspace) {
    await restoreWorkspace(workspace.id);
  }

  async function onRestoreThread(thread: Thread) {
    await restoreThread(thread.id);
  }

  async function onLocaleSelect(locale: AppLocale) {
    if (locale === activeLocale) return;

    try {
      const savedLocale = await ipc.setAppLocale(locale);
      await i18n.changeLanguage(savedLocale);
      toast.info(t("common:language.changed"));
    } catch {
      toast.error(t("app:sidebar.languageFailed"));
    }
  }

  async function onToggleTerminalAcceleratedRendering() {
    const nextValue = !terminalAcceleratedRendering;

    try {
      const saved = await ipc.setTerminalAcceleratedRendering(nextValue);
      setTerminalAcceleratedRendering(saved);
      emitTerminalAcceleratedRenderingChanged(saved);
    } catch {
      toast.error(t("app:sidebar.terminalAcceleratedRenderingFailed"));
    }
  }

  function getWorkspaceLabel(workspace: Workspace) {
    return workspace.name || workspace.rootPath.split("/").pop() || t("app:sidebar.workspaceFallback");
  }

  function getThreadLabel(thread: Thread) {
    return thread.title?.trim() || t("app:sidebar.untitledThread");
  }

  const preferredCodexModelId = useMemo(() => {
    if (
      preferredOnboardingChatSelection?.engineId === "codex" &&
      preferredOnboardingChatSelection.modelId
    ) {
      return preferredOnboardingChatSelection.modelId;
    }

    const codexEngine = engines.find((engine) => engine.id === "codex");
    return (
      codexEngine?.models.find((model) => model.isDefault && !model.hidden)?.id ??
      codexEngine?.models.find((model) => !model.hidden)?.id ??
      DEFAULT_CODEX_MODEL
    );
  }, [engines, preferredOnboardingChatSelection]);

  async function onSelectDetectedConversation(
    project: UnifiedProjectGroup,
    detectedThread: CodexDetectedProject["threads"][number],
  ) {
    await ensureLocalThreadForDetectedConversation(project, detectedThread, {
      activate: true,
      revealProject: true,
    });
  }

  const keepAwakeDescription = useMemo(() => {
    if (!keepAwakeState) {
      return t("app:sidebar.keepAwakeDescription");
    }
    if (!keepAwakeState?.supported) {
      return t("app:sidebar.keepAwakeUnsupported");
    }
    if (keepAwakeState.enabled && !keepAwakeState.active) {
      return t("app:sidebar.keepAwakeInactive");
    }
    if (
      keepAwakeState.enabled &&
      keepAwakeState.active &&
      keepAwakeState.supportsClosedDisplay === false &&
      keepAwakeState.closedDisplayActive === false
    ) {
      return t("app:sidebar.keepAwakeLimited");
    }
    return t("app:sidebar.keepAwakeDescription");
  }, [keepAwakeState, t]);
  const terminalNotificationDescription = useMemo(() => {
    if (!terminalNotificationLoadedOnce || !terminalNotificationSettings) {
      return t("app:sidebar.terminalNotificationsDescription");
    }
    if (terminalNotificationSettings.chatEnabled && terminalNotificationSettings.terminalEnabled) {
      return t("app:sidebar.terminalNotificationsEnabledAll");
    }
    if (terminalNotificationSettings.chatEnabled) {
      return t("app:sidebar.terminalNotificationsEnabledChat");
    }
    if (terminalNotificationSettings.terminalEnabled) {
      return t("app:sidebar.terminalNotificationsEnabledTerminal");
    }
    if (terminalNotificationSettings.terminalSetupComplete) {
      return t("app:sidebar.terminalNotificationsReady");
    }
    return t("app:sidebar.terminalNotificationsDescription");
  }, [terminalNotificationLoadedOnce, terminalNotificationSettings, t]);

  const terminalNotificationAnyEnabled =
    (terminalNotificationSettings?.chatEnabled ?? false)
    || (terminalNotificationSettings?.terminalEnabled ?? false);
  const terminalNotificationBusy =
    (terminalNotificationLoading && !terminalNotificationLoadedOnce)
    || terminalNotificationUpdatingChatEnabled
    || terminalNotificationUpdatingTerminalEnabled;

  const toggleProjectLibraryCollapsed = useCallback(
    (event: ReactMouseEvent<HTMLButtonElement>) => {
      const shouldExpandAll = event.shiftKey;
      const shouldCollapseAll = event.ctrlKey || event.metaKey;

      if (shouldExpandAll || shouldCollapseAll) {
        setSidebarPersistence((prev) => ({
          ...prev,
          projectLibraryCollapsed: false,
        }));
        setCollapsed((prev) => {
          const next = { ...prev };
          for (const project of projectEntries) {
            next[project.key] = shouldCollapseAll;
          }
          return next;
        });
        return;
      }

      setSidebarPersistence((prev) => ({
        ...prev,
        projectLibraryCollapsed: !prev.projectLibraryCollapsed,
      }));
    },
    [projectEntries],
  );

  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        background: "inherit",
        minWidth: 0,
      }}
    >
      {/* ── Header — drag region + actions ── */}
      <div
        className="sb-header"
        onMouseDown={handleDragMouseDown}
      >
        <div className="sb-header-main no-drag">
          {/* New thread */}
          <button
            type="button"
            className="sb-new-thread-btn"
            style={{ margin: 0 }}
            disabled={
              !activeWorkspaceId ||
              isSidebarActionPending(getCreateThreadActionKey(activeWorkspaceId))
            }
            onClick={() => {
              const activeProject = workspaceProjects.find(
                (p) => p.workspace.id === activeWorkspaceId,
              );
              if (activeProject) {
                void onCreateProjectThread(activeProject.workspace);
              }
            }}
          >
            <Plus size={14} strokeWidth={2.2} />
            {t("app:sidebar.newThread")}
          </button>

          {/* Agents */}
          <button
            type="button"
            className={`sb-open-project-btn${activeView === "harnesses" ? " sb-btn-active" : ""}`}
            style={{ margin: 0 }}
            onClick={() => setActiveView(activeView === "harnesses" ? "chat" : "harnesses")}
          >
            <Terminal size={13} strokeWidth={2} />
            {t("app:sidebar.agents")}
          </button>
        </div>
      </div>

      {/* ── Scrollable content ── */}
      <div className="sb-scroll">
        <SidebarSectionHeader
          label={t("app:sidebar.projects")}
          expanded={!projectLibraryCollapsed}
          controlsId={projectLibrarySectionId}
          onToggle={toggleProjectLibraryCollapsed}
          toggleTitle={
            projectLibraryCollapsed
              ? t("app:sidebar.showProjectLibrary")
              : t("app:sidebar.hideProjectLibrary")
          }
          action={(
            <button
              type="button"
              className="sb-add-project-btn"
              title={t("app:sidebar.openProject")}
              onClick={() => {
                if (activeView !== "chat") setActiveView("chat");
                void onOpenFolder();
              }}
            >
              <Plus size={12} strokeWidth={2.2} />
            </button>
          )}
        />

        {projectLibraryCollapsed ? null : projectEntries.length === 0 ? (
          <div className="sb-empty">
            {t("app:sidebar.noProjects")}
            <br />
            {t("app:sidebar.openFolder")}
          </div>
        ) : (
          <div id={projectLibrarySectionId} className="sb-project-library">
            {projectEntries.map((project) => {
            const workspace = project.workspace;
            const isActiveProject =
              !!workspace && workspace.id === activeWorkspaceId;
            const isCollapsed = collapsed[project.key] ?? true;
            const isShowingAll = showAll[project.key] ?? false;
            const projectOpenActionPending = isSidebarActionPending(
              getProjectOpenActionKey(project),
            );
            const visibleConversations = isShowingAll
              ? project.conversations
              : project.conversations.slice(0, MAX_VISIBLE_THREADS);
            const hasMore = project.conversations.length > MAX_VISIBLE_THREADS;

            return (
              <div key={project.key} style={{ marginBottom: 2 }}>
                {/* Project header */}
                <SidebarProjectRow
                  label={project.name}
                  active={isActiveProject}
                  collapsed={isCollapsed}
                  disabled={projectOpenActionPending}
                  leadingAction={{
                    title: project.isPinnedProject
                      ? t("app:sidebar.unpinProject")
                      : t("app:sidebar.pinProject"),
                    icon: <Pin size={11} style={{ transform: "rotate(-35deg)" }} />,
                    active: project.isPinnedProject,
                    onClick: () => onToggleProjectPinned(project),
                  }}
                  metaAction={
                    {
                      title: t("app:sidebar.archiveProject"),
                      icon: <Archive size={12} />,
                      onClick: () => {
                        void onArchiveProject(project);
                      },
                    }
                  }
                  icon={
                    <FolderGit2
                      size={14}
                      style={{
                        flexShrink: 0,
                        color: isActiveProject ? "var(--accent)" : "var(--text-3)",
                      }}
                    />
                  }
                  onClick={() => {
                    if (!workspace) {
                      void onSelectProject(project);
                      return;
                    }

                    if (isActiveProject) {
                      toggleCollapse(project.key);
                    } else {
                      void onSelectProject(project);
                    }
                  }}
                  trailing={
                    <WorkspaceMoreMenu
                      workspace={workspace}
                      onResolveWorkspace={() =>
                        ensureWorkspaceForProject(project, { activate: false })
                      }
                      onOpenSettings={(resolvedWorkspace) => {
                        openWorkspaceSettings(resolvedWorkspace.id);
                      }}
                      onArchive={(resolvedWorkspace) => {
                        onDeleteWorkspace(resolvedWorkspace);
                      }}
                    />
                  }
                />

                {/* Threads */}
                {!isCollapsed && (
                  <div className="sb-project-thread-list">
                    {project.conversations.length === 0 ? (
                      <div className="sb-no-threads">{t("app:sidebar.noThreads")}</div>
                    ) : (
                      <>
                        {visibleConversations.map((conversation, i) => {
                          const isLocalConversation = conversation.kind === "local";
                          const localThread =
                            conversation.kind === "local" ? conversation.localThread : null;
                          const detectedThread =
                            conversation.kind === "detected"
                              ? conversation.detectedThread
                              : null;
                          const detectedConversationActionPending =
                            detectedThread
                              ? isSidebarActionPending(
                                  getDetectedConversationActionKey(project, detectedThread),
                                )
                              : false;
                          const toggleThreadPinnedActionPending =
                            localThread
                              ? isSidebarActionPending(getTogglePinnedActionKey(localThread.id))
                              : false;
                          return (
                            <SidebarConversationRow
                              key={conversation.key}
                              label={
                                localThread
                                  ? getThreadLabel(localThread)
                                  : (detectedThread?.title ?? "")
                              }
                              timeLabel={
                                localThread
                                  ? (
                                      localThread.lastActivityAt
                                        ? formatRelativeTime(localThread.lastActivityAt, i18n.language)
                                        : ""
                                    )
                                  : (detectedThread
                                      ? formatRelativeTime(detectedThread.updatedAt, i18n.language)
                                      : "")
                              }
                              active={Boolean(localThread && localThread.id === activeThreadId)}
                              animationDelayMs={i * 20}
                              className="sb-library-thread-row"
                              opacity={
                                isLocalConversation
                                  ? undefined
                                  : (detectedThread?.archived ? 0.58 : 1)
                              }
                              disabled={
                                detectedConversationActionPending || toggleThreadPinnedActionPending
                              }
                              leadingAction={{
                                title:
                                  localThread && isThreadPinned(localThread)
                                    ? t("app:sidebar.unpinThread")
                                    : t("app:sidebar.pinThread"),
                                icon: <Pin size={11} style={{ transform: "rotate(-35deg)" }} />,
                                active: localThread ? isThreadPinned(localThread) : false,
                                onClick: () => {
                                  if (localThread) {
                                    void onToggleThreadPinned(localThread);
                                    return;
                                  }
                                  if (detectedThread) {
                                    void onToggleDetectedThreadPinned(project, detectedThread);
                                  }
                                },
                              }}
                              onClick={() => {
                                if (localThread) {
                                  void onSelectThread(localThread);
                                  return;
                                }
                                if (detectedThread) {
                                  void onSelectDetectedConversation(project, detectedThread);
                                }
                              }}
                              metaAction={
                                {
                                  title: t("app:sidebar.archiveThread"),
                                  icon: <Archive size={11} />,
                                  onClick: () => {
                                    if (localThread) {
                                      onArchiveThread(localThread);
                                      return;
                                    }
                                    if (detectedThread) {
                                      void onArchiveDetectedThread(project, detectedThread);
                                    }
                                  },
                                }
                              }
                              trailing={
                                (
                                  <SidebarConversationMoreMenu
                                    actions={
                                      localThread
                                        ? [
                                            {
                                              key: "delete",
                                              label: t("app:sidebar.deleteThread"),
                                              icon: <Trash2 size={13} />,
                                              danger: true,
                                              onSelect: () => {
                                                onDeleteThreadPermanently(localThread);
                                              },
                                            },
                                          ]
                                        : detectedThread
                                          ? [
                                              {
                                                key: "archive",
                                                label: t("app:sidebar.archiveThread"),
                                                icon: <Archive size={13} />,
                                                onSelect: () => {
                                                  void onArchiveDetectedThread(project, detectedThread);
                                                },
                                              },
                                            ]
                                          : []
                                    }
                                    disabled={detectedConversationActionPending || toggleThreadPinnedActionPending}
                                  />
                                )
                              }
                            />
                          );
                        })}

                        {hasMore && (
                          <button
                            type="button"
                            className="sb-show-more"
                            onClick={() =>
                              setShowAll((prev) => ({
                                ...prev,
                                [project.key]: !isShowingAll,
                              }))
                            }
                          >
                            {isShowingAll
                              ? t("app:sidebar.showLess")
                              : t("app:sidebar.showMore", {
                                  count: project.conversations.length - MAX_VISIBLE_THREADS,
                                })}
                          </button>
                        )}
                      </>
                    )}
                  </div>
                )}
              </div>
            );
          })}
          </div>
        )}

        {/* Archived section */}
        <div className="sb-archived-section">
          <div className="sb-archived-header">
            <button
              type="button"
              className="sb-archived-toggle"
              onClick={() => setArchivedOpen((c) => !c)}
            >
              {archivedOpen ? (
                <ChevronDown size={11} style={{ flexShrink: 0, opacity: 0.6 }} />
              ) : (
                <ChevronRight size={11} style={{ flexShrink: 0, opacity: 0.6 }} />
              )}
              <span className="sb-archived-title">{t("app:sidebar.archived")}</span>
            </button>
            <div className="sb-section-meta">
              <span className="sb-project-count-placeholder" aria-hidden="true" />
            </div>
            <div className="sb-section-action">
              <span className="sb-project-count">
                {archivedWorkspaces.length + archivedThreads.length}
              </span>
            </div>
          </div>

          {archivedOpen && (
            <div className="sb-archived-list">
              {archivedWorkspaces.map((workspace) => (
                <div key={workspace.id} className="sb-archived-item">
                  <FolderGit2 size={12} style={{ flexShrink: 0, color: "var(--text-3)" }} />
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                    title={workspace.name || workspace.rootPath}
                  >
                    {getWorkspaceLabel(workspace)}
                  </span>
                  <button
                    type="button"
                    className="sb-archived-restore"
                    onClick={() => void onRestoreWorkspace(workspace)}
                    title={t("app:sidebar.restoreWorkspace")}
                  >
                    <RotateCcw size={11} />
                  </button>
                </div>
              ))}

              {archivedThreads.map((thread) => (
                <div key={thread.id} className="sb-archived-item">
                  <MessageSquare size={12} style={{ flexShrink: 0, color: "var(--text-3)" }} />
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                    title={getThreadLabel(thread)}
                  >
                    {getThreadLabel(thread)}
                  </span>
                  <button
                    type="button"
                    className="sb-archived-restore"
                    onClick={() => void onRestoreThread(thread)}
                    title={t("app:sidebar.restoreThread")}
                  >
                    <RotateCcw size={11} />
                  </button>
                </div>
              ))}

              {archivedWorkspaces.length === 0 && archivedThreads.length === 0 && (
                <div className="sb-no-threads">{t("app:sidebar.nothingArchived")}</div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* ── Footer ── */}
      <div className="sb-footer">
        <button
          ref={settingsTriggerRef}
          type="button"
          className="sb-settings-btn"
          onClick={() => {
            if (settingsMenuOpen) {
              closeSettingsMenu();
              return;
            }
            const rawRect = settingsTriggerRef.current?.getBoundingClientRect();
            if (rawRect) {
              const rect = normalizeClientRectForFixedPosition(rawRect);
              const viewport = readFixedViewportSize();
              setSettingsMenuPos({
                bottom: Math.max(8, viewport.height - rect.top + 4),
                left: Math.max(8, Math.min(rect.left, viewport.width - 268)),
              });
            }
            setSettingsMenuOpen(true);
          }}
        >
          <span style={{ position: "relative", display: "inline-flex" }}>
            <Settings size={14} style={{ opacity: 0.5 }} />
            {hasUpdate && <span className="sb-update-dot" />}
          </span>
          {t("app:sidebar.settings")}
        </button>

      </div>

      {/* Settings portal menu */}
      {settingsMenuOpen &&
        createPortal(
          <div
            ref={settingsMenuRef}
            className="git-action-menu"
            style={{
              position: "fixed",
              bottom: settingsMenuPos.bottom,
              left: settingsMenuPos.left,
              minWidth: 260,
            }}
          >
            {/* ── Preferences ── */}
            <div
              style={{
                padding: "6px 12px 4px",
                fontSize: 10,
                color: "var(--text-3)",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
              }}
            >
              {t("app:sidebar.preferences")}
            </div>
            <div
              className="git-action-menu-item"
              style={{
                justifyContent: "space-between",
                opacity: keepAwakeLoading || !keepAwakeAvailable ? 0.5 : 1,
              }}
            >
              <button
                type="button"
                title={keepAwakeDescription}
                onClick={() => openPowerSettings()}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  color: "inherit",
                  padding: 0,
                  flex: 1,
                  minWidth: 0,
                }}
              >
                <PillBottle size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
                {t("app:sidebar.keepAwake")}
              </button>
              <label
                className="ws-toggle"
                title={keepAwakeDescription}
                onClick={(e) => e.stopPropagation()}
                style={{ cursor: keepAwakeLoading || !keepAwakeAvailable ? "not-allowed" : undefined }}
              >
                <input
                  type="checkbox"
                  checked={keepAwakeState?.enabled ?? false}
                  disabled={keepAwakeLoading || !keepAwakeAvailable}
                  onChange={() => void toggleKeepAwake()}
                />
                <span className="ws-toggle-track" />
                <span className="ws-toggle-thumb" />
              </label>
            </div>
            <div
              className="git-action-menu-item"
              style={{
                justifyContent: "space-between",
                opacity:
                  terminalNotificationBusy
                    ? 0.75
                    : 1,
              }}
            >
              <button
                type="button"
                title={terminalNotificationDescription}
                onClick={() => openTerminalNotificationSettings()}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  color: "inherit",
                  padding: 0,
                  flex: 1,
                  minWidth: 0,
                }}
              >
                <BellRing size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
                {t("app:sidebar.terminalNotifications")}
              </button>
              <label
                className="ws-toggle"
                title={terminalNotificationDescription}
                onClick={(e) => e.stopPropagation()}
                style={{
                  cursor:
                    terminalNotificationBusy
                      ? "wait"
                      : undefined,
                }}
              >
                <input
                  type="checkbox"
                  checked={terminalNotificationAnyEnabled}
                  disabled={terminalNotificationBusy}
                  onChange={() => { void toggleTerminalNotifications(); }}
                />
                <span className="ws-toggle-track" />
                <span className="ws-toggle-thumb" />
              </label>
            </div>
            <div className="git-action-menu-item" style={{ justifyContent: "space-between", cursor: "default" }}>
              <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Globe size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
                {t("common:language.label")}
              </span>
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  background: "var(--surface-3)",
                  borderRadius: 6,
                  padding: 2,
                  gap: 2,
                }}
              >
                {SUPPORTED_APP_LOCALES.map((locale) => (
                  <button
                    key={locale}
                    type="button"
                    onClick={() => { void onLocaleSelect(locale); }}
                    style={{
                      fontSize: 11,
                      lineHeight: 1,
                      padding: "3px 8px",
                      borderRadius: 4,
                      border: "none",
                      cursor: "pointer",
                      background: activeLocale === locale ? "var(--accent-dim)" : "transparent",
                      color: activeLocale === locale ? "var(--accent)" : "var(--text-3)",
                      fontWeight: activeLocale === locale ? 500 : 400,
                      boxShadow: "none",
                      transition: "background 0.15s, color 0.15s, box-shadow 0.15s",
                    }}
                  >
                    {locale === "en" ? "EN-US" : "PT-BR"}
                  </button>
                ))}
              </span>
            </div>
            <button
              type="button"
              className="git-action-menu-item"
              onClick={() => {
                closeSettingsMenu();
                openCodexProfilesModal();
              }}
            >
              <UserCircle size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
              Codex profiles
            </button>
            <button
              type="button"
              className="git-action-menu-item"
              onClick={() => {
                closeSettingsMenu();
                openAppearanceSettings();
              }}
            >
              <Monitor size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
              Appearance
            </button>
            <button
              type="button"
              className="git-action-menu-item"
              onClick={() => {
                closeSettingsMenu();
                openShortcutSettings();
              }}
            >
              <Keyboard size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
              {t("app:shortcutSettings.title")}
            </button>

            <div className="git-action-menu-divider" />
            <div
              style={{
                padding: "6px 10px 4px",
                fontSize: 11,
                color: "var(--text-3)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
              }}
            >
              {t("app:sidebar.terminal")}
            </div>
            <button
              type="button"
              className="git-action-menu-item"
              style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}
              onClick={() => {
                void onToggleTerminalAcceleratedRendering();
              }}
            >
              <span>{t("app:sidebar.terminalAcceleratedRendering")}</span>
              {terminalAcceleratedRendering ? <Check size={12} /> : null}
            </button>
            <div className="git-action-menu-divider" />

            {/* ── Actions ── */}
            <button
              type="button"
              className="git-action-menu-item"
              onClick={() => {
                closeSettingsMenu();
                openOnboarding();
              }}
            >
              <Rocket size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
              {t("app:sidebar.engineSetup")}
            </button>
            <button
              type="button"
              className="git-action-menu-item"
              style={{ justifyContent: "space-between" }}
              onClick={() => {
                closeSettingsMenu();
                setUpdateDialogOpen(true);
              }}
            >
              <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <RefreshCw size={14} style={{ opacity: 0.5, flexShrink: 0 }} />
                {t("app:sidebar.checkUpdates")}
              </span>
              {hasUpdate && (
                <span
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: "50%",
                    background: "var(--accent)",
                    flexShrink: 0,
                  }}
                />
              )}
            </button>
          </div>,
          document.body,
        )}

      <UpdateDialog open={updateDialogOpen} onClose={() => setUpdateDialogOpen(false)} />

      {createPortal(
        <ConfirmDialog
          open={archiveWorkspacePrompt !== null}
          title={t("app:sidebar.archiveWorkspaceTitle")}
          message={
            archiveWorkspacePrompt
              ? t("app:sidebar.archiveWorkspaceMessage", {
                  name: getWorkspaceLabel(archiveWorkspacePrompt.workspace),
                })
              : ""
          }
          confirmLabel={t("app:sidebar.archive")}
          onConfirm={() => {
            if (archiveWorkspacePrompt) void executeArchiveWorkspace(archiveWorkspacePrompt.workspace);
          }}
          onCancel={() => setArchiveWorkspacePrompt(null)}
        />,
        document.body,
      )}

      {createPortal(
        <ConfirmDialog
          open={archiveThreadPrompt !== null}
          title={t("app:sidebar.archiveThreadTitle")}
          message={
            archiveThreadPrompt
              ? t("app:sidebar.archiveThreadMessage", {
                  name: getThreadLabel(archiveThreadPrompt.thread),
                })
              : ""
          }
          confirmLabel={t("app:sidebar.archive")}
          onConfirm={() => {
            if (archiveThreadPrompt) void executeArchiveThread(archiveThreadPrompt.thread);
          }}
          onCancel={() => setArchiveThreadPrompt(null)}
        />,
        document.body,
      )}

      {createPortal(
        <ConfirmDialog
          open={deleteThreadPrompt !== null}
          title={t("app:sidebar.deleteThreadTitle")}
          message={
            deleteThreadPrompt
              ? t("app:sidebar.deleteThreadMessage", {
                  name: getThreadLabel(deleteThreadPrompt.thread),
                })
              : ""
          }
          confirmLabel={t("app:sidebar.deleteThread")}
          onConfirm={() => {
            if (deleteThreadPrompt) void executeDeleteThread(deleteThreadPrompt.thread);
          }}
          onCancel={() => setDeleteThreadPrompt(null)}
        />,
        document.body,
      )}

      {error && (
        <div
          style={{
            padding: "8px 12px",
            fontSize: 12,
            color: "var(--danger)",
            borderTop: "1px solid rgba(248, 113, 113, 0.15)",
            background: "rgba(248, 113, 113, 0.06)",
          }}
        >
          {error}
        </div>
      )}
    </div>
  );
}

export function Sidebar() {
  return <SidebarContent />;
}
