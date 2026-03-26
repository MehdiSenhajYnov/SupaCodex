import { useEffect, useMemo, type CSSProperties } from "react";
import { ThreeColumnLayout } from "./components/layout/ThreeColumnLayout";
import { CommandPalette } from "./components/shared/CommandPalette";
import { OnboardingWizard } from "./components/onboarding/OnboardingWizard";
import { ToastContainer } from "./components/shared/ToastContainer";
import { PowerSettingsModal } from "./components/shared/PowerSettingsModal";
import { TerminalNotificationSettingsModal } from "./components/shared/TerminalNotificationSettingsModal";
import { CodexProfilesModal } from "./components/shared/CodexProfilesModal";
import { AppearanceSettingsModal } from "./components/shared/AppearanceSettingsModal";
import { ShortcutSettingsModal } from "./components/shared/ShortcutSettingsModal";
import { t } from "./i18n";
import { useUpdateStore } from "./stores/updateStore";
import { useHarnessStore } from "./stores/harnessStore";
import {
  ipc,
  listenChatTurnFinished,
  listenEngineRuntimeUpdated,
  listenMenuAction,
  listenThreadUpdated,
} from "./lib/ipc";
import { useWorkspaceStore } from "./stores/workspaceStore";
import { useEngineStore } from "./stores/engineStore";
import { useUiStore } from "./stores/uiStore";
import { useThreadStore } from "./stores/threadStore";
import { useGitStore } from "./stores/gitStore";
import { useTerminalStore } from "./stores/terminalStore";
import { useKeepAwakeStore } from "./stores/keepAwakeStore";
import { useTerminalNotificationSettingsStore } from "./stores/terminalNotificationSettingsStore";
import { useCodexProfileStore } from "./stores/codexProfileStore";
import { DEFAULT_APPEARANCE_SETTINGS, useAppearanceStore } from "./stores/appearanceStore";
import { useProjectTabsStore } from "./stores/projectTabsStore";
import { useShortcutStore } from "./stores/shortcutStore";
import { toast } from "./stores/toastStore";
import type { RuntimeToast, Thread } from "./types";
import { CustomWindowFrame } from "./components/shared/CustomWindowFrame";
import { useCustomWindowFrameState } from "./lib/customWindowFrame";
import { runEditMenuAction } from "./lib/nativeEditActions";
import {
  SHORTCUT_ACTION_DEFINITIONS,
  type ShortcutActionId,
  getEffectiveShortcutBinding,
  matchesShortcutBinding,
} from "./lib/shortcutBindings";
import {
  executeCloseWindowShortcut,
  executeEditorFindShortcut,
  executeNewTerminalTabShortcut,
  executeShortcutAction,
  executeSplitTerminalShortcut,
  executeTerminalBroadcastShortcut,
} from "./lib/shortcutActions";
import {
  usesCustomWindowFrame,
  isTerminalInputFocused,
  shouldHandleAppShortcutWhileTerminalFocused,
} from "./lib/windowActions";

// Debounce guard: when both the JS keydown handler and the native menu-action
// fire for the same shortcut, only the first one within 100ms takes effect.
const shortcutLastFired = new Map<string, number>();
const SHORTCUT_DEBOUNCE_MS = 100;
const KEEP_AWAKE_REFRESH_MS = 15000;
const INTERFACE_ZOOM_MIN = 80;
const INTERFACE_ZOOM_MAX = 160;
const INTERFACE_ZOOM_STEP = 10;
const INTERFACE_ZOOM_WHEEL_THRESHOLD = 120;

function fireShortcut(id: string, action: () => void) {
  const now = Date.now();
  const last = shortcutLastFired.get(id) ?? 0;
  if (now - last < SHORTCUT_DEBOUNCE_MS) return;
  shortcutLastFired.set(id, now);
  action();
}

function clampInterfaceZoom(value: number): number {
  return Math.min(INTERFACE_ZOOM_MAX, Math.max(INTERFACE_ZOOM_MIN, Math.round(value)));
}

