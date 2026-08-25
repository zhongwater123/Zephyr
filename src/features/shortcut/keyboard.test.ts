import { describe, expect, it } from "vitest";
import { keyLabelFromCode, shortcutFromKeyboardEvent } from "./keyboard";

function keyboardEvent(overrides: Partial<KeyboardEvent> = {}) {
  return {
    altKey: false,
    code: "KeyA",
    ctrlKey: false,
    key: "a",
    metaKey: false,
    repeat: false,
    shiftKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe("shortcut keyboard capture", () => {
  it("uses the layout-resolved virtual key instead of the physical code", () => {
    expect(
      shortcutFromKeyboardEvent(
        keyboardEvent({ code: "KeyQ", key: "a", ctrlKey: true }),
      ),
    ).toBe("Ctrl+A");
  });

  it("ignores keyboard auto-repeat", () => {
    expect(shortcutFromKeyboardEvent(keyboardEvent({ ctrlKey: true, repeat: true }))).toBe("");
  });

  it("keeps named keys independent from printable layout", () => {
    expect(keyLabelFromCode("ArrowLeft", "ArrowLeft")).toBe("ArrowLeft");
    expect(keyLabelFromCode("ControlLeft", "Control")).toBe("");
  });
});
