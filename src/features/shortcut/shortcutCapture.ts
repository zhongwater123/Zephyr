import type { ShortcutBinding, ShortcutErrorCode } from "../../domain";

export type ModifierCode =
  | "ControlLeft"
  | "ControlRight"
  | "AltLeft"
  | "AltRight"
  | "ShiftLeft"
  | "ShiftRight"
  | "MetaLeft"
  | "MetaRight";

type ModifierBinding = ShortcutBinding["modifiers"][number];

type KeyDefinition = {
  scanCode: number;
  extended: boolean;
  label: string;
  standalone: boolean;
};

export type ShortcutCandidate = {
  binding: ShortcutBinding;
  label: string;
  codes: string[];
};

export type CandidateResult =
  | { candidate: ShortcutCandidate; error?: never }
  | {
      candidate?: never;
      error: { code: Extract<ShortcutErrorCode, "invalid_binding" | "reserved_binding">; message: string };
      label: string;
    };

const MODIFIER_ORDER: ModifierCode[] = [
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
];

const MODIFIER_TRIGGER_PRIORITY: ModifierCode[] = [
  "ShiftRight",
  "ShiftLeft",
  "AltRight",
  "AltLeft",
  "ControlRight",
  "ControlLeft",
  "MetaRight",
  "MetaLeft",
];

const MODIFIERS: Record<ModifierCode, ModifierBinding & { scanCode: number; extended: boolean; label: string }> = {
  ControlLeft: { kind: "control", side: "left", scanCode: 0x1d, extended: false, label: "左 Ctrl" },
  ControlRight: { kind: "control", side: "right", scanCode: 0x1d, extended: true, label: "右 Ctrl" },
  AltLeft: { kind: "alt", side: "left", scanCode: 0x38, extended: false, label: "左 Alt" },
  AltRight: { kind: "alt", side: "right", scanCode: 0x38, extended: true, label: "右 Alt" },
  ShiftLeft: { kind: "shift", side: "left", scanCode: 0x2a, extended: false, label: "左 Shift" },
  ShiftRight: { kind: "shift", side: "right", scanCode: 0x36, extended: false, label: "右 Shift" },
  MetaLeft: { kind: "win", side: "left", scanCode: 0x5b, extended: true, label: "左 Win" },
  MetaRight: { kind: "win", side: "right", scanCode: 0x5c, extended: true, label: "右 Win" },
};

const MAIN_KEYS = new Map<string, KeyDefinition>();

function add(
  code: string,
  scanCode: number,
  label: string,
  extended = false,
  standalone = false,
) {
  MAIN_KEYS.set(code, { scanCode, extended, label, standalone });
}

for (const [code, scanCode, label] of [
  ["Escape", 0x01, "Escape"],
  ["Digit1", 0x02, "1"],
  ["Digit2", 0x03, "2"],
  ["Digit3", 0x04, "3"],
  ["Digit4", 0x05, "4"],
  ["Digit5", 0x06, "5"],
  ["Digit6", 0x07, "6"],
  ["Digit7", 0x08, "7"],
  ["Digit8", 0x09, "8"],
  ["Digit9", 0x0a, "9"],
  ["Digit0", 0x0b, "0"],
  ["Minus", 0x0c, "Minus"],
  ["Equal", 0x0d, "Equal"],
  ["KeyQ", 0x10, "Q"],
  ["KeyW", 0x11, "W"],
  ["KeyE", 0x12, "E"],
  ["KeyR", 0x13, "R"],
  ["KeyT", 0x14, "T"],
  ["KeyY", 0x15, "Y"],
  ["KeyU", 0x16, "U"],
  ["KeyI", 0x17, "I"],
  ["KeyO", 0x18, "O"],
  ["KeyP", 0x19, "P"],
  ["BracketLeft", 0x1a, "BracketLeft"],
  ["BracketRight", 0x1b, "BracketRight"],
  ["Enter", 0x1c, "Enter"],
  ["KeyA", 0x1e, "A"],
  ["KeyS", 0x1f, "S"],
  ["KeyD", 0x20, "D"],
  ["KeyF", 0x21, "F"],
  ["KeyG", 0x22, "G"],
  ["KeyH", 0x23, "H"],
  ["KeyJ", 0x24, "J"],
  ["KeyK", 0x25, "K"],
  ["KeyL", 0x26, "L"],
  ["Semicolon", 0x27, "Semicolon"],
  ["Quote", 0x28, "Quote"],
  ["Backquote", 0x29, "Backquote"],
  ["Backslash", 0x2b, "Backslash"],
  ["KeyZ", 0x2c, "Z"],
  ["KeyX", 0x2d, "X"],
  ["KeyC", 0x2e, "C"],
  ["KeyV", 0x2f, "V"],
  ["KeyB", 0x30, "B"],
  ["KeyN", 0x31, "N"],
  ["KeyM", 0x32, "M"],
  ["Comma", 0x33, "Comma"],
  ["Period", 0x34, "Period"],
  ["Slash", 0x35, "Slash"],
  ["IntlBackslash", 0x56, "IntlBackslash"],
] as const) {
  add(code, scanCode, label);
}

