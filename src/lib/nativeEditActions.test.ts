import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockReadTextFromClipboard = vi.hoisted(() => vi.fn());
const mockToastError = vi.hoisted(() => vi.fn());

vi.mock("../stores/toastStore", () => ({
  toast: {
    error: mockToastError,
  },
}));

vi.mock("../stores/terminalStore", () => ({
  useTerminalStore: {
    getState: () => ({
      workspaces: {},
    }),
  },
}));

vi.mock("../stores/workspaceStore", () => ({
  useWorkspaceStore: {
    getState: () => ({
      activeWorkspaceId: null,
    }),
  },
}));

vi.mock("../components/editor/CodeMirrorEditor", () => ({
  runFocusedEditorHistoryAction: vi.fn(() => false),
}));

vi.mock("./clipboard", () => ({
  readTextFromClipboard: mockReadTextFromClipboard,
}));

vi.mock("./editMenu", () => ({
  shouldDispatchTerminalEditAction: vi.fn(() => false),
}));

vi.mock("./windowActions", () => ({
  isTerminalInputFocused: vi.fn(() => false),
}));

import {
  CHAT_COMPOSER_DATA_ATTRIBUTE,
  CHAT_COMPOSER_NATIVE_IMAGE_PASTE_EVENT,
  CHAT_COMPOSER_DATA_VALUE,
  type ChatComposerNativeImagePasteDetail,
} from "./chatComposerClipboard";
import { runEditMenuAction } from "./nativeEditActions";

class FakeHTMLElement extends EventTarget {
  private readonly attributes = new Map<string, string>();

  isContentEditable = false;

  focus() {}

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }
}

class FakeInputEvent extends Event {
  declare bubbles: boolean;

  data: string | null;

  inputType: string;

  constructor(
    type: string,
    init: { bubbles?: boolean; data?: string | null; inputType?: string } = {},
  ) {
    super(type, { bubbles: init.bubbles ?? false });
    this.data = init.data ?? null;
    this.inputType = init.inputType ?? "";
  }
}

class FakeTextAreaElement extends FakeHTMLElement {
  value = "";

  selectionStart = 0;

  selectionEnd = 0;

  setRangeText(replacement: string, start: number, end: number, _selectionMode?: string): void {
    const prefix = this.value.slice(0, start);
    const suffix = this.value.slice(end);
    this.value = `${prefix}${replacement}${suffix}`;
    const nextCursor = start + replacement.length;
    this.selectionStart = nextCursor;
    this.selectionEnd = nextCursor;
  }
}

class FakeInputElement extends FakeTextAreaElement {}

describe("nativeEditActions", () => {
  const originalHTMLElement = globalThis.HTMLElement;
  const originalHTMLInputElement = globalThis.HTMLInputElement;
  const originalHTMLTextAreaElement = globalThis.HTMLTextAreaElement;
  const originalInputEvent = globalThis.InputEvent;
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const originalNavigator = globalThis.navigator;

  beforeEach(() => {
    vi.clearAllMocks();

    vi.stubGlobal("HTMLElement", FakeHTMLElement);
    vi.stubGlobal("HTMLInputElement", FakeInputElement);
    vi.stubGlobal("HTMLTextAreaElement", FakeTextAreaElement);
    vi.stubGlobal("InputEvent", FakeInputEvent);

    const windowTarget = new EventTarget();
    vi.stubGlobal("window", {
      addEventListener: windowTarget.addEventListener.bind(windowTarget),
      removeEventListener: windowTarget.removeEventListener.bind(windowTarget),
      dispatchEvent: windowTarget.dispatchEvent.bind(windowTarget),
      getSelection: vi.fn(() => null),
    });

    vi.stubGlobal("document", {
      activeElement: null,
      execCommand: vi.fn(() => false),
    });

    vi.stubGlobal("navigator", {
      clipboard: {
        read: vi.fn().mockResolvedValue([]),
      },
    });

    mockReadTextFromClipboard.mockResolvedValue("");
  });

  afterEach(() => {
    if (originalHTMLElement) {
      vi.stubGlobal("HTMLElement", originalHTMLElement);
    } else {
      vi.unstubAllGlobals();
      return;
    }

    if (originalHTMLInputElement) {
      vi.stubGlobal("HTMLInputElement", originalHTMLInputElement);
    }
    if (originalHTMLTextAreaElement) {
      vi.stubGlobal("HTMLTextAreaElement", originalHTMLTextAreaElement);
    }
    if (originalInputEvent) {
      vi.stubGlobal("InputEvent", originalInputEvent);
    }
    if (originalDocument) {
      vi.stubGlobal("document", originalDocument);
    }
    if (originalWindow) {
      vi.stubGlobal("window", originalWindow);
    }
    if (originalNavigator) {
      vi.stubGlobal("navigator", originalNavigator);
    }
  });

  it("dispatches pasted image files when the focused element is the chat composer", async () => {
    const composer = new FakeTextAreaElement();
    composer.setAttribute(CHAT_COMPOSER_DATA_ATTRIBUTE, CHAT_COMPOSER_DATA_VALUE);
    (document as unknown as { activeElement: unknown }).activeElement = composer;

    const pastedImage = new Blob([Uint8Array.from([137, 80, 78, 71])], {
      type: "image/png",
    });
    const clipboardItem = {
      types: ["image/png"],
      getType: vi.fn().mockResolvedValue(pastedImage),
    };
    navigator.clipboard.read = vi.fn().mockResolvedValue([clipboardItem]);

    const received: File[][] = [];
    window.addEventListener(
      CHAT_COMPOSER_NATIVE_IMAGE_PASTE_EVENT,
      (event) => {
        received.push(
          (event as CustomEvent<ChatComposerNativeImagePasteDetail>).detail.files,
        );
      },
      { once: true },
    );

    await runEditMenuAction("edit-paste");

    expect(navigator.clipboard.read).toHaveBeenCalledTimes(1);
    expect(mockReadTextFromClipboard).not.toHaveBeenCalled();
    expect(received).toHaveLength(1);
    expect(received[0]?.[0]).toMatchObject({
      name: "pasted-image.png",
      type: "image/png",
    });
  });

  it("falls back to text insertion when the chat composer paste contains no image", async () => {
    const composer = new FakeTextAreaElement();
    composer.value = "hello";
    composer.selectionStart = composer.value.length;
    composer.selectionEnd = composer.value.length;
    composer.setAttribute(CHAT_COMPOSER_DATA_ATTRIBUTE, CHAT_COMPOSER_DATA_VALUE);
    (document as unknown as { activeElement: unknown }).activeElement = composer;

    navigator.clipboard.read = vi.fn().mockResolvedValue([]);
    mockReadTextFromClipboard.mockResolvedValue(" world");

    await runEditMenuAction("edit-paste");

    expect(navigator.clipboard.read).toHaveBeenCalledTimes(1);
    expect(mockReadTextFromClipboard).toHaveBeenCalledTimes(1);
    expect(composer.value).toBe("hello world");
  });
});
