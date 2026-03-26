import { create } from "zustand";
import type { Thread } from "../types";
import { useChatStore } from "./chatStore";
import { useCodexProfileStore } from "./codexProfileStore";
import { useThreadStore } from "./threadStore";
import { toast } from "./toastStore";
import { useUiStore } from "./uiStore";
import { useWorkspaceStore } from "./workspaceStore";

interface PersistedProjectTabsState {
  version: 1;
  openWorkspaceIds: string[];
  tabThreadIdsByWorkspace: Record<string, string[]>;
  activeThreadIdByWorkspace: Record<string, string | null>;
}

interface ProjectTabsState {
  openWorkspaceIds: string[];
  tabThreadIdsByWorkspace: Record<string, string[]>;
  activeThreadIdByWorkspace: Record<string, string | null>;
  trackWorkspaceVisit: (workspaceId: string | null) => void;
  trackActiveThread: (thread: Thread | null) => void;
  syncAvailableData: (workspaceIds: string[], threads: Thread[]) => void;
  switchToWorkspace: (workspaceId: string) => Promise<void>;
  switchToThread: (thread: Thread) => Promise<void>;
  closeWorkspace: (workspaceId: string) => Promise<void>;
  closeThreadTab: (workspaceId: string, threadId: string) => Promise<void>;
  switchToRelativeWorkspace: (direction: -1 | 1) => Promise<void>;
  switchToRelativeThread: (direction: -1 | 1) => Promise<void>;
  closeActiveThreadTab: () => Promise<void>;
}

const PROJECT_TABS_STORAGE_KEY = "supacodex:projectTabs:v1";

function dedupeStringArray(values: string[]): string[] {
  const seen = new Set<string>();
  const next: string[] = [];
  for (const value of values) {
    if (typeof value !== "string") {
      continue;
    }
    const normalized = value.trim();
    if (!normalized || seen.has(normalized)) {
      continue;
    }
    seen.add(normalized);
    next.push(normalized);
  }
  return next;
}

function normalizeTabThreadIdsByWorkspace(
  value: unknown,
): Record<string, string[]> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  const next: Record<string, string[]> = {};
  for (const [workspaceId, threadIds] of Object.entries(value as Record<string, unknown>)) {
    const normalizedWorkspaceId = workspaceId.trim();
    if (!normalizedWorkspaceId || !Array.isArray(threadIds)) {
      continue;
    }
    next[normalizedWorkspaceId] = dedupeStringArray(threadIds);
  }
  return next;
}

function normalizeActiveThreadIdsByWorkspace(
  value: unknown,
): Record<string, string | null> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  const next: Record<string, string | null> = {};
  for (const [workspaceId, threadId] of Object.entries(value as Record<string, unknown>)) {
    const normalizedWorkspaceId = workspaceId.trim();
    if (!normalizedWorkspaceId) {
      continue;
    }
    if (typeof threadId !== "string") {
      next[normalizedWorkspaceId] = null;
      continue;
    }
    const normalizedThreadId = threadId.trim();
    next[normalizedWorkspaceId] = normalizedThreadId || null;
  }
  return next;
}

function readPersistedProjectTabsState(): PersistedProjectTabsState {
  try {
    const raw = localStorage.getItem(PROJECT_TABS_STORAGE_KEY);
    if (!raw) {
      return {
        version: 1,
        openWorkspaceIds: [],
        tabThreadIdsByWorkspace: {},
        activeThreadIdByWorkspace: {},
      };
    }

    const parsed = JSON.parse(raw) as Partial<PersistedProjectTabsState> | null;
    if (!parsed || typeof parsed !== "object") {
      throw new Error("invalid project tabs state");
    }

    return {
      version: 1,
      openWorkspaceIds: Array.isArray(parsed.openWorkspaceIds)
        ? dedupeStringArray(parsed.openWorkspaceIds)
        : [],
      tabThreadIdsByWorkspace: normalizeTabThreadIdsByWorkspace(parsed.tabThreadIdsByWorkspace),
      activeThreadIdByWorkspace: normalizeActiveThreadIdsByWorkspace(
        parsed.activeThreadIdByWorkspace,
      ),
    };
  } catch {
    return {
      version: 1,
      openWorkspaceIds: [],
      tabThreadIdsByWorkspace: {},
      activeThreadIdByWorkspace: {},
    };
  }
}