add("Backspace", 0x0e, "Backspace", false, true);
add("Tab", 0x0f, "Tab", false, true);
add("Space", 0x39, "Space", false, true);
add("NumpadEnter", 0x1c, "NumpadEnter", true);
add("NumpadDivide", 0x35, "NumpadDivide", true);
add("NumpadMultiply", 0x37, "NumpadMultiply");
add("PrintScreen", 0x37, "PrintScreen", true, true);
add("Pause", 0x45, "Pause", false, true);
add("NumLock", 0x45, "NumLock", true, true);
add("ScrollLock", 0x46, "ScrollLock", false, true);
add("Numpad7", 0x47, "Numpad7");
add("Home", 0x47, "Home", true, true);
add("Numpad8", 0x48, "Numpad8");
add("ArrowUp", 0x48, "ArrowUp", true, true);
add("Numpad9", 0x49, "Numpad9");
add("PageUp", 0x49, "PageUp", true, true);
add("NumpadSubtract", 0x4a, "NumpadSubtract");
add("Numpad4", 0x4b, "Numpad4");
add("ArrowLeft", 0x4b, "ArrowLeft", true, true);
add("Numpad5", 0x4c, "Numpad5");
add("Numpad6", 0x4d, "Numpad6");
add("ArrowRight", 0x4d, "ArrowRight", true, true);
add("NumpadAdd", 0x4e, "NumpadAdd");
add("Numpad1", 0x4f, "Numpad1");
add("End", 0x4f, "End", true, true);
add("Numpad2", 0x50, "Numpad2");
add("ArrowDown", 0x50, "ArrowDown", true, true);
add("Numpad3", 0x51, "Numpad3");
add("PageDown", 0x51, "PageDown", true, true);
add("Numpad0", 0x52, "Numpad0");
add("Insert", 0x52, "Insert", true, true);
add("NumpadDecimal", 0x53, "NumpadDecimal");
add("Delete", 0x53, "Delete", true, true);

for (let number = 1; number <= 10; number += 1) {
  add("F" + number, 0x3a + number, "F" + number, false, true);
}
add("F11", 0x57, "F11", false, true);
add("F12", 0x58, "F12");
for (let number = 13; number <= 23; number += 1) {
  add("F" + number, 0x64 + number - 13, "F" + number, false, true);
}
add("F24", 0x76, "F24", false, true);

export function isModifierCode(code: string): code is ModifierCode {
  return Object.prototype.hasOwnProperty.call(MODIFIERS, code);
}

export function orderedModifierCodes(codes: Iterable<string>): ModifierCode[] {
  const set = new Set(codes);
  return MODIFIER_ORDER.filter((code) => set.has(code));
}

export function modifierLabels(codes: Iterable<string>): string[] {
  return orderedModifierCodes(codes).map((code) => MODIFIERS[code].label);
}

function modifierBinding(code: ModifierCode): ModifierBinding {
  const { kind, side } = MODIFIERS[code];
  return { kind, side };
}

