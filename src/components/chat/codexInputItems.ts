import type {
  ChatInputItem,
  CodexApp,
  CodexPluginMarketplace,
  CodexSkill,
} from "../../types";

const TOKEN_PATTERN = /\$([A-Za-z0-9._-]+)/g;

export function normalizeCodexReferenceToken(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/^\$+/, "")
    .replace(/\s+/g, "-");
}

function buildSkillLookup(skills: CodexSkill[]): Map<string, CodexSkill> {
  const lookup = new Map<string, CodexSkill>();
  for (const skill of skills) {
    if (!skill.enabled) {
      continue;
    }
    lookup.set(normalizeCodexReferenceToken(skill.name), skill);
  }
  return lookup;
}

function buildAppLookup(apps: CodexApp[]): Map<string, CodexApp> {
  const lookup = new Map<string, CodexApp>();
  for (const app of apps) {
    if (!app.isEnabled || !app.isAccessible) {
      continue;
    }
    lookup.set(normalizeCodexReferenceToken(app.id), app);
    lookup.set(normalizeCodexReferenceToken(app.name), app);
  }
  return lookup;
}

function buildPluginLookup(
  marketplaces: CodexPluginMarketplace[],
): Map<string, { marketplaceName: string; pluginId: string; pluginName: string }> {
  const lookup = new Map<
    string,
    { marketplaceName: string; pluginId: string; pluginName: string }
  >();
  for (const marketplace of marketplaces) {
    const marketplaceName = marketplace.name.trim();
    if (!marketplaceName) {
      continue;
    }
    for (const plugin of marketplace.plugins) {
      if (!plugin.enabled || !plugin.installed) {
        continue;
      }
      const pluginId = plugin.id.trim();
      const pluginName = plugin.name.trim();
      if (!pluginId || !pluginName) {
        continue;
      }
      const entry = { marketplaceName, pluginId, pluginName };
      lookup.set(normalizeCodexReferenceToken(pluginId), entry);
      lookup.set(normalizeCodexReferenceToken(pluginName), entry);
    }
  }
  return lookup;
}

function pushTextItem(items: ChatInputItem[], text: string) {
  if (!text) {
    return;
  }
  const previous = items.at(-1);
  if (previous?.type === "text") {
    previous.text += text;
    return;
  }
  items.push({ type: "text", text });
}

export function buildCodexInputItems(
  message: string,
  skills: CodexSkill[],
  apps: CodexApp[],
  pluginMarketplaces: CodexPluginMarketplace[] = [],
): ChatInputItem[] {
  const skillLookup = buildSkillLookup(skills);
  const appLookup = buildAppLookup(apps);
  const pluginLookup = buildPluginLookup(pluginMarketplaces);
  const items: ChatInputItem[] = [];
  let lastIndex = 0;

  for (const match of message.matchAll(TOKEN_PATTERN)) {
    const rawToken = match[0];
    const tokenName = match[1] ?? "";
    const matchIndex = match.index ?? 0;
    const normalizedToken = normalizeCodexReferenceToken(tokenName);
    const skill = skillLookup.get(normalizedToken);
    const app = skill ? null : appLookup.get(normalizedToken);
    const plugin = skill || app ? null : pluginLookup.get(normalizedToken);

    if (!skill && !app && !plugin) {
      continue;
    }

    pushTextItem(items, message.slice(lastIndex, matchIndex));
    if (skill) {
      items.push({
        type: "skill",
        name: skill.name,
        path: skill.path,
      });
    } else if (app) {
      items.push({
        type: "mention",
        name: app.name,
        path: `app://${app.id}`,
      });
    } else if (plugin) {
      items.push({
        type: "mention",
        name: plugin.pluginName,
        path: `plugin://${plugin.pluginId}@${plugin.marketplaceName}`,
      });
    }
    lastIndex = matchIndex + rawToken.length;
  }

  pushTextItem(items, message.slice(lastIndex));
  if (items.length === 0) {
    return [{ type: "text", text: message }];
  }

  return items;
}