function persistProjectTabsState(state: Pick<
  ProjectTabsState,
  "openWorkspaceIds" | "tabThreadIdsByWorkspace" | "activeThreadIdByWorkspace"
>): void {
  try {
    localStorage.setItem(
      PROJECT_TABS_STORAGE_KEY,
      JSON.stringify({
        version: 1,
        openWorkspaceIds: state.openWorkspaceIds,
        tabThreadIdsByWorkspace: state.tabThreadIdsByWorkspace,
        activeThreadIdByWorkspace: state.activeThreadIdByWorkspace,
      } satisfies PersistedProjectTabsState),
    );
  } catch {
    // Ignore persistence failures.
  }
}

function ensureWorkspaceTracked(workspaceIds: string[], workspaceId: string): string[] {
  if (workspaceIds.includes(workspaceId)) {
    return workspaceIds;
  }
  return [...workspaceIds, workspaceId];
}

function snapshotState(
  state: ProjectTabsState,
): Pick<ProjectTabsState, "openWorkspaceIds" | "tabThreadIdsByWorkspace" | "activeThreadIdByWorkspace"> {
  return {
    openWorkspaceIds: state.openWorkspaceIds,
    tabThreadIdsByWorkspace: state.tabThreadIdsByWorkspace,
    activeThreadIdByWorkspace: state.activeThreadIdByWorkspace,
  };
}

const initialState = readPersistedProjectTabsState();

