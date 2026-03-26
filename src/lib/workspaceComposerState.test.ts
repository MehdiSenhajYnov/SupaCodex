import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

function createStorageStub(initial: Record<string, string> = {}) {
  const storage = new Map<string, string>(Object.entries(initial));
  return {
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
    clear: vi.fn(() => {
      storage.clear();
    }),
  };
}

describe("workspaceComposerState", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads a persisted workspace composer state", async () => {
    vi.stubGlobal("localStorage", createStorageStub({
      "supacodex:workspaceComposer:v1": JSON.stringify({
        version: 1,
        workspaces: {
          ws1: {
            engineId: "codex",
            modelId: "gpt-5.4",
            effort: "high",
            planMode: true,
            personality: "pragmatic",
            serviceTier: "fast",
            outputSchemaText: "{\"type\":\"object\"}",
            customApprovalPolicyText: "{\"mode\":\"strict\"}",
          },
        },
      }),
    }));

    const { readPersistedWorkspaceComposerState } = await import("./workspaceComposerState");

    expect(readPersistedWorkspaceComposerState("ws1")).toEqual({
      engineId: "codex",
      modelId: "gpt-5.4",
      effort: "high",
      planMode: true,
      personality: "pragmatic",
      serviceTier: "fast",
      outputSchemaText: "{\"type\":\"object\"}",
      customApprovalPolicyText: "{\"mode\":\"strict\"}",
    });
  });

  it("writes and normalizes a workspace composer state", async () => {
    const localStorageStub = createStorageStub();
    vi.stubGlobal("localStorage", localStorageStub);

    const {
      readPersistedWorkspaceComposerState,
      writePersistedWorkspaceComposerState,
    } = await import("./workspaceComposerState");

    writePersistedWorkspaceComposerState("ws2", {
      engineId: " codex ",
      modelId: "  ",
      effort: " high ",
      planMode: false,
      personality: " pragmatic ",
      serviceTier: " fast ",
      outputSchemaText: " { } ",
      customApprovalPolicyText: "",
    });

    expect(readPersistedWorkspaceComposerState("ws2")).toEqual({
      engineId: "codex",
      modelId: null,
      effort: "high",
      planMode: false,
      personality: "pragmatic",
      serviceTier: "fast",
      outputSchemaText: " { } ",
      customApprovalPolicyText: "",
    });
  });
});
