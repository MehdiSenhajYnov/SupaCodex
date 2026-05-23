import { describe, expect, it } from "vitest";
import type { CodexDetectedProject, Thread, Workspace } from "../../types";
import { buildSidebarProjectEntries } from "./sidebarProjectEntries";

function makeWorkspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: "workspace-1",
    rootPath: "/tmp/workspace-1",
    name: "Workspace 1",
    scanDepth: 2,
    createdAt: "2026-04-01T10:00:00.000Z",
    lastOpenedAt: "2026-04-01T10:00:00.000Z",
    ...overrides,
  };
}

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
    messageCount: 1,
    totalTokens: 0,
    createdAt: "2026-04-01T10:00:00.000Z",
    lastActivityAt: "2026-04-01T10:00:00.000Z",
    ...overrides,
  };
}

function makeDetectedProject(overrides: Partial<CodexDetectedProject> = {}): CodexDetectedProject {
  return {
    path: "/tmp/workspace-1",
    name: "Workspace 1",
    threadCount: 1,
    lastActivityAt: "2026-04-01T11:00:00.000Z",
    workspaceId: "workspace-1",
    profiles: [],
    threads: [
      {
        engineThreadId: "engine-thread-1",
        title: "Detected Thread",
        preview: "preview",
        createdAt: "2026-04-01T10:00:00.000Z",
        updatedAt: "2026-04-01T11:00:00.000Z",
        profileId: "profile-1",
        profileName: "Default",
        modelProvider: "openai",
        archived: false,
      },
    ],
    ...overrides,
  };
}

describe("buildSidebarProjectEntries", () => {
  it("hides archived detected conversations from imported project lists", () => {
    const workspace = makeWorkspace();
    const entries = buildSidebarProjectEntries(
      [{ workspace, threads: [] }],
      [
        makeDetectedProject({
          threads: [
            {
              engineThreadId: "engine-thread-archived",
              title: "Archived Thread",
              preview: "preview",
              createdAt: "2026-04-01T10:00:00.000Z",
              updatedAt: "2026-04-01T11:00:00.000Z",
              profileId: "profile-1",
              profileName: "Default",
              modelProvider: "openai",
              archived: true,
            },
          ],
        }),
      ],
      {},
    );

    expect(entries).toHaveLength(1);
    expect(entries[0]?.conversations).toEqual([]);
    expect(entries[0]?.totalConversationCount).toBe(0);
  });

  it("keeps active detected conversations visible", () => {
    const workspace = makeWorkspace();
    const entries = buildSidebarProjectEntries(
      [{ workspace, threads: [] }],
      [makeDetectedProject()],
      {},
    );

    expect(entries[0]?.conversations).toHaveLength(1);
    expect(entries[0]?.conversations[0]?.kind).toBe("detected");
  });

  it("removes archived-only external detected projects from the sidebar list", () => {
    const entries = buildSidebarProjectEntries(
      [{ workspace: makeWorkspace(), threads: [makeThread()] }],
      [
        makeDetectedProject({
          path: "/tmp/external",
          name: "External",
          workspaceId: null,
          threads: [
            {
              engineThreadId: "engine-thread-archived",
              title: "Archived Thread",
              preview: "preview",
              createdAt: "2026-04-01T10:00:00.000Z",
              updatedAt: "2026-04-01T11:00:00.000Z",
              profileId: "profile-1",
              profileName: "Default",
              modelProvider: "openai",
              archived: true,
            },
          ],
        }),
      ],
      {},
    );

    expect(entries.some((entry) => entry.path === "/tmp/external")).toBe(false);
  });
});
