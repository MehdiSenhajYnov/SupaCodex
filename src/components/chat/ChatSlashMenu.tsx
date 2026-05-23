import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { LucideIcon } from "lucide-react";
import {
  normalizeClientRectForFixedPosition,
  readFixedViewportSize,
} from "../shared/anchoredPopoverPosition";

export interface ChatComposerMenuItem {
  id: string;
  name: string;
  description: string;
  icon: LucideIcon;
  badge?: string;
  disabled?: boolean;
}

interface CaretAnchorPosition {
  top: number;
  left: number;
  width: number;
}

interface ChatComposerMenuProps {
  visible: boolean;
  queryKey: string;
  items: ChatComposerMenuItem[];
  anchorRef: RefObject<HTMLTextAreaElement | null>;
  caretIndex: number;
  activeIndex: number;
  onSelect: (itemId: string) => void;
  onDismiss: () => void;
  onActiveChange: (index: number) => void;
}

const TEXTAREA_MIRROR_STYLE_KEYS = [
  "borderBottomWidth",
  "borderLeftWidth",
  "borderRightWidth",
  "borderTopWidth",
  "boxSizing",
  "fontFamily",
  "fontFeatureSettings",
  "fontKerning",
  "fontSize",
  "fontStretch",
  "fontStyle",
  "fontVariant",
  "fontWeight",
  "letterSpacing",
  "lineHeight",
  "paddingBottom",
  "paddingLeft",
  "paddingRight",
  "paddingTop",
  "tabSize",
  "textIndent",
  "textRendering",
  "textTransform",
  "whiteSpace",
  "wordBreak",
  "wordSpacing",
  "overflowWrap",
] as const;

function measureTextareaCaretPosition(
  textarea: HTMLTextAreaElement,
  caretIndex: number,
): CaretAnchorPosition {
  const computed = window.getComputedStyle(textarea);
  const mirror = document.createElement("div");
  for (const key of TEXTAREA_MIRROR_STYLE_KEYS) {
    mirror.style[key] = computed[key];
  }
  mirror.style.position = "absolute";
  mirror.style.visibility = "hidden";
  mirror.style.pointerEvents = "none";
  mirror.style.top = "0";
  mirror.style.left = "-9999px";
  mirror.style.whiteSpace = "pre-wrap";
  mirror.style.wordWrap = "break-word";
  mirror.style.overflow = "hidden";
  mirror.style.width = `${textarea.clientWidth}px`;
  mirror.textContent = textarea.value.slice(0, caretIndex);

  if (textarea.value.charAt(caretIndex - 1) === "\n") {
    mirror.textContent += "\u200b";
  }

  const marker = document.createElement("span");
  marker.textContent = textarea.value.slice(caretIndex) || "\u200b";
  mirror.appendChild(marker);
  document.body.appendChild(mirror);

  const textareaRect = normalizeClientRectForFixedPosition(
    textarea.getBoundingClientRect(),
  );
  const top = textareaRect.top + marker.offsetTop - textarea.scrollTop;
  const left = textareaRect.left + marker.offsetLeft - textarea.scrollLeft;
  const width = Math.min(360, Math.max(240, textareaRect.width));

  document.body.removeChild(mirror);
  return { top, left, width };
}

export function ChatComposerMenu({
  visible,
  queryKey,
  items,
  anchorRef,
  caretIndex,
  activeIndex,
  onSelect,
  onDismiss,
  onActiveChange,
}: ChatComposerMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ bottom: 0, left: 0, width: 0 });

  useLayoutEffect(() => {
    if (!visible || !anchorRef.current) return;
    const anchor = measureTextareaCaretPosition(anchorRef.current, caretIndex);
    const viewport = readFixedViewportSize();
    const width = Math.min(anchor.width, viewport.width - 16);
    setPos({
      bottom: viewport.height - anchor.top + 10,
      left: Math.max(8, Math.min(anchor.left, viewport.width - width - 8)),
      width,
    });
  }, [anchorRef, caretIndex, queryKey, visible]);

  // Close on outside click
  useEffect(() => {
    if (!visible) return;

    function onPointerDown(e: PointerEvent) {
      if (menuRef.current?.contains(e.target as Node)) return;
      onDismiss();
    }

    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [visible, onDismiss]);

  // Scroll active item into view
  useEffect(() => {
    if (!visible) return;
    const activeEl = menuRef.current?.querySelector(
      `[data-composer-index="${activeIndex}"]`,
    );
    activeEl?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, visible]);

  if (!visible || items.length === 0) return null;

  return createPortal(
    <div
      ref={menuRef}
      className="slash-menu"
      style={{
        position: "fixed",
        zIndex: 1400,
        bottom: pos.bottom,
        left: pos.left,
        width: pos.width,
      }}
    >
      {items.map((item, i) => {
        const Icon = item.icon;
        const isActive = i === activeIndex;
        return (
          <button
            key={item.id}
            type="button"
            data-composer-index={i}
            className={`slash-menu-item${isActive ? " slash-menu-item-active" : ""}${item.disabled ? " slash-menu-item-disabled" : ""}`}
            onPointerEnter={() => onActiveChange(i)}
            onClick={() => {
              if (!item.disabled) onSelect(item.id);
            }}
            disabled={item.disabled}
          >
            <span className="slash-menu-item-icon">
              <Icon size={14} />
            </span>
            <span className="slash-menu-item-text">
              <span className="slash-menu-item-name">{item.name}</span>
              <span className="slash-menu-item-desc">{item.description}</span>
            </span>
            {item.badge && (
              <span className="slash-menu-item-badge">{item.badge}</span>
            )}
          </button>
        );
      })}
    </div>,
    document.body,
  );
}
