export type ShortcutGroupId = "navigation" | "panels" | "search";

export interface ShortcutActionDefinition {
  id: string;
  group: ShortcutGroupId;
  defaultBinding: string | null;
  showInViewMenu?: boolean;
}

export const SHORTCUT_ACTION_DEFINITIONS = [
  {
    id: "previous-conversation",
    group: "navigation",
    defaultBinding: "mod+shift+tab",
    showInViewMenu: true,
  },
  {
    id: "next-conversation",
    group: "navigation",
    defaultBinding: "mod+tab",
    showInViewMenu: true,
  },
  {
    id: "previous-open-project",
    group: "navigation",
    defaultBinding: "mod+alt+pageup",
    showInViewMenu: true,
  },
  {
    id: "next-open-project",
    group: "navigation",
    defaultBinding: "mod+alt+pagedown",
    showInViewMenu: true,
  },
  {
    id: "close-conversation",
    group: "navigation",
    defaultBinding: "mod+alt+w",
    showInViewMenu: true,
  },
  {
    id: "new-thread",
    group: "navigation",
    defaultBinding: "mod+alt+n",
  },
  {
    id: "toggle-sidebar",
    group: "panels",
    defaultBinding: "mod+b",
    showInViewMenu: true,
  },
  {
    id: "toggle-git-panel",
    group: "panels",
    defaultBinding: "mod+shift+b",
    showInViewMenu: true,
  },
  {
    id: "toggle-focus-mode",
    group: "panels",
    defaultBinding: "mod+alt+f",
    showInViewMenu: true,
  },
  {
    id: "toggle-terminal",
    group: "panels",
    defaultBinding: "mod+shift+t",
    showInViewMenu: true,
  },
  {
    id: "toggle-editor",
    group: "panels",
    defaultBinding: "mod+e",
  },
  {
    id: "toggle-fullscreen",
    group: "panels",
    defaultBinding: "f11",
    showInViewMenu: true,
  },
  {
    id: "toggle-search",
    group: "search",
    defaultBinding: "mod+shift+f",
    showInViewMenu: true,
  },
  {
    id: "toggle-command-palette",
    group: "search",
    defaultBinding: "mod+k",
  },
  {
    id: "open-command-palette-files",
    group: "search",
    defaultBinding: "mod+p",
  },
  {
    id: "open-command-palette-threads",
    group: "search",
    defaultBinding: "mod+shift+k",
  },
] as const satisfies readonly ShortcutActionDefinition[];

export type ShortcutActionId = typeof SHORTCUT_ACTION_DEFINITIONS[number]["id"];
export type ShortcutOverrideValue = string | null;
export type ShortcutOverrides = Partial<Record<ShortcutActionId, ShortcutOverrideValue>>;

const SHORTCUT_ACTION_DEFINITION_MAP = SHORTCUT_ACTION_DEFINITIONS.reduce<
Record<ShortcutActionId, (typeof SHORTCUT_ACTION_DEFINITIONS)[number]>
>((acc, definition) => {
  acc[definition.id] = definition;
  return acc;
}, {} as Record<ShortcutActionId, (typeof SHORTCUT_ACTION_DEFINITIONS)[number]>);

const MODIFIER_TOKENS = ["mod", "alt", "shift"] as const;
type ModifierToken = (typeof MODIFIER_TOKENS)[number];

function isMacLikePlatform(): boolean {
  return typeof navigator !== "undefined" && /mac/i.test(navigator.platform);
}

function isModifierToken(token: string): token is ModifierToken {
  return MODIFIER_TOKENS.includes(token as ModifierToken);
}

function normalizeShortcutKeyToken(token: string): string | null {
  const normalized = token.trim().toLowerCase();
  if (!normalized) {
    return null;
  }

  switch (normalized) {
    case "left":
    case "arrowleft":
      return "arrowleft";
    case "right":
    case "arrowright":
      return "arrowright";
    case "up":
    case "arrowup":
      return "arrowup";
    case "down":
    case "arrowdown":
      return "arrowdown";
    case "pageup":
    case "pgup":
      return "pageup";
    case "pagedown":
    case "pgdn":
      return "pagedown";
    case "esc":
    case "escape":
      return "escape";
    case "enter":
    case "return":
      return "enter";
    case "space":
    case "spacebar":
      return "space";
    case "comma":
    case ",":
      return "comma";
    case "tab":
    case "home":
    case "end":
    case "delete":
    case "backspace":
      return normalized;
    default:
      break;
  }

  if (/^f\d{1,2}$/.test(normalized)) {
    return normalized;
  }

  if (normalized.length === 1) {
    return normalized;
  }

  return null;
}

