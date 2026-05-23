import { afterEach, describe, expect, it, vi } from "vitest";
import {
  getAnchoredPopoverPosition,
  normalizeClientPointForFixedPosition,
  normalizeClientRectForFixedPosition,
  readAppZoomScale,
} from "./anchoredPopoverPosition";

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubDocumentZoom(zoom: string) {
  const properties = new Map<string, string>([["zoom", zoom]]);
  const style = {
    getPropertyValue: (name: string) => properties.get(name) ?? "",
    setProperty: (name: string, value: string) => {
      properties.set(name, value);
    },
    removeProperty: (name: string) => {
      properties.delete(name);
    },
  };

  vi.stubGlobal("document", {
    documentElement: {
      style,
    },
  });
  vi.stubGlobal("getComputedStyle", () => style);
}

describe("getAnchoredPopoverPosition", () => {
  it("centers the popover around the trigger when space allows", () => {
    expect(
      getAnchoredPopoverPosition({
        triggerRect: {
          top: 520,
          left: 420,
          right: 520,
          bottom: 548,
          width: 100,
          height: 28,
        },
        popoverWidth: 420,
        popoverHeight: 180,
        viewportWidth: 1200,
        viewportHeight: 900,
      }),
    ).toEqual({
      top: 334,
      left: 260,
      direction: "top",
    });
  });

  it("flips below when there is not enough room above", () => {
    expect(
      getAnchoredPopoverPosition({
        triggerRect: {
          top: 24,
          left: 300,
          right: 420,
          bottom: 52,
          width: 120,
          height: 28,
        },
        popoverWidth: 320,
        popoverHeight: 160,
        viewportWidth: 900,
        viewportHeight: 700,
      }),
    ).toEqual({
      top: 58,
      left: 200,
      direction: "bottom",
    });
  });

  it("clamps the popover inside the viewport when it would overflow", () => {
    expect(
      getAnchoredPopoverPosition({
        triggerRect: {
          top: 580,
          left: 20,
          right: 92,
          bottom: 608,
          width: 72,
          height: 28,
        },
        popoverWidth: 420,
        popoverHeight: 260,
        viewportWidth: 460,
        viewportHeight: 640,
      }),
    ).toEqual({
      top: 314,
      left: 8,
      direction: "top",
    });
  });
});

describe("fixed positioning zoom helpers", () => {
  it("normalizes root CSS zoom percentages", () => {
    stubDocumentZoom("80%");
    expect(readAppZoomScale()).toBe(0.8);
  });

  it("converts client rects and points back into fixed element coordinates", () => {
    stubDocumentZoom("0.8");

    expect(
      normalizeClientRectForFixedPosition({
        top: 80,
        left: 160,
        right: 240,
        bottom: 120,
        width: 80,
        height: 40,
      }),
    ).toEqual({
      top: 100,
      left: 200,
      right: 300,
      bottom: 150,
      width: 100,
      height: 50,
    });

    expect(normalizeClientPointForFixedPosition({ x: 320, y: 96 })).toEqual({
      x: 400,
      y: 120,
    });
  });
});
