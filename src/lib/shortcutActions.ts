import { t } from "../i18n";
import { getActiveEditorView, openSearchPanel } from "../components/editor/CodeMirrorEditor";
import { resolvePreferredOnboardingChatSelection } from "./onboarding";
import { requestWindowClose, toggleWindowFullscreen } from "./windowActions";
import type { ShortcutActionId } from "./shortcutBindings";
import { useChatStore } from "../stores/chatStore";
import { useEngineStore } from "../stores/engineStore";
import { useFileStore } from "../stores/fileStore";
import { useOnboardingStore } from "../stores/onboardingStore";
import { useProjectTabsStore } from "../stores/projectTabsStore";
import { useTerminalStore, collectSessionIds } from "../stores/terminalStore";
import { useThreadStore } from "../stores/threadStore";
import { useUiStore } from "../stores/uiStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

let newThreadInFlight = false;

async function executeNewThreadShortcut(): Promise<void> {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (!workspaceId || newThreadInFlight) {
    return;
  }

  const preferredChatSelection = resolvePreferredOnboardingChatSelection(
    useOnboardingStore.getState().selectedChatEngines,
    useEngineStore.getState().engines,
  );

  newThreadInFlight = true;
  try {
    useWorkspaceStore.getState().setActiveRepo(null, { remember: false });
    const threadId = await useThreadStore.getState().createThread({
      workspaceId,
      repoId: null,
      engineId: preferredChatSelection?.engineId,
      modelId: preferredChatSelection?.modelId,
      title: t("app:sidebar.newThreadTitle"),
    });
    if (!threadId) {
      return;
    }

    const createdThread =
      useThreadStore
        .getState()
        .threads.find((thread) => thread.id === threadId && thread.workspaceId === workspaceId)
        ?? null;

    if (createdThread) {
      await useProjectTabsStore.getState().switchToThread(createdThread);
      return;
    }
    await useChatStore.getState().setActiveThread(threadId);
  } finally {
    newThreadInFlight = false;
  }
}

function executeSearchShortcut(): void {
  useUiStore.getState().openCommandPalette({ variant: "search", initialQuery: "?" });
}

function executeToggleEditorShortcut(): void {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (!workspaceId) {
    return;
  }

  const workspaceState = useTerminalStore.getState().workspaces[workspaceId];
  const currentMode = workspaceState?.layoutMode ?? "chat";
  if (currentMode === "editor") {
    void useTerminalStore.getState().setLayoutMode(
      workspaceId,
      workspaceState?.preEditorLayoutMode ?? "chat",
    );
    return;
  }

  void useTerminalStore.getState().setLayoutMode(workspaceId, "editor");
}

function executeToggleTerminalShortcut(): void {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (workspaceId) {
    void useTerminalStore.getState().cycleLayoutMode(workspaceId);
  }
}

function executeToggleCommandPaletteShortcut(): void {
  const uiState = useUiStore.getState();
  if (uiState.commandPaletteOpen) {
    uiState.closeCommandPalette();
    return;
  }
  uiState.openCommandPalette();
}

export async function executeShortcutAction(actionId: ShortcutActionId): Promise<void> {
  switch (actionId) {
    case "toggle-sidebar":
      useUiStore.getState().toggleSidebar();
      return;
    case "toggle-git-panel":
      useUiStore.getState().toggleGitPanel();
      return;
    case "toggle-focus-mode":
      useUiStore.getState().toggleFocusMode();
      return;
    case "toggle-fullscreen":
      await toggleWindowFullscreen();
      return;
    case "toggle-search":
      executeSearchShortcut();
      return;
    case "toggle-terminal":
      executeToggleTerminalShortcut();
      return;
    case "previous-conversation":
      await useProjectTabsStore.getState().switchToRelativeThread(-1);
      return;
    case "next-conversation":
      await useProjectTabsStore.getState().switchToRelativeThread(1);
      return;
    case "previous-open-project":
      await useProjectTabsStore.getState().switchToRelativeWorkspace(-1);
      return;
    case "next-open-project":
      await useProjectTabsStore.getState().switchToRelativeWorkspace(1);
      return;
    case "close-conversation":
      await useProjectTabsStore.getState().closeActiveThreadTab();
      return;
    case "new-thread":
      await executeNewThreadShortcut();
      return;
    case "toggle-command-palette":
      executeToggleCommandPaletteShortcut();
      return;
    case "open-command-palette-files":
      useUiStore.getState().openCommandPalette({ initialQuery: "%" });
      return;
    case "open-command-palette-threads":
      useUiStore.getState().openCommandPalette({ initialQuery: "@" });
      return;
    case "toggle-editor":
      executeToggleEditorShortcut();
      return;
    default:
      return;
  }
}

export function executeEditorFindShortcut(openReplace = false): void {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  const workspaceState = workspaceId ? useTerminalStore.getState().workspaces[workspaceId] : undefined;
  if (workspaceState?.layoutMode !== "editor") {
    return;
  }

  const fileState = useFileStore.getState();
  const activeTabId = fileState.activeTabId;
  if (!activeTabId) {
    return;
  }

  const activeTab = fileState.tabs.find((tab) => tab.id === activeTabId);
  const editorId =
    activeTab?.renderMode === "git-diff-editor"
      ? `${activeTabId}:git-modified`
      : activeTabId;
  const view = getActiveEditorView(editorId);
  if (!view) {
    return;
  }

  openSearchPanel(view);
  if (!openReplace) {
    return;
  }

  requestAnimationFrame(() => {
    const replaceInput = view.dom.querySelector<HTMLInputElement>("[name=replace]");
    replaceInput?.focus();
  });
}

export function executeNewTerminalTabShortcut(): void {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (!workspaceId) {
    return;
  }

  const workspaceState = useTerminalStore.getState().workspaces[workspaceId];
  if (!workspaceState || (workspaceState.layoutMode !== "split" && workspaceState.layoutMode !== "terminal")) {
    return;
  }

  void useTerminalStore.getState().createSession(workspaceId);
}

export function executeTerminalBroadcastShortcut(): void {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (!workspaceId) {
    return;
  }

  const workspaceState = useTerminalStore.getState().workspaces[workspaceId];
  if (!workspaceState || (workspaceState.layoutMode !== "split" && workspaceState.layoutMode !== "terminal")) {
    return;
  }

  const activeGroupId = workspaceState.activeGroupId;
  if (!activeGroupId) {
    return;
  }

  const activeGroup = workspaceState.groups.find((group) => group.id === activeGroupId);
  if (!activeGroup) {
    return;
  }

  const isBroadcastingActiveGroup = workspaceState.broadcastGroupId === activeGroupId;
  if (!isBroadcastingActiveGroup && collectSessionIds(activeGroup.root).length < 2) {
    return;
  }

  useTerminalStore.getState().toggleBroadcast(workspaceId, activeGroupId);
}

export function executeSplitTerminalShortcut(direction: "horizontal" | "vertical"): void {
  const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
  if (!workspaceId) {
    return;
  }

  const workspaceState = useTerminalStore.getState().workspaces[workspaceId];
  if (!workspaceState || (workspaceState.layoutMode !== "split" && workspaceState.layoutMode !== "terminal")) {
    return;
  }

  const sessionId = workspaceState.focusedSessionId;
  if (!sessionId) {
    return;
  }

  void useTerminalStore.getState().splitSession(workspaceId, sessionId, direction);
}

export async function executeCloseWindowShortcut(): Promise<void> {
  await requestWindowClose();
}