function stepInterfaceZoom(delta: number): void {
  const appearanceStore = useAppearanceStore.getState();
  const nextZoom = clampInterfaceZoom(appearanceStore.interfaceZoom + delta);
  if (nextZoom === appearanceStore.interfaceZoom) {
    return;
  }
  appearanceStore.patchSettings({ interfaceZoom: nextZoom });
}

function resetInterfaceZoom(): void {
  const appearanceStore = useAppearanceStore.getState();
  if (appearanceStore.interfaceZoom === DEFAULT_APPEARANCE_SETTINGS.interfaceZoom) {
    return;
  }
  appearanceStore.patchSettings({ interfaceZoom: DEFAULT_APPEARANCE_SETTINGS.interfaceZoom });
}

function isCodexSyncRequired(thread: Thread | null | undefined): boolean {
  return thread?.engineId === "codex" && thread.engineMetadata?.codexSyncRequired === true;
}

function showRuntimeToast(runtimeToast?: RuntimeToast) {
  if (!runtimeToast) {
    return;
  }

  switch (runtimeToast.variant) {
    case "success":
      toast.success(runtimeToast.message);
      break;
    case "warning":
      toast.warning(runtimeToast.message);
      break;
    case "info":
      toast.info(runtimeToast.message);
      break;
    case "error":
    default:
      toast.error(runtimeToast.message);
      break;
  }
}

function resolveAgentDisplayName(engineId: "codex" | "claude"): string {
  return engineId === "claude" ? "Claude" : "Codex";
}

function resolveChatNotificationBody(
  status: "completed" | "interrupted" | "error",
  preview?: string | null,
): string {
  const normalizedPreview = preview?.trim();
  if (normalizedPreview) {
    return normalizedPreview;
  }
  if (status === "error") {
    return t("app:notificationSettings.chatNotificationFallbackError");
  }
  return t("app:notificationSettings.chatNotificationFallbackComplete");
}

