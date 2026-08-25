export type ShortcutKeyEvent = Pick<
  KeyboardEvent,
  "altKey" | "code" | "ctrlKey" | "key" | "metaKey" | "repeat" | "shiftKey"
>;

export function shortcutFromKeyboardEvent(event: ShortcutKeyEvent) {
  if (event.repeat) return "";
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Win");
  const key = keyLabelFromCode(event.code, event.key);
  if (key) parts.push(key);
  return parts.join("+");
}

export function keyLabelFromCode(code: string, key: string) {
  if (
    [
      "ControlLeft",
      "ControlRight",
      "AltLeft",
      "AltRight",
      "ShiftLeft",
      "ShiftRight",
      "MetaLeft",
      "MetaRight",
    ].includes(code)
  ) {
    return "";
  }
  if (code === "Space") return "Space";
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const named: Record<string, string> = {
    Tab: "Tab",
    Enter: "Enter",
    Escape: "Escape",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
  };
  if (named[code]) return named[code];
  // RegisterHotKey consumes virtual-key semantics. `key` represents the
  // layout-resolved key the user saw, while `code` is only its physical slot.
  if (key.length === 1 && /[a-z0-9]/i.test(key)) return key.toUpperCase();
  return "";
}