function modifierPhysicalKey(code: ModifierCode) {
  const { scanCode, extended } = MODIFIERS[code];
  return { scanCode, extended };
}

function rejected(
  code: "invalid_binding" | "reserved_binding",
  message: string,
  label: string,
): CandidateResult {
  return { error: { code, message }, label };
}

function validateCandidate(candidate: ShortcutCandidate, triggerCode?: string): CandidateResult {
  if (candidate.codes.length > 3) {
    return rejected("invalid_binding", "快捷键最多支持三个物理按键。", candidate.label);
  }
  const kinds = new Set(candidate.binding.modifiers.map((modifier) => modifier.kind));
  if (triggerCode === "F12") {
    return rejected(
      "reserved_binding",
      "F12 由 Windows 调试器保留，不能作为语音快捷键。",
      candidate.label,
    );
  }
  if (triggerCode === "Delete" && kinds.has("control") && kinds.has("alt")) {
    return rejected("reserved_binding", "Ctrl+Alt+Delete 是系统安全快捷键。", candidate.label);
  }
  if (triggerCode === "Escape" && kinds.has("control") && kinds.has("shift")) {
    return rejected("reserved_binding", "Ctrl+Shift+Escape 是系统任务管理器快捷键。", candidate.label);
  }
  if (triggerCode === "Tab" && kinds.has("alt")) {
    return rejected("reserved_binding", "Alt+Tab 是系统切换窗口快捷键。", candidate.label);
  }
  if (triggerCode === "F4" && kinds.has("alt")) {
    return rejected("reserved_binding", "Alt+F4 是系统关闭窗口快捷键。", candidate.label);
  }
  if (triggerCode === "KeyL" && kinds.has("win")) {
    return rejected("reserved_binding", "Win+L 是系统锁屏快捷键。", candidate.label);
  }
  return { candidate };
}

export function buildMainCandidate(
  modifierCodes: Iterable<string>,
  triggerCode: string,
): CandidateResult {
  const modifiers = orderedModifierCodes(modifierCodes);
  const trigger = MAIN_KEYS.get(triggerCode);
  const labels = modifierLabels(modifiers);
  const label = [...labels, trigger?.label ?? triggerCode].join("+");
  if (!trigger) {
    return rejected("invalid_binding", "暂不支持按键 " + triggerCode + "。", label);
  }
  const candidate: ShortcutCandidate = {
    binding: {
      modifiers: modifiers.map(modifierBinding),
      trigger: { scanCode: trigger.scanCode, extended: trigger.extended },
    },
    label,
    codes: [...modifiers, triggerCode],
  };
  const validated = validateCandidate(candidate, triggerCode);
  if (validated.error) {
    return validated;
  }
  if (modifiers.length === 0 && !trigger.standalone) {
    return rejected(
      "invalid_binding",
      "字母、数字、标点和 Enter 需要与修饰键组合使用。",
      label,
    );
  }
  return validated;
}

export function buildModifierCandidate(codes: Iterable<string>): CandidateResult {
  const modifiers = orderedModifierCodes(codes);
  const label = modifierLabels(modifiers).join("+");
  if (modifiers.length === 0) {
    return rejected("invalid_binding", "请按下快捷键。", "");
  }
  if (
    modifiers.length === 1
    && !["ControlRight", "AltRight", "ShiftRight"].includes(modifiers[0])
  ) {
    return rejected(
      "invalid_binding",
      "单独使用修饰键时，仅支持右 Ctrl、右 Alt 或右 Shift。",
      label,
    );
  }
  const triggerCode = MODIFIER_TRIGGER_PRIORITY.find((code) => modifiers.includes(code));
  if (!triggerCode) {
    return rejected("invalid_binding", "无法识别修饰键组合。", label);
  }
  const candidate: ShortcutCandidate = {
    binding: {
      modifiers: modifiers
        .filter((code) => code !== triggerCode)
        .map(modifierBinding),
      trigger: modifierPhysicalKey(triggerCode),
    },
    label,
    codes: modifiers,
  };
  return validateCandidate(candidate);
}