export function App() {
  const loadWorkspaces = useWorkspaceStore((s) => s.loadWorkspaces);
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspacesLoadedOnce = useWorkspaceStore((s) => s.loadedOnce);
  const loadEngines = useEngineStore((s) => s.load);
  const applyEngineRuntimeUpdate = useEngineStore((s) => s.applyRuntimeUpdate);
  const scanHarnesses = useHarnessStore((s) => s.scan);
  const loadKeepAwake = useKeepAwakeStore((s) => s.load);
  const loadTerminalNotificationSettings = useTerminalNotificationSettingsStore((s) => s.load);
  const loadCodexProfiles = useCodexProfileStore((s) => s.load);
  const refreshDetectedProjects = useCodexProfileStore((s) => s.refreshDetectedProjects);
  const refreshKeepAwake = useKeepAwakeStore((s) => s.refresh);
  const keepAwakeEnabled = useKeepAwakeStore((s) => s.state?.enabled ?? false);
  const keepAwakeSessionTimer = useKeepAwakeStore((s) => s.state?.sessionRemainingSecs);
  const refreshAllThreads = useThreadStore((s) => s.refreshAllThreads);
  const refreshThreads = useThreadStore((s) => s.refreshThreads);
  const refreshArchivedThreads = useThreadStore((s) => s.refreshArchivedThreads);
  const applyThreadUpdateLocal = useThreadStore((s) => s.applyThreadUpdateLocal);
  const threads = useThreadStore((s) => s.threads);
  const activeThreadId = useThreadStore((s) => s.activeThreadId);
  const threadsLoadedOnce = useThreadStore((s) => s.loadedOnce);
  const syncProjectTabs = useProjectTabsStore((s) => s.syncAvailableData);
  const trackProjectWorkspaceVisit = useProjectTabsStore((s) => s.trackWorkspaceVisit);
  const trackProjectActiveThread = useProjectTabsStore((s) => s.trackActiveThread);
  const activeProjectThreadIdsByWorkspace = useProjectTabsStore((s) => s.activeThreadIdByWorkspace);
  const projectTabThreadIdsByWorkspace = useProjectTabsStore((s) => s.tabThreadIdsByWorkspace);
  const switchProjectThread = useProjectTabsStore((s) => s.switchToThread);
  const commandPaletteOpen = useUiStore((s) => s.commandPaletteOpen);
  const closeCommandPalette = useUiStore((s) => s.closeCommandPalette);
  const checkForUpdate = useUpdateStore((s) => s.checkForUpdate);
  const customWindowFrame = usesCustomWindowFrame();
  const customWindowFrameState = useCustomWindowFrameState();
  const interfaceZoom = useAppearanceStore((s) => s.interfaceZoom);
  const windowRadius = useAppearanceStore((s) => s.windowRadius);
  const windowGap = useAppearanceStore((s) => s.windowGap);
  const surfaceBlur = useAppearanceStore((s) => s.surfaceBlur);
  const transparentSidebar = useAppearanceStore((s) => s.transparentSidebar);
  const transparentContent = useAppearanceStore((s) => s.transparentContent);
  const transparentTerminal = useAppearanceStore((s) => s.transparentTerminal);
  const shortcutOverrides = useShortcutStore((s) => s.overrides);
  const shortcutBindingsByAction = useMemo(
    () =>
      SHORTCUT_ACTION_DEFINITIONS.reduce<Record<ShortcutActionId, string | null>>((acc, definition) => {
        acc[definition.id] = getEffectiveShortcutBinding(definition.id, shortcutOverrides);
        return acc;
      }, {} as Record<ShortcutActionId, string | null>),
    [shortcutOverrides],
  );
  const activeWorkspaceTitle = useMemo(() => {
    const activeWorkspace = workspaces.find((workspace) => workspace.id === activeWorkspaceId) ?? null;
    if (!activeWorkspace) {
      return null;
    }
    return activeWorkspace.name || activeWorkspace.rootPath.split("/").pop() || null;
  }, [activeWorkspaceId, workspaces]);

  useEffect(() => {
    void loadWorkspaces();
    void loadEngines();
    void scanHarnesses();
    void loadKeepAwake();
    void loadTerminalNotificationSettings();
    void loadCodexProfiles();
  }, [
    loadCodexProfiles,
    loadEngines,
    loadKeepAwake,
    loadTerminalNotificationSettings,
    loadWorkspaces,
    scanHarnesses,
  ]);

  useEffect(() => {
    void refreshAllThreads(workspaces.map((workspace) => workspace.id));
  }, [workspaces, refreshAllThreads]);

  useEffect(() => {
    void refreshDetectedProjects();
  }, [refreshDetectedProjects, workspaces]);

  useEffect(() => {
    const hasSessionTimer = keepAwakeSessionTimer != null;
    if (!keepAwakeEnabled && !hasSessionTimer) {
      return;
    }

    const pollInterval = hasSessionTimer ? 30_000 : KEEP_AWAKE_REFRESH_MS;
    const intervalId = window.setInterval(() => {
      void refreshKeepAwake();
    }, pollInterval);

    return () => window.clearInterval(intervalId);
  }, [keepAwakeEnabled, keepAwakeSessionTimer, refreshKeepAwake]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenThreadUpdated(async ({ workspaceId, thread }) => {
      if (thread) {
        const applied = applyThreadUpdateLocal(thread);
        const activeThreadId = useThreadStore.getState().activeThreadId;
        if (thread.id === activeThreadId && isCodexSyncRequired(thread)) {
          try {
            const syncedThread = await ipc.syncThreadFromEngine(thread.id);
            if (useThreadStore.getState().applyThreadUpdateLocal(syncedThread)) {
              return;
            }
          } catch (error) {
            console.warn(`Failed to sync active Codex thread ${thread.id}:`, error);
          }
          void refreshThreads(workspaceId);
          void refreshArchivedThreads(workspaceId);
          return;
        }
        if (applied) {
          return;
        }
      }
      void refreshThreads(workspaceId);
      void refreshArchivedThreads(workspaceId);
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [applyThreadUpdateLocal, refreshArchivedThreads, refreshThreads]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenChatTurnFinished(async (event) => {
      const notificationStore = useTerminalNotificationSettingsStore.getState();
      const settings = notificationStore.settings ?? await notificationStore.load();
      if (!settings?.chatEnabled || event.status === "interrupted") {
        return;
      }

      const activeWorkspaceId = useWorkspaceStore.getState().activeWorkspaceId;
      const activeThreadId = useThreadStore.getState().activeThreadId;
      if (
        document.hasFocus()
        && activeWorkspaceId === event.workspaceId
        && activeThreadId === event.threadId
      ) {
        return;
      }

      const title = event.threadTitle.trim() || resolveAgentDisplayName(event.engineId);
      const body = resolveChatNotificationBody(event.status, event.preview);

      try {
        await ipc.showAgentNotification(title, body);
      } catch (error) {
        console.warn(`Failed to show chat notification for thread ${event.threadId}:`, error);
      }
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenEngineRuntimeUpdated((event) => {
      applyEngineRuntimeUpdate(event);
      showRuntimeToast(event.toast);
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [applyEngineRuntimeUpdate]);

  useEffect(() => {
    function onBeforeUnload() {
      const wsId = useWorkspaceStore.getState().activeWorkspaceId;
      if (wsId) {
        useGitStore.getState().flushDrafts(wsId);
      }
    }

    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      void checkForUpdate();
    }, 3000);
    return () => clearTimeout(timer);
  }, [checkForUpdate]);

  useEffect(() => {
    document.documentElement.style.setProperty("zoom", String(interfaceZoom / 100));
  }, [interfaceZoom]);

  // Handle app-level keyboard shortcuts via JavaScript keydown listeners.
  // On macOS, when a contenteditable element (CodeMirror editor) is focused,
  // WKWebView claims Cmd+key events for text formatting before they reach
  // Tauri's native menu accelerators. JavaScript keydown events still fire,
  // so the JS handler is the primary source of truth for these shortcuts.
  //
  // When the native menu accelerator DOES fire (non-contenteditable focus),
  // both the JS handler and the menu-action listener would toggle the same
  // state, canceling each other out. A debounce guard (`shortcutLastFired`)
  // prevents the second handler from re-toggling within 100ms.
  //
  // Customizable app shortcuts are matched first so they behave consistently
  // across the custom frame menu, the settings UI, and JS keyboard handling.
  // Cmd+S always prevents the browser save-page dialog.
  // Cmd+W is debounced like the native menu path so Linux can use the same
  // close behavior even without a native menubar.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      for (const definition of SHORTCUT_ACTION_DEFINITIONS) {
        const binding = shortcutBindingsByAction[definition.id];
        if (!binding || !matchesShortcutBinding(e, binding)) {
          continue;
        }

        e.preventDefault();
        fireShortcut(definition.id, () => {
          void executeShortcutAction(definition.id);
        });
        return;
      }

      const meta = e.metaKey || e.ctrlKey;
      if (!meta) return;

      const key = e.key.toLowerCase();
      if ((key === "=" || key === "+" || e.code === "NumpadAdd") && !e.altKey) {
        e.preventDefault();
        stepInterfaceZoom(INTERFACE_ZOOM_STEP);
        return;
      }
      if ((key === "-" || e.code === "NumpadSubtract") && !e.altKey) {
        e.preventDefault();
        stepInterfaceZoom(-INTERFACE_ZOOM_STEP);
        return;
      }
      if ((key === "0" || e.code === "Numpad0") && !e.altKey) {
        e.preventDefault();
        resetInterfaceZoom();
        return;
      }

      // On macOS/WebKit, e.key is lowercase even when Shift is held with Cmd,
      // so normalize to lowercase and use e.shiftKey to differentiate.
      const allowWhileTerminalFocused = shouldHandleAppShortcutWhileTerminalFocused(
        key,
        e.shiftKey,
        e.altKey,
      );

      if (isTerminalInputFocused() && !allowWhileTerminalFocused) return;

      // Always prevent Cmd+S from opening the browser save dialog
      if (key === "s" && !e.shiftKey) {
        e.preventDefault();
        return;
      }

      switch (key) {
        case "f": {
          if (!e.shiftKey) {
            // Cmd+F — editor find (only in editor mode)
            e.preventDefault();
            executeEditorFindShortcut(false);
            return;
          }
          break;
        }
        case "h": {
          if (e.shiftKey) return;
          // Cmd+H — editor find & replace (only in editor mode)
          e.preventDefault();
          executeEditorFindShortcut(true);
          break;
        }
        case "t":
          if (e.shiftKey) return;
          e.preventDefault();
          fireShortcut("new-terminal-tab", () => {
            executeNewTerminalTabShortcut();
          });
          break;
        case "w":
          if (e.shiftKey) return;
          e.preventDefault();
          fireShortcut("close-window", () => {
            void executeCloseWindowShortcut();
          });
          break;
        case "i":
          if (!e.shiftKey) return;
          e.preventDefault();
          fireShortcut("toggle-broadcast", () => {
            executeTerminalBroadcastShortcut();
          });
          break;
        case "d":
          e.preventDefault();
          fireShortcut(e.shiftKey ? "split-horizontal" : "split-vertical", () => {
            executeSplitTerminalShortcut(e.shiftKey ? "horizontal" : "vertical");
          });
          break;
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [shortcutBindingsByAction]);

  useEffect(() => {
    let wheelAccumulator = 0;
    let accumulatorResetTimer: number | null = null;

    function scheduleAccumulatorReset() {
      if (accumulatorResetTimer != null) {
        window.clearTimeout(accumulatorResetTimer);
      }
      accumulatorResetTimer = window.setTimeout(() => {
        wheelAccumulator = 0;
        accumulatorResetTimer = null;
      }, 180);
    }

    function onWheel(event: WheelEvent) {
      if (!event.ctrlKey && !event.metaKey) {
        return;
      }

      event.preventDefault();
      if (!Number.isFinite(event.deltaY) || event.deltaY === 0) {
        return;
      }

      wheelAccumulator += event.deltaY;
      scheduleAccumulatorReset();

      while (wheelAccumulator >= INTERFACE_ZOOM_WHEEL_THRESHOLD) {
        stepInterfaceZoom(-INTERFACE_ZOOM_STEP);
        wheelAccumulator -= INTERFACE_ZOOM_WHEEL_THRESHOLD;
      }

      while (wheelAccumulator <= -INTERFACE_ZOOM_WHEEL_THRESHOLD) {
        stepInterfaceZoom(INTERFACE_ZOOM_STEP);
        wheelAccumulator += INTERFACE_ZOOM_WHEEL_THRESHOLD;
      }
    }

    window.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      if (accumulatorResetTimer != null) {
        window.clearTimeout(accumulatorResetTimer);
      }
      window.removeEventListener("wheel", onWheel);
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenMenuAction((action) => {
      switch (action) {
        case "toggle-sidebar":
        case "toggle-git-panel":
        case "toggle-focus-mode":
        case "toggle-fullscreen":
        case "toggle-search":
        case "toggle-terminal":
        case "previous-conversation":
        case "next-conversation":
        case "previous-open-project":
        case "next-open-project":
        case "close-conversation":
          fireShortcut(action, () => {
            void executeShortcutAction(action as ShortcutActionId);
          });
          break;
        case "close-window": {
          void executeCloseWindowShortcut();
          break;
        }
        case "edit-undo":
        case "edit-redo":
        case "edit-cut":
        case "edit-copy":
        case "edit-paste":
        case "edit-select-all":
          void runEditMenuAction(action).catch((error) => {
            if (import.meta.env.DEV) {
              console.warn("[App] Failed to execute edit menu action", action, error);
            }
          });
          break;
      }
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, []);

  const effectiveWindowGap =
    customWindowFrameState.isFullscreen || customWindowFrameState.isMaximized
      ? 0
      : windowGap;
  const effectiveWindowRadius =
    customWindowFrameState.isFullscreen || customWindowFrameState.isMaximized
      ? 0
      : windowRadius;
  const effectiveSurfaceRadius = Math.max(effectiveWindowRadius - effectiveWindowGap, 0);
  const activeThread = threads.find((thread) => thread.id === activeThreadId) ?? null;

  useEffect(() => {
    if (!workspacesLoadedOnce || !threadsLoadedOnce) {
      return;
    }
    syncProjectTabs(
      workspaces.map((workspace) => workspace.id),
      threads,
    );
  }, [syncProjectTabs, threads, threadsLoadedOnce, workspaces, workspacesLoadedOnce]);

  useEffect(() => {
    trackProjectWorkspaceVisit(activeWorkspaceId);
  }, [activeWorkspaceId, trackProjectWorkspaceVisit]);

  useEffect(() => {
    trackProjectActiveThread(activeThread);
  }, [activeThread, trackProjectActiveThread]);

  useEffect(() => {
    if (!workspacesLoadedOnce || !threadsLoadedOnce) {
      return;
    }
    if (!activeWorkspaceId || threads.length === 0) {
      return;
    }

    if (activeThread?.workspaceId === activeWorkspaceId) {
      return;
    }

    const targetThreadId =
      activeProjectThreadIdsByWorkspace[activeWorkspaceId]
      ?? projectTabThreadIdsByWorkspace[activeWorkspaceId]?.[0]
      ?? null;
    if (!targetThreadId) {
      return;
    }

    const targetThread =
      threads.find(
        (thread) =>
          thread.id === targetThreadId && thread.workspaceId === activeWorkspaceId,
      ) ?? null;
    if (!targetThread) {
      return;
    }

    void switchProjectThread(targetThread);
  }, [
    activeProjectThreadIdsByWorkspace,
    activeThread?.workspaceId,
    activeWorkspaceId,
    projectTabThreadIdsByWorkspace,
    switchProjectThread,
    threadsLoadedOnce,
    threads,
    workspacesLoadedOnce,
  ]);

  const shellStyle = {
    "--surface-blur-custom": `${surfaceBlur}px`,
    ...(customWindowFrame
      ? {
          padding: effectiveWindowGap,
          borderRadius: effectiveWindowRadius,
          clipPath: `inset(0 round ${effectiveWindowRadius}px)`,
          WebkitClipPath: `inset(0 round ${effectiveWindowRadius}px)`,
        }
      : null),
  } as CSSProperties;
  const shellSurfaceStyle = {
    margin: customWindowFrame ? 0 : effectiveWindowGap,
    borderRadius: customWindowFrame ? effectiveSurfaceRadius : effectiveWindowRadius,
  } as CSSProperties;

  return (
    <div
      className={`app-shell${customWindowFrame ? " app-shell-custom-frame" : ""}${
        customWindowFrameState.isMaximized ? " app-shell-custom-frame-maximized" : ""
      }${customWindowFrameState.isFullscreen ? " app-shell-custom-frame-fullscreen" : ""}${
        transparentSidebar ? " app-shell-sidebar-transparent" : ""
      }${transparentContent ? " app-shell-content-transparent" : ""}${
        transparentTerminal ? " app-shell-terminal-transparent" : ""
      }`}
      style={shellStyle}
    >
      <div className="app-shell-surface" style={shellSurfaceStyle}>
        {customWindowFrame && (
          <CustomWindowFrame
            frameState={customWindowFrameState}
            centerTitle={activeWorkspaceTitle}
          />
        )}
        <div className="app-shell-body">
          <ThreeColumnLayout />
        </div>
      </div>
      <CommandPalette open={commandPaletteOpen} onClose={closeCommandPalette} />
      <PowerSettingsModal />
      <TerminalNotificationSettingsModal />
      <CodexProfilesModal />
      <AppearanceSettingsModal />
      <ShortcutSettingsModal />
      <OnboardingWizard />
      <ToastContainer />
    </div>
  );
}
