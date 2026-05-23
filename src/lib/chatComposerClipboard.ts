export const CHAT_COMPOSER_DATA_ATTRIBUTE = "data-chat-composer";
export const CHAT_COMPOSER_DATA_VALUE = "true";
export const CHAT_COMPOSER_NATIVE_IMAGE_PASTE_EVENT =
  "supacodex:chat-composer-native-image-paste";

export interface ChatComposerNativeImagePasteDetail {
  files: File[];
}

export interface ChatComposerPasteShortcutLike {
  altKey: boolean;
  ctrlKey: boolean;
  key: string;
  metaKey: boolean;
  shiftKey: boolean;
}

export function isChatComposerElement(
  value: EventTarget | Element | null | undefined,
): value is HTMLElement {
  return (
    value instanceof HTMLElement
    && value.getAttribute(CHAT_COMPOSER_DATA_ATTRIBUTE) === CHAT_COMPOSER_DATA_VALUE
  );
}

export function isChatComposerPasteShortcut(
  event: ChatComposerPasteShortcutLike,
  activeElement: EventTarget | Element | null | undefined,
): boolean {
  return (
    isChatComposerElement(activeElement)
    && !event.altKey
    && !event.shiftKey
    && (event.metaKey || event.ctrlKey)
    && event.key.toLowerCase() === "v"
  );
}

export function dispatchChatComposerNativeImagePaste(files: File[]): void {
  window.dispatchEvent(
    new CustomEvent<ChatComposerNativeImagePasteDetail>(
      CHAT_COMPOSER_NATIVE_IMAGE_PASTE_EVENT,
      {
        detail: { files },
      },
    ),
  );
}
