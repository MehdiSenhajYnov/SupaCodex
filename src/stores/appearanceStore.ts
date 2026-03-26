import { create } from "zustand";

const APPEARANCE_STORAGE_KEY = "supacodex:appearance";

export interface AppearanceSettings {
  interfaceZoom: number;
  windowRadius: number;
  windowGap: number;
  surfaceBlur: number;
  transparentSidebar: boolean;
  transparentContent: boolean;
  transparentTerminal: boolean;
}

interface AppearanceStoreState extends AppearanceSettings {
  modalOpen: boolean;
  setSetting: <K extends keyof AppearanceSettings>(
    key: K,
    value: AppearanceSettings[K],
  ) => void;
  patchSettings: (patch: Partial<AppearanceSettings>) => void;
  reset: () => void;
  openModal: () => void;
  closeModal: () => void;
}

export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {
  interfaceZoom: 100,
  windowRadius: 18,
  windowGap: 0,
  surfaceBlur: 18,
  transparentSidebar: false,
  transparentContent: false,
  transparentTerminal: false,
};

function normalizeAppearanceSettings(raw: unknown): AppearanceSettings {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return DEFAULT_APPEARANCE_SETTINGS;
  }

  const candidate = raw as Partial<AppearanceSettings>;
  const numberOr = (value: unknown, fallback: number, min: number, max: number) => {
    if (typeof value !== "number" || !Number.isFinite(value)) {
      return fallback;
    }
    return Math.min(max, Math.max(min, Math.round(value)));
  };

  return {
    interfaceZoom: numberOr(candidate.interfaceZoom, DEFAULT_APPEARANCE_SETTINGS.interfaceZoom, 80, 160),
    windowRadius: numberOr(candidate.windowRadius, DEFAULT_APPEARANCE_SETTINGS.windowRadius, 0, 36),
    windowGap: numberOr(candidate.windowGap, DEFAULT_APPEARANCE_SETTINGS.windowGap, 0, 24),
    surfaceBlur: numberOr(candidate.surfaceBlur, DEFAULT_APPEARANCE_SETTINGS.surfaceBlur, 0, 40),
    transparentSidebar:
      typeof candidate.transparentSidebar === "boolean"
        ? candidate.transparentSidebar
        : DEFAULT_APPEARANCE_SETTINGS.transparentSidebar,
    transparentContent:
      typeof candidate.transparentContent === "boolean"
        ? candidate.transparentContent
        : DEFAULT_APPEARANCE_SETTINGS.transparentContent,
    transparentTerminal:
      typeof candidate.transparentTerminal === "boolean"
        ? candidate.transparentTerminal
        : DEFAULT_APPEARANCE_SETTINGS.transparentTerminal,
  };
}

function readAppearanceSettings(): AppearanceSettings {
  try {
    const raw = localStorage.getItem(APPEARANCE_STORAGE_KEY);
    return raw ? normalizeAppearanceSettings(JSON.parse(raw)) : DEFAULT_APPEARANCE_SETTINGS;
  } catch {
    return DEFAULT_APPEARANCE_SETTINGS;
  }
}

function persistAppearanceSettings(settings: AppearanceSettings): void {
  try {
    localStorage.setItem(APPEARANCE_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Ignore localStorage failures.
  }
}

const initialAppearanceSettings = readAppearanceSettings();

export const useAppearanceStore = create<AppearanceStoreState>((set, get) => ({
  ...initialAppearanceSettings,
  modalOpen: false,

  setSetting: (key, value) => {
    const next = {
      ...get(),
      [key]: value,
    };
    const normalized = normalizeAppearanceSettings(next);
    persistAppearanceSettings(normalized);
    set(normalized);
  },

  patchSettings: (patch) => {
    const next = normalizeAppearanceSettings({
      ...get(),
      ...patch,
    });
    persistAppearanceSettings(next);
    set(next);
  },

  reset: () => {
    persistAppearanceSettings(DEFAULT_APPEARANCE_SETTINGS);
    set(DEFAULT_APPEARANCE_SETTINGS);
  },

  openModal: () => set({ modalOpen: true }),
  closeModal: () => set({ modalOpen: false }),
}));