export const useProjectTabsStore = create<ProjectTabsState>((set, get) => ({
  openWorkspaceIds: initialState.openWorkspaceIds,
  tabThreadIdsByWorkspace: initialState.tabThreadIdsByWorkspace,
  activeThreadIdByWorkspace: initialState.activeThreadIdByWorkspace,
  trackWorkspaceVisit: (workspaceId) => {
    if (!workspaceId) {
      return;
    }

    set((state) => {
      const openWorkspaceIds = ensureWorkspaceTracked(state.openWorkspaceIds, workspaceId);
      if (openWorkspaceIds === state.openWorkspaceIds) {
        return state;
      }

      const nextState = {
        ...state,
        openWorkspaceIds,
      };
      persistProjectTabsState(snapshotState(nextState));
      return nextState;
    });
  },
  trackActiveThread: (thread) => {
    if (!thread) {
      return;
    }

    set((state) => {
      const workspaceId = thread.workspaceId;
      const currentTabs = state.tabThreadIdsByWorkspace[workspaceId] ?? [];
      const nextTabs = currentTabs.includes(thread.id)
        ? currentTabs
        : [...currentTabs, thread.id];
      const openWorkspaceIds = ensureWorkspaceTracked(state.openWorkspaceIds, workspaceId);
      const activeThreadIdByWorkspace = {
        ...state.activeThreadIdByWorkspace,
        [workspaceId]: thread.id,
      };
      const tabThreadIdsByWorkspace = {
        ...state.tabThreadIdsByWorkspace,
        [workspaceId]: nextTabs,
      };
      const unchanged =
        state.openWorkspaceIds.length === openWorkspaceIds.length &&
        state.openWorkspaceIds.every((value, index) => value === openWorkspaceIds[index]) &&
        currentTabs.length === nextTabs.length &&
        currentTabs.every((value, index) => value === nextTabs[index]) &&
        state.activeThreadIdByWorkspace[workspaceId] === thread.id;
      if (unchanged) {
        return state;
      }

      const nextState = {
        ...state,
        openWorkspaceIds,
        tabThreadIdsByWorkspace,
        activeThreadIdByWorkspace,
      };
      persistProjectTabsState(snapshotState(nextState));
      return nextState;
    });
  },
  syncAvailableData: (workspaceIds, threads) => {
    const validWorkspaceIds = new Set(workspaceIds);
    const validThreadIdsByWorkspace = threads.reduce<Record<string, Set<string>>>((acc, thread) => {
      if (!acc[thread.workspaceId]) {
        acc[thread.workspaceId] = new Set<string>();
      }
      acc[thread.workspaceId].add(thread.id);
      return acc;
    }, {});

    set((state) => {
      const openWorkspaceIds = state.openWorkspaceIds.filter((workspaceId) =>
        validWorkspaceIds.has(workspaceId),
      );

      const tabThreadIdsByWorkspace = openWorkspaceIds.reduce<Record<string, string[]>>(
        (acc, workspaceId) => {
          const validThreadIds = validThreadIdsByWorkspace[workspaceId];
          const nextTabs = (state.tabThreadIdsByWorkspace[workspaceId] ?? []).filter((threadId) =>
            validThreadIds?.has(threadId),
          );
          acc[workspaceId] = nextTabs;
          return acc;
        },
        {},
      );

      const activeThreadIdByWorkspace = openWorkspaceIds.reduce<Record<string, string | null>>(
        (acc, workspaceId) => {
          const activeThreadId = state.activeThreadIdByWorkspace[workspaceId];
          const nextTabs = tabThreadIdsByWorkspace[workspaceId] ?? [];
          acc[workspaceId] =
            activeThreadId && nextTabs.includes(activeThreadId)
              ? activeThreadId
              : nextTabs[0] ?? null;
          return acc;
        },
        {},
      );

      const nextState = {
        ...state,
        openWorkspaceIds,
        tabThreadIdsByWorkspace,
        activeThreadIdByWorkspace,
      };
      persistProjectTabsState(snapshotState(nextState));
      return nextState;
    });
  },
  switchToWorkspace: async (workspaceId) => {
    get().trackWorkspaceVisit(workspaceId);

    if (useUiStore.getState().activeView !== "chat") {
      useUiStore.getState().setActiveView("chat");
    }

    if (useWorkspaceStore.getState().activeWorkspaceId !== workspaceId) {
      await useWorkspaceStore.getState().setActiveWorkspace(workspaceId);
    }

    const tabThreadIds = get().tabThreadIdsByWorkspace[workspaceId] ?? [];
    const targetThreadId = get().activeThreadIdByWorkspace[workspaceId] ?? tabThreadIds[0] ?? null;
    const targetThread =
      targetThreadId
        ? (
            useThreadStore
              .getState()
              .threads.find(
                (thread) => thread.id === targetThreadId && thread.workspaceId === workspaceId,
              ) ?? null
          )
        : null;

    if (targetThread) {
      await get().switchToThread(targetThread);
      return;
    }

    useThreadStore.getState().setActiveThread(null);
    await useChatStore.getState().setActiveThread(null);
  },
  switchToThread: async (thread) => {
    get().trackActiveThread(thread);

    if (useUiStore.getState().activeView !== "chat") {
      useUiStore.getState().setActiveView("chat");
    }

    if (useWorkspaceStore.getState().activeWorkspaceId !== thread.workspaceId) {
      await useWorkspaceStore.getState().setActiveWorkspace(thread.workspaceId);
    }

    if (thread.engineId === "codex") {
      try {
        await useCodexProfileStore.getState().ensureActiveProfileForThread(thread);
      } catch (error) {
        toast.error(String(error));
      }
    }

    if (thread.repoId) {
      useWorkspaceStore.getState().setActiveRepo(thread.repoId);
    } else {
      useWorkspaceStore.getState().setActiveRepo(null, { remember: false });
    }

    useThreadStore.getState().setActiveThread(thread.id);
    await useChatStore.getState().setActiveThread(thread.id);
  },
  closeWorkspace: async (workspaceId) => {
    const activeWorkspaceId = useWorkspaceStore.getState().activeWorkspaceId;
    const currentOpenWorkspaceIds = get().openWorkspaceIds;
    const currentIndex = currentOpenWorkspaceIds.indexOf(workspaceId);
    if (currentIndex < 0) {
      return;
    }

    const remainingWorkspaceIds = currentOpenWorkspaceIds.filter(
      (currentWorkspaceId) => currentWorkspaceId !== workspaceId,
    );
    const nextWorkspaceId =
      activeWorkspaceId === workspaceId
        ? (
            remainingWorkspaceIds[Math.min(currentIndex, remainingWorkspaceIds.length - 1)]
            ?? remainingWorkspaceIds[0]
            ?? null
          )
        : null;

    set((state) => {
      const tabThreadIdsByWorkspace = { ...state.tabThreadIdsByWorkspace };
      const activeThreadIdByWorkspace = { ...state.activeThreadIdByWorkspace };
      delete tabThreadIdsByWorkspace[workspaceId];
      delete activeThreadIdByWorkspace[workspaceId];

      const nextState = {
        ...state,
        openWorkspaceIds: remainingWorkspaceIds,
        tabThreadIdsByWorkspace,
        activeThreadIdByWorkspace,
      };
      persistProjectTabsState(snapshotState(nextState));
      return nextState;
    });

    if (nextWorkspaceId) {
      await get().switchToWorkspace(nextWorkspaceId);
      return;
    }

    if (activeWorkspaceId === workspaceId) {
      useThreadStore.getState().setActiveThread(null);
      await useChatStore.getState().setActiveThread(null);
    }
  },
  closeThreadTab: async (workspaceId, threadId) => {
    let nextThreadId: string | null = null;

    set((state) => {
      const currentTabs = state.tabThreadIdsByWorkspace[workspaceId] ?? [];
      const removedIndex = currentTabs.indexOf(threadId);
      if (removedIndex < 0) {
        return state;
      }

      const nextTabs = currentTabs.filter((currentThreadId) => currentThreadId !== threadId);
      nextThreadId = nextTabs[removedIndex] ?? nextTabs[removedIndex - 1] ?? nextTabs[0] ?? null;

      const nextState = {
        ...state,
        tabThreadIdsByWorkspace: {
          ...state.tabThreadIdsByWorkspace,
          [workspaceId]: nextTabs,
        },
        activeThreadIdByWorkspace: {
          ...state.activeThreadIdByWorkspace,
          [workspaceId]:
            state.activeThreadIdByWorkspace[workspaceId] === threadId
              ? nextThreadId
              : state.activeThreadIdByWorkspace[workspaceId] ?? null,
        },
      };
      persistProjectTabsState(snapshotState(nextState));
      return nextState;
    });

    const workspaceState = useWorkspaceStore.getState();
    const threadState = useThreadStore.getState();
    if (workspaceState.activeWorkspaceId !== workspaceId || threadState.activeThreadId !== threadId) {
      return;
    }

    const nextThread =
      nextThreadId
        ? (
            useThreadStore
              .getState()
              .threads.find(
                (thread) => thread.id === nextThreadId && thread.workspaceId === workspaceId,
              ) ?? null
          )
        : null;

    if (nextThread) {
      await get().switchToThread(nextThread);
      return;
    }

    useThreadStore.getState().setActiveThread(null);
    await useChatStore.getState().setActiveThread(null);
  },
  switchToRelativeWorkspace: async (direction) => {
    const { openWorkspaceIds } = get();
    if (openWorkspaceIds.length < 2) {
      return;
    }

    const activeWorkspaceId = useWorkspaceStore.getState().activeWorkspaceId;
    const currentIndex = activeWorkspaceId
      ? openWorkspaceIds.indexOf(activeWorkspaceId)
      : -1;
    const normalizedCurrentIndex = currentIndex >= 0 ? currentIndex : 0;
    const nextIndex =
      (normalizedCurrentIndex + direction + openWorkspaceIds.length) % openWorkspaceIds.length;
    const nextWorkspaceId = openWorkspaceIds[nextIndex];
    if (!nextWorkspaceId || nextWorkspaceId === activeWorkspaceId) {
      return;
    }

    await get().switchToWorkspace(nextWorkspaceId);
  },
  switchToRelativeThread: async (direction) => {
    const activeWorkspaceId = useWorkspaceStore.getState().activeWorkspaceId;
    if (!activeWorkspaceId) {
      return;
    }

    const threadIds = get().tabThreadIdsByWorkspace[activeWorkspaceId] ?? [];
    if (threadIds.length < 2) {
      return;
    }

    const activeThreadId = useThreadStore.getState().activeThreadId;
    const currentIndex = activeThreadId ? threadIds.indexOf(activeThreadId) : -1;
    const normalizedCurrentIndex = currentIndex >= 0 ? currentIndex : 0;
    const nextIndex = (normalizedCurrentIndex + direction + threadIds.length) % threadIds.length;
    const nextThreadId = threadIds[nextIndex];
    if (!nextThreadId || nextThreadId === activeThreadId) {
      return;
    }

    const nextThread =
      useThreadStore
        .getState()
        .threads.find(
          (thread) => thread.id === nextThreadId && thread.workspaceId === activeWorkspaceId,
        ) ?? null;
    if (!nextThread) {
      return;
    }

    await get().switchToThread(nextThread);
  },
  closeActiveThreadTab: async () => {
    const activeWorkspaceId = useWorkspaceStore.getState().activeWorkspaceId;
    const activeThreadId = useThreadStore.getState().activeThreadId;
    if (!activeWorkspaceId || !activeThreadId) {
      return;
    }

    const openThreadIds = get().tabThreadIdsByWorkspace[activeWorkspaceId] ?? [];
    if (!openThreadIds.includes(activeThreadId)) {
      return;
    }

    await get().closeThreadTab(activeWorkspaceId, activeThreadId);
  },
}));
