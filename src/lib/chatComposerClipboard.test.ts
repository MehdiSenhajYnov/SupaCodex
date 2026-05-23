import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CHAT_COMPOSER_DATA_ATTRIBUTE,
  CHAT_COMPOSER_DATA_VALUE,
  isChatComposerPasteShortcut,
} from "./chatComposerClipboard";

class FakeHTMLElement extends EventTarget {
  private readonly attributes = new Map<string, string>();

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }
}

describe("chatComposerClipboard", () => {
  const originalHTMLElement = globalThis.HTMLElement;

  beforeEach(() => {
    vi.stubGlobal("HTMLElement", FakeHTMLElement);
  });

  afterEach(() => {
    if (originalHTMLElement) {
      vi.stubGlobal("HTMLElement", originalHTMLElement);
      return;
    }

    vi.unstubAllGlobals();
  });

  it("matches the standard paste shortcut when the chat composer is focused", () => {
    const composer = new FakeHTMLElement();
    composer.setAttribute(CHAT_COMPOSER_DATA_ATTRIBUTE, CHAT_COMPOSER_DATA_VALUE);

    expect(
      isChatComposerPasteShortcut(
        {
          altKey: false,
          ctrlKey: true,
          key: "v",
          metaKey: false,
          shiftKey: false,
        },
        composer,
      ),
    ).toBe(true);
  });

  it("ignores non-composer elements and modified paste variants", () => {
    const composer = new FakeHTMLElement();
    composer.setAttribute(CHAT_COMPOSER_DATA_ATTRIBUTE, CHAT_COMPOSER_DATA_VALUE);
    const otherElement = new FakeHTMLElement();

    expect(
      isChatComposerPasteShortcut(
        {
          altKey: false,
          ctrlKey: true,
          key: "v",
          metaKey: false,
          shiftKey: false,
        },
        otherElement,
      ),
    ).toBe(false);

    expect(
      isChatComposerPasteShortcut(
        {
          altKey: false,
          ctrlKey: true,
          key: "v",
          metaKey: false,
          shiftKey: true,
        },
        composer,
      ),
    ).toBe(false);
  });
});
