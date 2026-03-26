export interface ComposerTriggerMatch {
  kind: "slash" | "reference" | "file";
  trigger: "/" | "$" | "@";
  query: string;
  replaceFrom: number;
  replaceTo: number;
  anchorOffset: number;
}

const WHITESPACE_PATTERN = /\s/;
const REFERENCE_TOKEN_PATTERN = /^[A-Za-z0-9._-]*$/;
const SLASH_TOKEN_PATTERN = /^[A-Za-z-]*$/;

function clampCursor(value: string, cursor: number): number {
  if (!Number.isFinite(cursor)) {
    return value.length;
  }
  return Math.max(0, Math.min(value.length, Math.trunc(cursor)));
}

function isWhitespace(value: string): boolean {
  return WHITESPACE_PATTERN.test(value);
}

function findTokenStart(value: string, cursor: number): number {
  let index = cursor;
  while (index > 0 && !isWhitespace(value[index - 1] ?? "")) {
    index -= 1;
  }
  return index;
}

function findTokenEnd(value: string, cursor: number): number {
  let index = cursor;
  while (index < value.length && !isWhitespace(value[index] ?? "")) {
    index += 1;
  }
  return index;
}

export function findComposerTrigger(
  value: string,
  cursorPosition: number,
): ComposerTriggerMatch | null {
  const cursor = clampCursor(value, cursorPosition);
  const tokenStart = findTokenStart(value, cursor);
  const tokenEnd = findTokenEnd(value, cursor);
  const token = value.slice(tokenStart, tokenEnd);

  if (!token) {
    return null;
  }

  if (token.startsWith("/")) {
    const prefix = value.slice(0, tokenStart);
    if (prefix.includes("\n") || prefix.trim().length > 0) {
      return null;
    }
    const query = token.slice(1);
    if (!SLASH_TOKEN_PATTERN.test(query)) {
      return null;
    }
    return {
      kind: "slash",
      trigger: "/",
      query,
      replaceFrom: tokenStart,
      replaceTo: tokenEnd,
      anchorOffset: tokenStart,
    };
  }

  if (token.startsWith("$")) {
    const query = token.slice(1);
    if (!REFERENCE_TOKEN_PATTERN.test(query)) {
      return null;
    }
    return {
      kind: "reference",
      trigger: "$",
      query,
      replaceFrom: tokenStart,
      replaceTo: tokenEnd,
      anchorOffset: tokenStart,
    };
  }

  if (token.startsWith("@")) {
    const query = token.slice(1);
    if (query.includes("\"") || query.includes("'")) {
      return null;
    }
    return {
      kind: "file",
      trigger: "@",
      query,
      replaceFrom: tokenStart,
      replaceTo: tokenEnd,
      anchorOffset: tokenStart,
    };
  }

  return null;
}

export function extractShellCommand(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed.startsWith("!")) {
    return null;
  }
  const command = trimmed.slice(1).trim();
  return command.length > 0 ? command : null;
}
