import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";

const mockIsTauri = vi.hoisted(() => vi.fn());
const mockStartDragging = vi.hoisted(() => vi.fn());
const mockToggleMaximize = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: mockIsTauri,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    startDragging: mockStartDragging,
    toggleMaximize: mockToggleMaximize,
  }),
}));

import { handleDragMouseDown } from "./windowDrag";

class MockElement {
  constructor(private readonly interactive = false) {}

  closest() {
    return this.interactive ? this : null;
  }
}

const originalElement = globalThis.Element;

describe("windowDrag", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauri.mockReturnValue(true);
    mockStartDragging.mockResolvedValue(undefined);
    mockToggleMaximize.mockResolvedValue(undefined);
    Object.defineProperty(globalThis, "Element", {
      configurable: true,
      value: MockElement,
    });
  });

  it("starts dragging the window on a primary single click", () => {
    const target = new MockElement();

    handleDragMouseDown({
      button: 0,
      detail: 1,
      target,
    } as unknown as React.MouseEvent);

    expect(mockStartDragging).toHaveBeenCalledTimes(1);
    expect(mockToggleMaximize).not.toHaveBeenCalled();
  });

  it("toggles maximize on a primary double click", () => {
    const target = new MockElement();

    handleDragMouseDown({
      button: 0,
      detail: 2,
      target,
    } as unknown as React.MouseEvent);

    expect(mockToggleMaximize).toHaveBeenCalledTimes(1);
    expect(mockStartDragging).not.toHaveBeenCalled();
  });

  it("ignores interactive targets inside the titlebar", () => {
    const button = new MockElement(true);

    handleDragMouseDown({
      button: 0,
      detail: 2,
      target: button,
    } as unknown as React.MouseEvent);

    expect(mockToggleMaximize).not.toHaveBeenCalled();
    expect(mockStartDragging).not.toHaveBeenCalled();
  });

  afterAll(() => {
    if (originalElement) {
      Object.defineProperty(globalThis, "Element", {
        configurable: true,
        value: originalElement,
      });
      return;
    }

    Reflect.deleteProperty(globalThis, "Element");
  });
});
