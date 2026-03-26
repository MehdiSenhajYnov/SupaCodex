import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
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
import type { CodexDetectedProject, Thread, Workspace } from "../../types";

interface ProjectGroup {
  workspace: Workspace;
  threads: Thread[];
}

interface UnifiedProjectGroup {
  key: string;
  path: string;
  name: string;
  workspace: Workspace | null;
  detectedCodexProject: CodexDetectedProject | null;
  conversations: SidebarConversation[];
  totalConversationCount: number;
  latestActivityAt: string;
}

type SidebarConversation =
  | {
      kind: "local";
      key: string;
      updatedAt: string;
      localThread: Thread;
    }
  | {
      kind: "detected";
      key: string;
      updatedAt: string;
      detectedThread: CodexDetectedProject["threads"][number];
    };

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
  trailingAction?: {
    title: string;
    icon: ReactNode;
    onClick: () => void;
  };
}

interface SidebarProjectRowProps {
  label: string;
  count?: number;
  active?: boolean;
  collapsed?: boolean;
  disabled?: boolean;
  icon: ReactNode;
  wrapperClassName?: string;
  onClick: () => void;
  onMiddleClick?: () => void;
  trailing?: ReactNode;
}

interface SidebarSectionHeaderProps {
  label: string;
  count: number;
  expanded: boolean;
  controlsId: string;
  toggleTitle: string;
  onToggle: () => void;
  action?: ReactNode;
}

const MAX_VISIBLE_THREADS = 8;
const LEGACY_SCAN_DEPTH_STORAGE_KEY = "supacodex.workspace.scanDepth";
const SIDEBAR_SECTION_STATE_STORAGE_KEY = "supacodex:sidebarSections:v1";
const LEGACY_SCAN_DEPTH_MIN = 0;
const LEGACY_SCAN_DEPTH_MAX = 12;
const DEFAULT_CODEX_MODEL = "gpt-5.3-codex";

interface SidebarSectionState {
  openProjectsCollapsed: boolean;
  projectLibraryCollapsed: boolean;
}

