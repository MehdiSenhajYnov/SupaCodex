import { create } from "zustand";
import {
  COMMAND_PALETTE_DEFAULT_LAUNCH,
  type CommandPaletteLaunchState,
} from "../lib/commandPalette";

interface MessageFocusTarget {
  threadId: string;
  messageId: string;
  requestedAt: number;
}

interface FocusModeSnapshot {
  showSidebar: boolean;
  showGitPanel: boolean;
}

type ActiveView = "chat" | "harnesses" | "workspace-settings";

interface PersistedUiShellState {
  version: 1;
  showSidebar: boolean;
  showGitPanel: boolean;
  activeView: ActiveView;
}

const UI_SHELL_STORAGE_KEY = "supacodex:uiShell:v1";

interface UiState {
  showSidebar: boolean;
  showGitPanel: boolean;
  focusMode: boolean;
  focusModeSnapshot: FocusModeSnapshot | null;
  activeView: ActiveView;
  settingsWorkspaceId: string | null;
  commandPaletteOpen: boolean;
  commandPaletteLaunch: CommandPaletteLaunchState;
  messageFocusTarget: MessageFocusTarget | null;
  openCommandPalette: (launch?: Partial<CommandPaletteLaunchState>) => void;
  closeCommandPalette: () => void;
  toggleSidebar: () => void;
  toggleGitPanel: () => void;
  setFocusMode: (enabled: boolean) => void;
  toggleFocusMode: () => void;
  setActiveView: (view: ActiveView) => void;
  openWorkspaceSettings: (workspaceId: string) => void;
  setMessageFocusTarget: (target: { threadId: string; messageId: string }) => void;
  clearMessageFocusTarget: () => void;
}

function normalizePersistedActiveView(value: unknown): ActiveView {
  return value === "harnesses" ? "harnesses" : "chat";
}

function readPersistedUiShellState(): PersistedUiShellState {
  try {
    const raw = localStorage.getItem(UI_SHELL_STORAGE_KEY);
    if (!raw) {
      return {
        version: 1,
        showSidebar: true,
        showGitPanel: true,
        activeView: "chat",
      };
    }

    const parsed = JSON.parse(raw) as Partial<PersistedUiShellState> | null;
    if (!parsed || typeof parsed !== "object") {
      throw new Error("invalid ui shell state");
    }

    return {
      version: 1,
      showSidebar: parsed.showSidebar !== false,
      showGitPanel: parsed.showGitPanel !== false,
      activeView: normalizePersistedActiveView(parsed.activeView),
    };
  } catch {
    return {
      version: 1,
      showSidebar: true,
      showGitPanel: true,
      activeView: "chat",
    };
  }
}

function persistUiShellState(state: Pick<UiState, "showSidebar" | "showGitPanel" | "activeView">): void {
  try {
    localStorage.setItem(
      UI_SHELL_STORAGE_KEY,
      JSON.stringify({
        version: 1,
        showSidebar: state.showSidebar,
        showGitPanel: state.showGitPanel,
        activeView:
          state.activeView === "workspace-settings"
            ? "chat"
            : state.activeView,
      } satisfies PersistedUiShellState),
    );
  } catch {
    // Ignore storage failures in non-browser/test environments.
  }
}

const persistedUiShellState = readPersistedUiShellState();

export const useUiStore = create<UiState>((set) => ({
  showSidebar: persistedUiShellState.showSidebar,
  showGitPanel: persistedUiShellState.showGitPanel,
  focusMode: false,
  focusModeSnapshot: null,
  commandPaletteOpen: false,
  commandPaletteLaunch: COMMAND_PALETTE_DEFAULT_LAUNCH,
  activeView: persistedUiShellState.activeView,
  settingsWorkspaceId: null,
  messageFocusTarget: null,
  openCommandPalette: (launch) =>
    set({
      commandPaletteOpen: true,
      commandPaletteLaunch: {
        ...COMMAND_PALETTE_DEFAULT_LAUNCH,
        ...launch,
      },
    }),
  closeCommandPalette: () =>
    set({
      commandPaletteOpen: false,
      commandPaletteLaunch: COMMAND_PALETTE_DEFAULT_LAUNCH,
    }),
  toggleSidebar: () =>
    set((state) => {
      const nextState = {
        showSidebar: !state.showSidebar,
      };
      if (!state.focusMode) {
        persistUiShellState({
          ...state,
          ...nextState,
        });
      }
      return nextState;
    }),
  toggleGitPanel: () =>
    set((state) => {
      const nextState = {
        showGitPanel: !state.showGitPanel,
      };
      if (!state.focusMode) {
        persistUiShellState({
          ...state,
          ...nextState,
        });
      }
      return nextState;
    }),
  setFocusMode: (enabled) =>
    set((state) => {
      if (enabled) {
        if (state.focusMode) {
          return state;
        }
        return {
          focusMode: true,
          focusModeSnapshot: {
            showSidebar: state.showSidebar,
            showGitPanel: state.showGitPanel,
          },
          showSidebar: false,
        };
      }

      if (!state.focusMode) {
        return state;
      }

      const snapshot = state.focusModeSnapshot;
      return {
        focusMode: false,
        focusModeSnapshot: null,
        showSidebar: snapshot?.showSidebar ?? state.showSidebar,
        showGitPanel: snapshot?.showGitPanel ?? state.showGitPanel,
      };
    }),
  toggleFocusMode: () =>
    set((state) => {
      if (!state.focusMode) {
        return {
          focusMode: true,
          focusModeSnapshot: {
            showSidebar: state.showSidebar,
            showGitPanel: state.showGitPanel,
          },
          showSidebar: false,
        };
      }

      const snapshot = state.focusModeSnapshot;
      return {
        focusMode: false,
        focusModeSnapshot: null,
        showSidebar: snapshot?.showSidebar ?? state.showSidebar,
        showGitPanel: snapshot?.showGitPanel ?? state.showGitPanel,
      };
    }),
  setActiveView: (view) => {
    set((state) => {
      persistUiShellState({
        ...state,
        activeView: view,
      });
      return { activeView: view };
    });
    if (view === "harnesses") {
      // Lazy import to avoid circular dependency
      void import("./harnessStore").then(({ useHarnessStore }) => {
        void useHarnessStore.getState().scan();
      });
    }
  },
  openWorkspaceSettings: (workspaceId) => {
    set({ activeView: "workspace-settings", settingsWorkspaceId: workspaceId });
  },
  setMessageFocusTarget: (target) =>
    set({
      messageFocusTarget: {
        ...target,
        requestedAt: Date.now(),
      },
    }),
  clearMessageFocusTarget: () => set({ messageFocusTarget: null }),
}));
