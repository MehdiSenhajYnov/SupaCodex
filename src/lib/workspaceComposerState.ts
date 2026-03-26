const WORKSPACE_COMPOSER_STORAGE_KEY = "supacodex:workspaceComposer:v1";

export interface PersistedWorkspaceComposerState {
  engineId: string;
  modelId: string | null;
  effort: string;
  planMode: boolean;
  personality: string;
  serviceTier: string;
  outputSchemaText: string;
  customApprovalPolicyText: string;
}

interface PersistedWorkspaceComposerStorage {
  version: 1;
  workspaces: Record<string, PersistedWorkspaceComposerState>;
}

function normalizeText(value: unknown, fallback = ""): string {
  if (typeof value !== "string") {
    return fallback;
  }
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : fallback;
}

function normalizeNullableText(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : null;
}

function normalizeWorkspaceComposerState(
  value: unknown,
): PersistedWorkspaceComposerState | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }

  const record = value as Record<string, unknown>;
  const engineId = normalizeText(record.engineId, "codex");
  const effort = normalizeText(record.effort, "medium");

  return {
    engineId,
    modelId: normalizeNullableText(record.modelId),
    effort,
    planMode: record.planMode === true,
    personality: normalizeText(record.personality, "inherit"),
    serviceTier: normalizeText(record.serviceTier, "inherit"),
    outputSchemaText: typeof record.outputSchemaText === "string" ? record.outputSchemaText : "",
    customApprovalPolicyText:
      typeof record.customApprovalPolicyText === "string"
        ? record.customApprovalPolicyText
        : "",
  };
}

function readPersistedWorkspaceComposerStorage(): PersistedWorkspaceComposerStorage {
  try {
    const raw = localStorage.getItem(WORKSPACE_COMPOSER_STORAGE_KEY);
    if (!raw) {
      return {
        version: 1,
        workspaces: {},
      };
    }

    const parsed = JSON.parse(raw) as Partial<PersistedWorkspaceComposerStorage> | null;
    if (!parsed || typeof parsed !== "object") {
      throw new Error("invalid workspace composer storage");
    }

    const workspaces: Record<string, PersistedWorkspaceComposerState> = {};
    const rawWorkspaces = parsed.workspaces;
    if (rawWorkspaces && typeof rawWorkspaces === "object" && !Array.isArray(rawWorkspaces)) {
      for (const [workspaceId, state] of Object.entries(rawWorkspaces)) {
        const normalizedWorkspaceId = workspaceId.trim();
        const normalizedState = normalizeWorkspaceComposerState(state);
        if (!normalizedWorkspaceId || !normalizedState) {
          continue;
        }
        workspaces[normalizedWorkspaceId] = normalizedState;
      }
    }

    return {
      version: 1,
      workspaces,
    };
  } catch {
    return {
      version: 1,
      workspaces: {},
    };
  }
}

function writePersistedWorkspaceComposerStorage(
  storage: PersistedWorkspaceComposerStorage,
): void {
  try {
    localStorage.setItem(WORKSPACE_COMPOSER_STORAGE_KEY, JSON.stringify(storage));
  } catch {
    // Ignore persistence failures.
  }
}

export function readPersistedWorkspaceComposerState(
  workspaceId: string | null | undefined,
): PersistedWorkspaceComposerState | null {
  const normalizedWorkspaceId = workspaceId?.trim();
  if (!normalizedWorkspaceId) {
    return null;
  }
  return readPersistedWorkspaceComposerStorage().workspaces[normalizedWorkspaceId] ?? null;
}

export function writePersistedWorkspaceComposerState(
  workspaceId: string,
  state: PersistedWorkspaceComposerState,
): void {
  const normalizedWorkspaceId = workspaceId.trim();
  if (!normalizedWorkspaceId) {
    return;
  }

  const storage = readPersistedWorkspaceComposerStorage();
  storage.workspaces[normalizedWorkspaceId] = normalizeWorkspaceComposerState(state) ?? state;
  writePersistedWorkspaceComposerStorage(storage);
}