function SidebarProjectRow({
  label,
  count,
  active = false,
  collapsed = false,
  disabled = false,
  icon,
  wrapperClassName,
  onClick,
  onMiddleClick,
  trailing,
}: SidebarProjectRowProps) {
  return (
    <div
      className={`sb-project-row ${active ? "sb-project-row-active" : ""}${disabled ? " sb-project-row-disabled" : ""}${wrapperClassName ? ` ${wrapperClassName}` : ""}`}
    >
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
        {collapsed ? (
          <ChevronRight size={12} style={{ flexShrink: 0, opacity: 0.4 }} />
        ) : (
          <ChevronDown size={12} style={{ flexShrink: 0, opacity: 0.4 }} />
        )}
        {icon}
        <span className="sb-project-name">{label}</span>
      </button>
      <div className="sb-project-row-meta">
        {typeof count === "number" && count > 0 ? (
          <span className="sb-project-count">{count}</span>
        ) : (
          <span className="sb-project-count-placeholder" aria-hidden="true" />
        )}
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
  trailingAction,
}: SidebarConversationRowProps) {
  return (
    <div
      className={`sb-thread sb-thread-animate ${active ? "sb-thread-active" : ""}${disabled ? " sb-thread-disabled" : ""}${className ? ` ${className}` : ""}`}
      style={{
        animationDelay: `${animationDelayMs}ms`,
        ...(opacity === undefined ? {} : { opacity }),
      }}
    >
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
      <span className={`sb-thread-time-slot${timeLabel ? "" : " sb-thread-time-slot-empty"}`}>
        {timeLabel ?? ""}
      </span>
      <div className="sb-thread-action-slot">
        {trailingAction ? (
          <button
            type="button"
            aria-label={trailingAction.title}
            title={trailingAction.title}
            className="sb-thread-archive"
            disabled={disabled}
            onMouseDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              trailingAction.onClick();
            }}
          >
            {trailingAction.icon}
          </button>
        ) : (
          <span className="sb-thread-action-placeholder" aria-hidden="true" />
        )}
      </div>
    </div>
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
  return (
    <div className="sb-section-label">
      <button
        type="button"
        className="sb-section-toggle"
        aria-expanded={expanded}
        aria-controls={controlsId}
        onClick={onToggle}
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
        <span className="sb-project-count">{count}</span>
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

function readSidebarSectionState(): SidebarSectionState {
  try {
    const raw = window.localStorage.getItem(SIDEBAR_SECTION_STATE_STORAGE_KEY);
    if (!raw) {
      return {
        openProjectsCollapsed: false,
        projectLibraryCollapsed: false,
      };
    }

    const parsed = JSON.parse(raw) as Partial<SidebarSectionState> | null;
    if (!parsed || typeof parsed !== "object") {
      throw new Error("invalid sidebar section state");
    }

    return {
      openProjectsCollapsed: parsed.openProjectsCollapsed === true,
      projectLibraryCollapsed: parsed.projectLibraryCollapsed === true,
    };
  } catch {
    return {
      openProjectsCollapsed: false,
      projectLibraryCollapsed: false,
    };
  }
}

function writeSidebarSectionState(state: SidebarSectionState): void {
  try {
    window.localStorage.setItem(SIDEBAR_SECTION_STATE_STORAGE_KEY, JSON.stringify(state));
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
    restoreThread,
    createThread,
    refreshArchivedThreads,
    attachCodexRemoteThread,
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
  const openWorkspaceIds = useProjectTabsStore((s) => s.openWorkspaceIds);
  const tabThreadIdsByWorkspace = useProjectTabsStore((s) => s.tabThreadIdsByWorkspace);
  const switchToWorkspaceTabs = useProjectTabsStore((s) => s.switchToWorkspace);
  const switchToThreadTabs = useProjectTabsStore((s) => s.switchToThread);
  const closeProjectWorkspace = useProjectTabsStore((s) => s.closeWorkspace);
  const closeProjectThreadTab = useProjectTabsStore((s) => s.closeThreadTab);
  const hasUpdate = updateStatus === "available" && !updateSnoozed;
  const keepAwakeAvailable = canToggleKeepAwake(keepAwakeState);
  const preferredOnboardingChatSelection = useMemo(
    () => resolvePreferredOnboardingChatSelection(selectedChatEngines, engines),
    [engines, selectedChatEngines],
  );

  const workspaceProjects = useMemo<ProjectGroup[]>(
    () =>
      workspaces.map((ws) => ({
        workspace: ws,
        threads: threads.filter((t) => t.workspaceId === ws.id),
      })),
    [workspaces, threads],
  );
  const detectedCodexProjectsByWorkspaceId = useMemo(
    () =>
      detectedCodexProjects.reduce<Record<string, CodexDetectedProject>>((acc, project) => {
        if (project.workspaceId) {
          acc[project.workspaceId] = project;
        }
        return acc;
      }, {}),
    [detectedCodexProjects],
  );
  const projectEntries = useMemo<UnifiedProjectGroup[]>(() => {
    const importedProjects = workspaceProjects.map((project) => {
      const detectedCodexProject =
        detectedCodexProjectsByWorkspaceId[project.workspace.id] ?? null;
      const attachedEngineThreadIds = new Set(
        project.threads
          .map((thread) => thread.engineThreadId)
          .filter((engineThreadId): engineThreadId is string => Boolean(engineThreadId)),
      );
      const detectedConversations = (detectedCodexProject?.threads ?? [])
        .filter((thread) => !attachedEngineThreadIds.has(thread.engineThreadId))
        .map<SidebarConversation>((thread) => ({
          kind: "detected",
          key: `detected:${thread.profileId}:${thread.engineThreadId}`,
          updatedAt: thread.updatedAt,
          detectedThread: thread,
        }));
      const localConversations = project.threads.map<SidebarConversation>((thread) => ({
        kind: "local",
        key: `local:${thread.id}`,
        updatedAt: thread.lastActivityAt,
        localThread: thread,
      }));
      const conversations = [...localConversations, ...detectedConversations].sort(
        (left, right) => new Date(right.updatedAt).getTime() - new Date(left.updatedAt).getTime(),
      );

      return {
        key: project.workspace.id,
        path: project.workspace.rootPath,
        name: getWorkspaceLabel(project.workspace),
        workspace: project.workspace,
        detectedCodexProject,
        conversations,
        totalConversationCount: conversations.length,
        latestActivityAt:
          conversations[0]?.updatedAt ??
          detectedCodexProject?.lastActivityAt ??
          project.workspace.lastOpenedAt,
      };
    });

    const externalProjects = detectedCodexProjects
      .filter((project) => !project.workspaceId)
      .map<UnifiedProjectGroup>((project) => ({
        key: `detected:${project.path}`,
        path: project.path,
        name: project.name,
        workspace: null,
        detectedCodexProject: project,
        conversations: project.threads
          .map<SidebarConversation>((thread) => ({
            kind: "detected",
            key: `detected:${thread.profileId}:${thread.engineThreadId}`,
            updatedAt: thread.updatedAt,
            detectedThread: thread,
          }))
          .sort((left, right) => new Date(right.updatedAt).getTime() - new Date(left.updatedAt).getTime()),
        totalConversationCount: project.threads.length,
        latestActivityAt: project.lastActivityAt,
      }));

    return [...importedProjects, ...externalProjects].sort((left, right) => {
      const timeDiff =
        new Date(right.latestActivityAt).getTime() - new Date(left.latestActivityAt).getTime();
      if (timeDiff !== 0) {
        return timeDiff;
      }
      return left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
    });
  }, [detectedCodexProjects, detectedCodexProjectsByWorkspaceId, threads, workspaceProjects]);

  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [showAll, setShowAll] = useState<Record<string, boolean>>({});
  const [sidebarSections, setSidebarSections] = useState<SidebarSectionState>(() =>
    readSidebarSectionState(),
  );
  const [pendingActionKeys, setPendingActionKeys] = useState<Record<string, true>>({});
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [archiveWorkspacePrompt, setArchiveWorkspacePrompt] = useState<{
    workspace: Workspace;
  } | null>(null);
  const [archiveThreadPrompt, setArchiveThreadPrompt] = useState<{
    thread: Thread;
  } | null>(null);
  const [settingsMenuOpen, setSettingsMenuOpen] = useState(false);
  const [settingsMenuPos, setSettingsMenuPos] = useState({ top: 0, left: 0 });
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
    writeSidebarSectionState(sidebarSections);
  }, [sidebarSections]);

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
      activeWorkspaceId
        ? archivedThreadsByWorkspace[activeWorkspaceId] ?? []
        : [],
    [archivedThreadsByWorkspace, activeWorkspaceId],
  );
  const openProjectEntries = useMemo(
    () =>
      openWorkspaceIds
        .map((workspaceId) =>
          projectEntries.find((project) => project.workspace?.id === workspaceId) ?? null,
        )
        .filter((project): project is UnifiedProjectGroup => project !== null),
    [openWorkspaceIds, projectEntries],
  );
  const projectLibraryEntries = useMemo(
    () =>
      projectEntries.filter((project) => {
        const workspaceId = project.workspace?.id;
        return !workspaceId || !openWorkspaceIds.includes(workspaceId);
      }),
    [openWorkspaceIds, projectEntries],
  );
  const openProjectsCollapsed = sidebarSections.openProjectsCollapsed;
  const projectLibraryCollapsed = sidebarSections.projectLibraryCollapsed;
  const openProjectsSectionId = "sidebar-open-projects";
  const projectLibrarySectionId = "sidebar-project-library";

  const toggleCollapse = (projectKey: string) =>
    setCollapsed((prev) => ({ ...prev, [projectKey]: !prev[projectKey] }));
  const toggleOpenProjectsCollapsed = () =>
    setSidebarSections((prev) => ({
      ...prev,
      openProjectsCollapsed: !prev.openProjectsCollapsed,
    }));
  const toggleProjectLibraryCollapsed = () =>
    setSidebarSections((prev) => ({
      ...prev,
      projectLibraryCollapsed: !prev.projectLibraryCollapsed,
    }));

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

  useEffect(() => {
    void refreshArchivedWorkspaces();
  }, [refreshArchivedWorkspaces]);

  useEffect(() => {
    if (!activeWorkspaceId) return;
    void refreshArchivedThreads(activeWorkspaceId);
  }, [activeWorkspaceId, refreshArchivedThreads]);

  async function onOpenFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) return;
    await openWorkspace(selected, readLegacyDefaultScanDepth());
  }

  async function onSelectThread(thread: Thread) {
    await switchToThreadTabs(thread);
  }

  async function ensureWorkspaceForProject(
    project: UnifiedProjectGroup,
  ): Promise<Workspace | null> {
    if (project.workspace) {
      return project.workspace;
    }

    const workspace = await openWorkspace(project.path, readLegacyDefaultScanDepth());
    if (!workspace) {
      return null;
    }

    await refreshDetectedCodexProjects();
    return workspace;
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

      setCollapsed(
        Object.fromEntries(
          projectEntries.map((entry) => [
            entry.workspace?.id ?? entry.key,
            (entry.workspace?.id ?? entry.key) !== workspace.id,
          ]),
        ),
      );
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

  function onDeleteThread(thread: Thread) {
    setArchiveThreadPrompt({ thread });
  }

  async function executeArchiveThread(thread: Thread) {
    setArchiveThreadPrompt(null);
    const wasActive = thread.id === activeThreadId;
    await removeThread(thread.id);
    if (wasActive) {
      setActiveThread(null);
      await bindChatThread(null);
    }
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
    try {
      await runWithSidebarActionLock(
        getDetectedConversationActionKey(project, detectedThread),
        async () => {
          await ensureActiveCodexProfile(detectedThread.profileId);

          const workspace = await ensureWorkspaceForProject(project);
          if (!workspace) {
            return;
          }

          const existingThread = threads.find(
            (thread) =>
              thread.workspaceId === workspace.id &&
              thread.engineId === "codex" &&
              thread.engineThreadId === detectedThread.engineThreadId,
          );
          if (existingThread) {
            await onSelectThread(existingThread);
            return;
          }

          const attachedThread = await attachCodexRemoteThread(
            workspace.id,
            detectedThread.engineThreadId,
            preferredCodexModelId,
          );
          if (!attachedThread) {
            throw new Error("Failed to resume Codex CLI conversation.");
          }

          setCollapsed((prev) => ({ ...prev, [workspace.id]: false }));
          await switchToThreadTabs(attachedThread);
          await refreshDetectedCodexProjects();
        },
      );
    } catch (error) {
      toast.error(String(error));
    }
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
        {openProjectEntries.length > 0 && (
          <>
            <SidebarSectionHeader
              label={t("app:sidebar.openProjects")}
              count={openProjectEntries.length}
              expanded={!openProjectsCollapsed}
              controlsId={openProjectsSectionId}
              onToggle={toggleOpenProjectsCollapsed}
              toggleTitle={
                openProjectsCollapsed
                  ? t("app:sidebar.showOpenProjects")
                  : t("app:sidebar.hideOpenProjects")
              }
            />
            {!openProjectsCollapsed && (
              <div id={openProjectsSectionId} className="sb-open-projects-list">
                {openProjectEntries.map((project) => {
                  const workspace = project.workspace;
                  if (!workspace) {
                    return null;
                  }
                  const isActive = workspace.id === activeWorkspaceId;
                  const isCollapsed = collapsed[workspace.id] ?? false;
                  const openThreadIds = new Set(tabThreadIdsByWorkspace[workspace.id] ?? []);
                  return (
                    <div key={workspace.id} className="sb-open-project-entry">
                      <SidebarProjectRow
                        label={getWorkspaceLabel(workspace)}
                        count={project.totalConversationCount}
                        active={isActive}
                        collapsed={isCollapsed}
                        icon={
                          <FolderGit2
                            size={14}
                            style={{
                              flexShrink: 0,
                              color: isActive ? "var(--accent)" : "var(--text-3)",
                            }}
                          />
                        }
                        onClick={() => {
                          if (isActive) {
                            toggleCollapse(workspace.id);
                            return;
                          }
                          setCollapsed((prev) => ({ ...prev, [workspace.id]: false }));
                          void switchToWorkspaceTabs(workspace.id);
                        }}
                        onMiddleClick={() => {
                          void closeProjectWorkspace(workspace.id);
                        }}
                        trailing={
                          <button
                            type="button"
                            className="sb-project-archive"
                            title={t("app:sidebar.closeOpenProject")}
                            aria-label={t("app:sidebar.closeOpenProject")}
                            onMouseDown={(event) => event.stopPropagation()}
                            onClick={(event) => {
                              event.stopPropagation();
                              void closeProjectWorkspace(workspace.id);
                            }}
                          >
                            <X size={11} />
                          </button>
                        }
                      />

                      {!isCollapsed && project.conversations.length > 0 && (
                        <div className="sb-open-project-tabs">
                          {project.conversations.map((conversation) => {
                            const localThread =
                              conversation.kind === "local" ? conversation.localThread : null;
                            const detectedThread =
                              conversation.kind === "detected"
                                ? conversation.detectedThread
                                : null;
                            const isOpenTab = Boolean(localThread && openThreadIds.has(localThread.id));

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
                                animationDelayMs={0}
                                className="sb-open-project-tab-row"
                                onClick={() => {
                                  if (localThread) {
                                    void switchToThreadTabs(localThread);
                                    return;
                                  }
                                  if (detectedThread) {
                                    void onSelectDetectedConversation(project, detectedThread);
                                  }
                                }}
                                onMiddleClick={
                                  isOpenTab && localThread
                                    ? () => {
                                        void closeProjectThreadTab(workspace.id, localThread.id);
                                      }
                                    : undefined
                                }
                                trailingAction={
                                  isOpenTab && localThread
                                    ? {
                                        title: t("chat:panel.projectTabs.closeTab"),
                                        icon: <X size={11} />,
                                        onClick: () => {
                                          void closeProjectThreadTab(workspace.id, localThread.id);
                                        },
                                      }
                                    : undefined
                                }
                              />
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}

        <SidebarSectionHeader
          label={t("app:sidebar.projects")}
          count={projectLibraryEntries.length}
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

        {projectLibraryCollapsed ? null : projectLibraryEntries.length === 0 ? (
          <div className="sb-empty">
            {t("app:sidebar.noProjects")}
            <br />
            {t("app:sidebar.openFolder")}
          </div>
        ) : (
          <div id={projectLibrarySectionId} className="sb-project-library">
            {projectLibraryEntries.map((project) => {
            const workspace = project.workspace;
            const isActiveProject =
              !!workspace && workspace.id === activeWorkspaceId;
            const isCollapsed = collapsed[project.key] ?? false;
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
                  count={project.totalConversationCount}
                  active={isActiveProject}
                  collapsed={isCollapsed}
                  disabled={projectOpenActionPending}
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
                    workspace ? (
                      <WorkspaceMoreMenu
                        workspace={workspace}
                        onOpenSettings={() => openWorkspaceSettings(workspace.id)}
                        onArchive={() => onDeleteWorkspace(workspace)}
                      />
                    ) : undefined
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
                              disabled={detectedConversationActionPending}
                              onClick={() => {
                                if (localThread) {
                                  void onSelectThread(localThread);
                                  return;
                                }
                                if (detectedThread) {
                                  void onSelectDetectedConversation(project, detectedThread);
                                }
                              }}
                              trailingAction={
                                localThread
                                  ? {
                                      title: t("app:sidebar.archiveThread"),
                                      icon: <Archive size={11} />,
                                      onClick: () => {
                                        void onDeleteThread(localThread);
                                      },
                                    }
                                  : undefined
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
            <Archive size={11} style={{ flexShrink: 0, opacity: 0.6 }} />
            <span style={{ flex: 1, textAlign: "left" }}>{t("app:sidebar.archived")}</span>
            <span className="sb-project-count">
              {archivedWorkspaces.length + archivedThreads.length}
            </span>
          </button>

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
            const rect = settingsTriggerRef.current?.getBoundingClientRect();
            if (rect) {
              setSettingsMenuPos({ top: rect.top - 4, left: rect.left });
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
              bottom: window.innerHeight - settingsMenuPos.top,
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
