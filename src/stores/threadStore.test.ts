import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Thread } from "../types";

const mockIpc = vi.hoisted(() => ({
  attachCodexRemoteThread: vi.fn(),
  deleteThread: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  ipc: mockIpc,
}));

import { useThreadStore } from "./threadStore";

function makeThread(overrides: Partial<Thread> = {}): Thread {
  return {
    id: "thread-1",
    workspaceId: "workspace-1",
    repoId: null,
    engineId: "codex",
    modelId: "gpt-5.3-codex",
    engineThreadId: "engine-thread-1",
    engineMetadata: undefined,
    title: "Thread 1",
    status: "idle",
    messageCount: 0,
    totalTokens: 0,
    createdAt: new Date(0).toISOString(),
    lastActivityAt: new Date(0).toISOString(),
    ...overrides,
  };
}

describe("threadStore.attachCodexRemoteThread", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("localStorage", {
      getItem: vi.fn(() => null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
    });

    useThreadStore.setState({
      threads: [],
      threadsByWorkspace: {},
      archivedThreadsByWorkspace: {},
      activeThreadId: "thread-active",
      loadedOnce: false,
      loading: false,
      error: undefined,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps the current selection when attaching a detected thread in silent mode", async () => {
    const attached = makeThread({
      id: "attached-1",
      workspaceId: "workspace-1",
      engineThreadId: "engine-remote-1",
    });
    mockIpc.attachCodexRemoteThread.mockResolvedValue(attached);

    const result = await useThreadStore
      .getState()
      .attachCodexRemoteThread("workspace-1", "engine-remote-1", "gpt-5.3-codex", {
        activate: false,
      });

    expect(result).toEqual(attached);
    expect(useThreadStore.getState().activeThreadId).toBe("thread-active");
    expect(localStorage.setItem).not.toHaveBeenCalled();
    expect(useThreadStore.getState().threadsByWorkspace["workspace-1"]).toEqual([attached]);
  });

  it("activates the attached thread by default", async () => {
    const attached = makeThread({
      id: "attached-2",
      workspaceId: "workspace-1",
      engineThreadId: "engine-remote-2",
    });
    mockIpc.attachCodexRemoteThread.mockResolvedValue(attached);

    const result = await useThreadStore
      .getState()
      .attachCodexRemoteThread("workspace-1", "engine-remote-2", "gpt-5.3-codex");

    expect(result).toEqual(attached);
    expect(useThreadStore.getState().activeThreadId).toBe(attached.id);
    expect(localStorage.setItem).toHaveBeenCalledWith(
      "supacodex:lastActiveThreadId",
      attached.id,
    );
  });
});

describe("threadStore.deleteThread", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("localStorage", {
      getItem: vi.fn(() => null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
      clear: vi.fn(),
    });

    useThreadStore.setState({
      threads: [],
      threadsByWorkspace: {},
      archivedThreadsByWorkspace: {},
      activeThreadId: null,
      loadedOnce: false,
      loading: false,
      error: undefined,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("removes the deleted thread from active and archived collections", async () => {
    const activeThread = makeThread({
      id: "thread-active",
      workspaceId: "workspace-1",
    });
    const archivedThread = makeThread({
      id: "thread-archived",
      workspaceId: "workspace-1",
    });

    useThreadStore.setState({
      threads: [activeThread],
      threadsByWorkspace: {
        "workspace-1": [activeThread],
      },
      archivedThreadsByWorkspace: {
        "workspace-1": [archivedThread],
      },
      activeThreadId: activeThread.id,
      loadedOnce: true,
      loading: false,
      error: undefined,
    });

    await useThreadStore.getState().deleteThread(activeThread.id);

    expect(mockIpc.deleteThread).toHaveBeenCalledWith(activeThread.id);
    expect(useThreadStore.getState().threads).toEqual([]);
    expect(useThreadStore.getState().threadsByWorkspace["workspace-1"]).toEqual([]);
    expect(useThreadStore.getState().archivedThreadsByWorkspace["workspace-1"]).toEqual([
      archivedThread,
    ]);
    expect(useThreadStore.getState().activeThreadId).toBeNull();
    expect(localStorage.removeItem).toHaveBeenCalledWith("supacodex:lastActiveThreadId");
  });
});
