import { useCallback, useEffect, useRef, useState } from "react";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Sidebar } from "../sidebar/Sidebar";
import { ChatPanel } from "../chat/ChatPanel";
import { HarnessPanel } from "../onboarding/HarnessPanel";
import { WorkspaceSettingsPage } from "../workspace/WorkspaceSettingsPage";
import { GitPanel } from "../git/GitPanel";
import { usesCustomWindowFrame } from "../../lib/windowActions";
import { useUiStore } from "../../stores/uiStore";
import { handleDragMouseDown } from "../../lib/windowDrag";

const SIDEBAR_WIDTH_KEY = "supacodex:sidebar-width";
const MIN_SIDEBAR = 160;
const MAX_SIDEBAR = 380;
const DEFAULT_SIDEBAR = 220;

function loadSidebarWidth(): number {
  try {
    const stored = localStorage.getItem(SIDEBAR_WIDTH_KEY);
    if (stored) {
      const v = parseInt(stored, 10);
      if (v >= MIN_SIDEBAR && v <= MAX_SIDEBAR) return v;
    }
  } catch { /* ignore */ }
  return DEFAULT_SIDEBAR;
}

export function ThreeColumnLayout() {
  const { t } = useTranslation(["app", "git"]);
  const showSidebar = useUiStore((state) => state.showSidebar);
  const showGitPanel = useUiStore((state) => state.showGitPanel);
  const focusMode = useUiStore((state) => state.focusMode);
  const activeView = useUiStore((state) => state.activeView);
  const toggleSidebar = useUiStore((state) => state.toggleSidebar);
  const toggleGitPanel = useUiStore((state) => state.toggleGitPanel);
  const customWindowFrame = usesCustomWindowFrame();

  const sidebarVisible = showSidebar;
  const centerDefaultSize = showGitPanel ? 74 : 100;
  const fullBleedContent = focusMode || !showSidebar;
  const showFocusDragStrip = focusMode && !showSidebar && !showGitPanel && !customWindowFrame;

  const [sidebarWidth, setSidebarWidth] = useState(loadSidebarWidth);
  const [revealedClosedEdge, setRevealedClosedEdge] = useState<"left" | "right" | null>(null);
  const draggingRef = useRef(false);
  const handleRef = useRef<HTMLDivElement>(null);
  const contentCardRef = useRef<HTMLDivElement>(null);

  // Persist sidebar width
  useEffect(() => {
    try { localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth)); } catch { /* ignore */ }
  }, [sidebarWidth]);

  const handleSidebarResizeMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidth;
    draggingRef.current = true;
    handleRef.current?.classList.add("dragging");

    function onMove(ev: MouseEvent) {
      const delta = ev.clientX - startX;
      setSidebarWidth(Math.min(MAX_SIDEBAR, Math.max(MIN_SIDEBAR, startWidth + delta)));
    }
    function onUp() {
      draggingRef.current = false;
      handleRef.current?.classList.remove("dragging");
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    }
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }, [sidebarWidth]);

  const handleContentPointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const element = contentCardRef.current;
    if (!element || focusMode) {
      return;
    }

    const rect = element.getBoundingClientRect();
    const pointerX = event.clientX - rect.left;
    const edgeRevealThreshold = 52;

    if (!showSidebar && pointerX <= edgeRevealThreshold) {
      setRevealedClosedEdge("left");
      return;
    }

    if (!showGitPanel && pointerX >= rect.width - edgeRevealThreshold) {
      setRevealedClosedEdge("right");
      return;
    }

    setRevealedClosedEdge(null);
  }, [focusMode, showGitPanel, showSidebar]);

  const handleContentPointerLeave = useCallback(() => {
    setRevealedClosedEdge(null);
  }, []);

  return (
    <div className="layout-root">
      {sidebarVisible && (
        <div className="layout-sidebar-shell" style={{ width: sidebarWidth }}>
          <div className="layout-sidebar">
            <Sidebar />
          </div>
          {!focusMode && (
            <button
              type="button"
              className="layout-edge-toggle layout-edge-toggle-left-open"
              aria-label={t("app:sidebar.hideSidebarPanel")}
              title={t("app:sidebar.hideSidebarPanel")}
              onClick={toggleSidebar}
            >
              <ChevronLeft size={12} />
            </button>
          )}
        </div>
      )}

      {sidebarVisible && (
        <div
          ref={handleRef}
          className="sidebar-resize-handle"
          onMouseDown={handleSidebarResizeMouseDown}
        />
      )}

      {/* Floating content card */}
      <div
        ref={contentCardRef}
        className={`content-card ${fullBleedContent ? "content-card-full" : ""}`}
        onPointerMove={handleContentPointerMove}
        onPointerLeave={handleContentPointerLeave}
      >
        {!focusMode && !showSidebar && (
          <button
            type="button"
            className={`layout-edge-toggle layout-edge-toggle-left-closed ${
              revealedClosedEdge === "left" ? "layout-edge-toggle-revealed" : ""
            }`}
            tabIndex={revealedClosedEdge === "left" ? 0 : -1}
            aria-hidden={revealedClosedEdge === "left" ? undefined : true}
            aria-label={t("app:sidebar.showSidebarPanel")}
            title={t("app:sidebar.showSidebarPanel")}
            onClick={toggleSidebar}
          >
            <ChevronRight size={12} />
          </button>
        )}
        {!focusMode && !showGitPanel && (
          <button
            type="button"
            className={`layout-edge-toggle layout-edge-toggle-right-closed ${
              revealedClosedEdge === "right" ? "layout-edge-toggle-revealed" : ""
            }`}
            tabIndex={revealedClosedEdge === "right" ? 0 : -1}
            aria-hidden={revealedClosedEdge === "right" ? undefined : true}
            aria-label={t("app:sidebar.showGitPanel")}
            title={t("app:sidebar.showGitPanel")}
            onClick={toggleGitPanel}
          >
            <ChevronLeft size={12} />
          </button>
        )}
        {showFocusDragStrip && (
          <div
            className="focus-drag-strip"
            onMouseDown={handleDragMouseDown}
          />
        )}
        <PanelGroup
          key={`${showGitPanel}`}
          direction="horizontal"
          style={{ height: "100%", flex: 1 }}
        >
          <Panel defaultSize={centerDefaultSize} minSize={35}>
            <div className="content-panel" style={{ height: "100%" }}>
              {activeView === "harnesses" ? (
                <HarnessPanel />
              ) : activeView === "workspace-settings" ? (
                <WorkspaceSettingsPage />
              ) : (
                <ChatPanel />
              )}
            </div>
          </Panel>

          {showGitPanel && <PanelResizeHandle className="resize-handle" />}

          {showGitPanel && (
            <Panel defaultSize={26} minSize={18} maxSize={40}>
              <div className="content-panel layout-git-shell" style={{ height: "100%" }}>
                {!focusMode && (
                  <button
                    type="button"
                    className="layout-edge-toggle layout-edge-toggle-right-open"
                    aria-label={t("app:sidebar.hideGitPanel")}
                    title={t("app:sidebar.hideGitPanel")}
                    onClick={toggleGitPanel}
                  >
                    <ChevronRight size={12} />
                  </button>
                )}
                <GitPanel />
              </div>
            </Panel>
          )}
        </PanelGroup>
      </div>
    </div>
  );
}
