import { create } from "zustand";
import {
  SHORTCUT_ACTION_DEFINITIONS,
  type ShortcutActionId,
  type ShortcutOverrides,
  getEffectiveShortcutBinding,
  getShortcutDefaultBinding,
  normalizeShortcutBinding,
} from "../lib/shortcutBindings";

const SHORTCUT_SETTINGS_STORAGE_KEY = "supacodex:shortcuts:v1";

interface ShortcutStoreState {
  modalOpen: boolean;
  overrides: ShortcutOverrides;
  openModal: () => void;
  closeModal: () => void;
  setActionShortcut: (actionId: ShortcutActionId, binding: string | null) => ShortcutActionId[];
  clearActionShortcut: (actionId: ShortcutActionId) => void;
  resetActionShortcut: (actionId: ShortcutActionId) => void;
  resetAll: () => void;
}

function normalizeShortcutOverrides(raw: unknown): ShortcutOverrides {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return {};
  }

  const next: ShortcutOverrides = {};
  for (const definition of SHORTCUT_ACTION_DEFINITIONS) {
    const candidate = (raw as Record<string, unknown>)[definition.id];
    if (candidate === undefined) {
      continue;
    }
    if (candidate === null) {
      next[definition.id] = null;
      continue;
    }
    if (typeof candidate !== "string") {
      continue;
    }
    const normalized = normalizeShortcutBinding(candidate);
    if (normalized) {
      next[definition.id] = normalized;
    }
  }

  return next;
}

function readShortcutOverrides(): ShortcutOverrides {
  try {
    const raw = localStorage.getItem(SHORTCUT_SETTINGS_STORAGE_KEY);
    return raw ? normalizeShortcutOverrides(JSON.parse(raw)) : {};
  } catch {
    return {};
  }
}

function persistShortcutOverrides(overrides: ShortcutOverrides): void {
  try {
    localStorage.setItem(SHORTCUT_SETTINGS_STORAGE_KEY, JSON.stringify(overrides));
  } catch {
    // Ignore persistence failures.
  }
}

function withActionValue(
  overrides: ShortcutOverrides,
  actionId: ShortcutActionId,
  binding: string | null,
): ShortcutOverrides {
  const next = { ...overrides };
  const defaultBinding = getShortcutDefaultBinding(actionId);
  const normalizedBinding = normalizeShortcutBinding(binding);

  if (normalizedBinding === defaultBinding) {
    delete next[actionId];
    return next;
  }

  next[actionId] = normalizedBinding;
  return next;
}

const initialOverrides = readShortcutOverrides();

export const useShortcutStore = create<ShortcutStoreState>((set, get) => ({
  modalOpen: false,
  overrides: initialOverrides,
  openModal: () => set({ modalOpen: true }),
  closeModal: () => set({ modalOpen: false }),
  setActionShortcut: (actionId, binding) => {
    const nextOverrides = withActionValue(get().overrides, actionId, binding);
    const effectiveBinding = getEffectiveShortcutBinding(actionId, nextOverrides);
    const clearedConflicts: ShortcutActionId[] = [];

    if (effectiveBinding) {
      for (const definition of SHORTCUT_ACTION_DEFINITIONS) {
        if (definition.id === actionId) {
          continue;
        }
        if (getEffectiveShortcutBinding(definition.id, nextOverrides) !== effectiveBinding) {
          continue;
        }
        nextOverrides[definition.id] = null;
        clearedConflicts.push(definition.id);
      }
    }

    persistShortcutOverrides(nextOverrides);
    set({ overrides: nextOverrides });
    return clearedConflicts;
  },
  clearActionShortcut: (actionId) => {
    const nextOverrides = withActionValue(get().overrides, actionId, null);
    persistShortcutOverrides(nextOverrides);
    set({ overrides: nextOverrides });
  },
  resetActionShortcut: (actionId) => {
    const nextOverrides = { ...get().overrides };
    delete nextOverrides[actionId];
    persistShortcutOverrides(nextOverrides);
    set({ overrides: nextOverrides });
  },
  resetAll: () => {
    persistShortcutOverrides({});
    set({ overrides: {} });
  },
}));