function normalizeModifierToken(token: string): ModifierToken | null {
  const normalized = token.trim().toLowerCase();
  switch (normalized) {
    case "mod":
    case "cmdorctrl":
    case "commandorcontrol":
    case "command":
    case "cmd":
    case "control":
    case "ctrl":
    case "meta":
      return "mod";
    case "alt":
    case "option":
      return "alt";
    case "shift":
      return "shift";
    default:
      return null;
  }
}

function splitShortcutBinding(binding: string): { modifiers: ModifierToken[]; key: string } | null {
  const rawTokens = binding
    .split("+")
    .map((token) => token.trim())
    .filter(Boolean);
  if (rawTokens.length === 0) {
    return null;
  }

  const modifierSet = new Set<ModifierToken>();
  let keyToken: string | null = null;

  for (const token of rawTokens) {
    const modifierToken = normalizeModifierToken(token);
    if (modifierToken) {
      modifierSet.add(modifierToken);
      continue;
    }

    const normalizedKeyToken = normalizeShortcutKeyToken(token);
    if (!normalizedKeyToken || keyToken) {
      return null;
    }
    keyToken = normalizedKeyToken;
  }

  if (!keyToken) {
    return null;
  }

  return {
    modifiers: MODIFIER_TOKENS.filter((token) => modifierSet.has(token)),
    key: keyToken,
  };
}

export function getShortcutActionDefinition(actionId: ShortcutActionId) {
  return SHORTCUT_ACTION_DEFINITION_MAP[actionId];
}

export function getShortcutDefaultBinding(actionId: ShortcutActionId): string | null {
  return SHORTCUT_ACTION_DEFINITION_MAP[actionId].defaultBinding;
}

export function normalizeShortcutBinding(binding: string | null | undefined): string | null {
  if (binding == null) {
    return null;
  }

  const split = splitShortcutBinding(binding);
  if (!split) {
    return null;
  }

  return [...split.modifiers, split.key].join("+");
}

export function eventToShortcutBinding(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">,
): string | null {
  const key = normalizeShortcutKeyToken(event.key);
  if (!key || isModifierToken(key)) {
    return null;
  }

  const modifiers: ModifierToken[] = [];
  if (event.ctrlKey || event.metaKey) {
    modifiers.push("mod");
  }
  if (event.altKey) {
    modifiers.push("alt");
  }
  if (event.shiftKey) {
    modifiers.push("shift");
  }

  return normalizeShortcutBinding([...modifiers, key].join("+"));
}

export function formatShortcutBinding(binding: string | null | undefined): string {
  const normalized = normalizeShortcutBinding(binding);
  if (!normalized) {
    return "";
  }

  const platformIsMac = isMacLikePlatform();
  return normalized
    .split("+")
    .map((token) => {
      switch (token) {
        case "mod":
          return platformIsMac ? "Cmd" : "Ctrl";
        case "alt":
          return platformIsMac ? "Option" : "Alt";
        case "shift":
          return "Shift";
        case "pageup":
          return "PgUp";
        case "pagedown":
          return "PgDn";
        case "arrowleft":
          return "Left";
        case "arrowright":
          return "Right";
        case "arrowup":
          return "Up";
        case "arrowdown":
          return "Down";
        case "escape":
          return "Esc";
        case "comma":
          return ",";
        case "space":
          return "Space";
        default:
          break;
      }

      return token.length === 1 ? token.toUpperCase() : token[0].toUpperCase() + token.slice(1);
    })
    .join("+");
}

export function matchesShortcutBinding(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">,
  binding: string | null | undefined,
): boolean {
  const normalizedBinding = normalizeShortcutBinding(binding);
  if (!normalizedBinding) {
    return false;
  }

  const split = splitShortcutBinding(normalizedBinding);
  if (!split) {
    return false;
  }

  const eventKey = normalizeShortcutKeyToken(event.key);
  if (!eventKey || eventKey !== split.key) {
    return false;
  }

  const requiredModifiers = new Set(split.modifiers);
  const hasMod = event.ctrlKey || event.metaKey;

  return (
    requiredModifiers.has("mod") === hasMod &&
    requiredModifiers.has("alt") === event.altKey &&
    requiredModifiers.has("shift") === event.shiftKey
  );
}

export function getEffectiveShortcutBinding(
  actionId: ShortcutActionId,
  overrides: ShortcutOverrides,
): string | null {
  const override = overrides[actionId];
  if (override !== undefined) {
    return normalizeShortcutBinding(override);
  }

  return getShortcutDefaultBinding(actionId);
}

