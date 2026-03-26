import { useLayoutEffect, useState, type RefObject } from "react";

export interface AnchoredPopoverRect {
  top: number;
  left: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

export interface AnchoredPopoverPosition {
  top: number;
  left: number;
  direction: "top" | "bottom";
}

interface GetAnchoredPopoverPositionOptions {
  triggerRect: AnchoredPopoverRect;
  popoverWidth: number;
  popoverHeight: number;
  viewportWidth: number;
  viewportHeight: number;
  preferredDirection?: "top" | "bottom";
  align?: "start" | "center" | "end";
  gap?: number;
  padding?: number;
}

interface UseAnchoredPopoverPositionOptions {
  open: boolean;
  triggerRef: RefObject<HTMLElement | null>;
  popoverRef: RefObject<HTMLElement | null>;
  preferredDirection?: "top" | "bottom";
  align?: "start" | "center" | "end";
  gap?: number;
  padding?: number;
}

function clamp(value: number, min: number, max: number): number {
  if (max < min) {
    return min;
  }
  return Math.min(Math.max(value, min), max);
}

function readViewportSize(): { width: number; height: number } {
  const visualViewport = window.visualViewport;
  return {
    width: visualViewport?.width ?? window.innerWidth,
    height: visualViewport?.height ?? window.innerHeight,
  };
}

export function getAnchoredPopoverPosition({
  triggerRect,
  popoverWidth,
  popoverHeight,
  viewportWidth,
  viewportHeight,
  preferredDirection = "top",
  align = "center",
  gap = 6,
  padding = 8,
}: GetAnchoredPopoverPositionOptions): AnchoredPopoverPosition {
  const unclampedLeft =
    align === "start"
      ? triggerRect.left
      : align === "end"
        ? triggerRect.right - popoverWidth
        : triggerRect.left + (triggerRect.width - popoverWidth) / 2;

  const maxLeft = Math.max(padding, viewportWidth - popoverWidth - padding);
  const left = clamp(unclampedLeft, padding, maxLeft);

  const spaceAbove = triggerRect.top - padding;
  const spaceBelow = viewportHeight - triggerRect.bottom - padding;
  const canOpenAbove = spaceAbove >= popoverHeight + gap;
  const canOpenBelow = spaceBelow >= popoverHeight + gap;

  let direction: "top" | "bottom";
  if (preferredDirection === "top") {
    direction =
      canOpenAbove || (!canOpenBelow && spaceAbove >= spaceBelow)
        ? "top"
        : "bottom";
  } else {
    direction =
      canOpenBelow || (!canOpenAbove && spaceBelow >= spaceAbove)
        ? "bottom"
        : "top";
  }

  const preferredTop =
    direction === "top"
      ? triggerRect.top - popoverHeight - gap
      : triggerRect.bottom + gap;
  const maxTop = Math.max(padding, viewportHeight - popoverHeight - padding);
  const top = clamp(preferredTop, padding, maxTop);

  return { top, left, direction };
}

export function useAnchoredPopoverPosition({
  open,
  triggerRef,
  popoverRef,
  preferredDirection = "top",
  align = "center",
  gap = 6,
  padding = 8,
}: UseAnchoredPopoverPositionOptions): AnchoredPopoverPosition {
  const [position, setPosition] = useState<AnchoredPopoverPosition>({
    top: 0,
    left: 0,
    direction: preferredDirection,
  });

  useLayoutEffect(() => {
    if (!open) {
      return;
    }

    const trigger = triggerRef.current;
    const popover = popoverRef.current;
    if (!trigger || !popover) {
      return;
    }

    const updatePosition = () => {
      const nextTrigger = triggerRef.current;
      const nextPopover = popoverRef.current;
      if (!nextTrigger || !nextPopover) {
        return;
      }

      const triggerRect = nextTrigger.getBoundingClientRect();
      const popoverRect = nextPopover.getBoundingClientRect();
      const viewport = readViewportSize();

      setPosition(
        getAnchoredPopoverPosition({
          triggerRect,
          popoverWidth: popoverRect.width,
          popoverHeight: popoverRect.height,
          viewportWidth: viewport.width,
          viewportHeight: viewport.height,
          preferredDirection,
          align,
          gap,
          padding,
        }),
      );
    };

    updatePosition();

    const handleViewportChange = () => updatePosition();
    const resizeObserver =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => updatePosition())
        : null;

    resizeObserver?.observe(trigger);
    resizeObserver?.observe(popover);

    window.addEventListener("resize", handleViewportChange);
    window.addEventListener("scroll", handleViewportChange, true);
    window.visualViewport?.addEventListener("resize", handleViewportChange);
    window.visualViewport?.addEventListener("scroll", handleViewportChange);

    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener("resize", handleViewportChange);
      window.removeEventListener("scroll", handleViewportChange, true);
      window.visualViewport?.removeEventListener("resize", handleViewportChange);
      window.visualViewport?.removeEventListener("scroll", handleViewportChange);
    };
  }, [
    align,
    gap,
    open,
    padding,
    popoverRef,
    preferredDirection,
    triggerRef,
  ]);

  return position;
}
