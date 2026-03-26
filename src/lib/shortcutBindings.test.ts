import { beforeEach, describe, expect, it } from "vitest";
import {
  eventToShortcutBinding,
  formatShortcutBinding,
  getEffectiveShortcutBinding,
  matchesShortcutBinding,
  normalizeShortcutBinding,
} from "./shortcutBindings";
import { useShortcutStore } from "../stores/shortcutStore";

const storage = new Map<string, string>();

Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => {
      storage.set(key, value);
    },
    removeItem: (key: string) => {
      storage.delete(key);
    },
    clear: () => {
      storage.clear();
    },
  },
});

describe("shortcutBindings", () => {
  beforeEach(() => {
    localStorage.clear();
    useShortcutStore.setState({
      modalOpen: false,
      overrides: {},
    });
  });

  it("normalizes shortcut descriptors into a canonical format", () => {
    expect(normalizeShortcutBinding("Ctrl+Shift+Tab")).toBe("mod+shift+tab");
    expect(normalizeShortcutBinding("Alt + PgDn")).toBe("alt+pagedown");
    expect(normalizeShortcutBinding("f11")).toBe("f11");
  });

  it("records shortcut bindings from keyboard events", () => {
    expect(
      eventToShortcutBinding({
        key: "PageDown",
        ctrlKey: true,
        metaKey: false,
        altKey: true,
        shiftKey: false,
      }),
    ).toBe("mod+alt+pagedown");
  });

  it("matches shortcuts with exact modifier requirements", () => {
    expect(
      matchesShortcutBinding(
        {
          key: "Tab",
          ctrlKey: true,
          metaKey: false,
          altKey: false,
          shiftKey: true,
        },
        "mod+shift+tab",
      ),
    ).toBe(true);

    expect(
      matchesShortcutBinding(
        {
          key: "Tab",
          ctrlKey: true,
          metaKey: false,
          altKey: false,
          shiftKey: true,
        },
        "mod+tab",
      ),
    ).toBe(false);
  });

  it("formats bindings for display", () => {
    expect(formatShortcutBinding("mod+alt+pagedown")).toMatch(/(Ctrl|Cmd)\+/);
    expect(formatShortcutBinding("mod+alt+pagedown")).toContain("PgDn");
  });

  it("disables conflicting shortcuts when a new binding is assigned", () => {
    const cleared = useShortcutStore.getState().setActionShortcut("next-conversation", "mod+alt+w");

    expect(cleared).toContain("close-conversation");
    expect(getEffectiveShortcutBinding("next-conversation", useShortcutStore.getState().overrides)).toBe(
      "mod+alt+w",
    );
    expect(getEffectiveShortcutBinding("close-conversation", useShortcutStore.getState().overrides)).toBe(
      null,
    );
  });
});
