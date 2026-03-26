import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const INTERACTIVE = "button, input, textarea, select, a, .dropdown-menu, .no-drag";

function isInteractive(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return target.closest(INTERACTIVE) !== null;
}

function reportWindowActionError(action: string, error: unknown) {
  if (import.meta.env.DEV) {
    console.warn(`[windowDrag] Failed to ${action}`, error);
  }
}

export function handleDragMouseDown(e: React.MouseEvent) {
  if (e.button !== 0) return;
  if (isInteractive(e.target)) return;
  if (!isTauri()) return;

  const currentWindow = getCurrentWindow();
  const action =
    e.detail === 2
      ? currentWindow.toggleMaximize()
      : e.detail === 1
        ? currentWindow.startDragging()
        : null;

  action?.catch((error) => {
    reportWindowActionError(
      e.detail === 2 ? "toggle maximize window" : "start dragging window",
      error,
    );
  });
}
