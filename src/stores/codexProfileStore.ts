import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { useEngineStore } from "./engineStore";
import type {
  CodexDetectedProject,
  CodexProfile,
  CodexProfilesState,
  Thread,
} from "../types";

function readThreadCodexProfileId(thread: Thread | null | undefined): string | null {
  const raw = thread?.engineMetadata?.codexProfileId;
  if (typeof raw !== "string") {
    return null;
  }

  const normalized = raw.trim();
  return normalized.length > 0 ? normalized : null;
}

function preferredDetectedProjectProfileId(
  project: CodexDetectedProject | null | undefined,
): string | null {
  const raw = project?.profiles[0]?.profileId;
  if (typeof raw !== "string") {
    return null;
  }

  const normalized = raw.trim();
  return normalized.length > 0 ? normalized : null;
}

interface CodexProfileStoreState {
  profiles: CodexProfile[];
  activeProfileId: string | null;
  detectedProjects: CodexDetectedProject[];
  loading: boolean;
  loadedOnce: boolean;
  detectedLoading: boolean;
  detectedLoadedOnce: boolean;
  modalOpen: boolean;
  error?: string;
  load: () => Promise<CodexProfilesState | null>;
  refreshDetectedProjects: () => Promise<CodexDetectedProject[]>;
  saveProfiles: (
    profiles: CodexProfile[],
    activeProfileId: string,
  ) => Promise<CodexProfilesState | null>;
  setActiveProfile: (profileId: string) => Promise<CodexProfilesState | null>;
  ensureActiveProfile: (profileId?: string | null) => Promise<void>;
  ensureActiveProfileForThread: (thread?: Thread | null) => Promise<void>;
  openModal: () => void;
  closeModal: () => void;
  findProfileById: (profileId?: string | null) => CodexProfile | null;
}

let pendingProfilesRequest: Promise<CodexProfilesState | null> | null = null;

function normalizeProfilesState(
  set: (patch: Partial<CodexProfileStoreState>) => void,
  state: CodexProfilesState,
): CodexProfilesState {
  set({
    profiles: state.profiles,
    activeProfileId: state.activeProfileId,
    loading: false,
    loadedOnce: true,
    error: undefined,
  });
  return state;
}

export { preferredDetectedProjectProfileId, readThreadCodexProfileId };

export const useCodexProfileStore = create<CodexProfileStoreState>((set, get) => ({
  profiles: [],
  activeProfileId: null,
  detectedProjects: [],
  loading: false,
  loadedOnce: false,
  detectedLoading: false,
  detectedLoadedOnce: false,
  modalOpen: false,

  load: async () => {
    if (pendingProfilesRequest) {
      return pendingProfilesRequest;
    }

    set({ loading: true, error: undefined });
    const request = (async () => {
      try {
        const state = await ipc.getCodexProfiles();
        normalizeProfilesState(set, state);
        void get().refreshDetectedProjects();
        return state;
      } catch (error) {
        set({
          loading: false,
          loadedOnce: true,
          error: String(error),
        });
        return null;
      }
    })();

    pendingProfilesRequest = request;
    request.finally(() => {
      if (pendingProfilesRequest === request) {
        pendingProfilesRequest = null;
      }
    });
    return request;
  },

  refreshDetectedProjects: async () => {
    set({ detectedLoading: true, error: undefined });
    try {
      const detectedProjects = await ipc.listCodexDetectedProjects();
      set({
        detectedProjects,
        detectedLoading: false,
        detectedLoadedOnce: true,
      });
      return detectedProjects;
    } catch (error) {
      set({
        detectedLoading: false,
        detectedLoadedOnce: true,
        error: String(error),
      });
      return get().detectedProjects;
    }
  },

  saveProfiles: async (profiles, activeProfileId) => {
    set({ loading: true, error: undefined });
    try {
      const state = await ipc.saveCodexProfiles(profiles, activeProfileId);
      normalizeProfilesState(set, state);
      await get().refreshDetectedProjects();
      await useEngineStore.getState().load();
      return state;
    } catch (error) {
      set({
        loading: false,
        error: String(error),
      });
      throw error;
    }
  },

  setActiveProfile: async (profileId) => {
    const normalized = profileId.trim();
    if (!normalized) {
      return null;
    }

    if (get().activeProfileId === normalized && get().loadedOnce) {
      return {
        activeProfileId: normalized,
        profiles: get().profiles,
      };
    }

    set({ loading: true, error: undefined });
    try {
      const state = await ipc.setActiveCodexProfile(normalized);
      normalizeProfilesState(set, state);
      await get().refreshDetectedProjects();
      await useEngineStore.getState().load();
      return state;
    } catch (error) {
      set({
        loading: false,
        error: String(error),
      });
      throw error;
    }
  },

  ensureActiveProfile: async (profileId) => {
    const normalized = profileId?.trim();
    if (!normalized || normalized === get().activeProfileId) {
      return;
    }
    await get().setActiveProfile(normalized);
  },

  ensureActiveProfileForThread: async (thread) => {
    if (thread?.engineId !== "codex") {
      return;
    }

    const profileId = readThreadCodexProfileId(thread);
    if (!profileId) {
      return;
    }

    await get().ensureActiveProfile(profileId);
  },

  openModal: () => {
    if (!get().loadedOnce && !get().loading) {
      void get().load();
    }
    if (!get().detectedLoadedOnce && !get().detectedLoading) {
      void get().refreshDetectedProjects();
    }
    set({ modalOpen: true });
  },

  closeModal: () => set({ modalOpen: false }),

  findProfileById: (profileId) => {
    const normalized = profileId?.trim();
    if (!normalized) {
      return null;
    }
    return get().profiles.find((profile) => profile.id === normalized) ?? null;
  },
}));
