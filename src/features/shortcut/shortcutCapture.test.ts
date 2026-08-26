import { describe, expect, it } from "vitest";
import { buildMainCandidate, buildModifierCandidate } from "./shortcutCapture";

describe("shortcutCapture", () => {
  it("preserves the physical side of every modifier", () => {
    const result = buildMainCandidate(["ControlLeft", "ShiftRight"], "KeyK");
    expect(result.candidate?.binding).toEqual({
      modifiers: [
        { kind: "control", side: "left" },
        { kind: "shift", side: "right" },
      ],
      trigger: { scanCode: 0x25, extended: false },
    });
    expect(result.candidate?.label).toBe("左 Ctrl+右 Shift+K");
  });

  it("allows ordinary combinations and supported standalone keys", () => {
    expect(buildMainCandidate(["ControlLeft"], "KeyC").candidate?.label).toBe("左 Ctrl+C");
    expect(buildMainCandidate([], "Space").candidate?.label).toBe("Space");
    expect(buildMainCandidate([], "F24").candidate?.label).toBe("F24");
  });

  it("keeps an unmodified character in capture with a validation reason", () => {
    const result = buildMainCandidate([], "KeyK");
    expect(result.error?.code).toBe("invalid_binding");
    expect(result.label).toBe("K");
  });

  it("rejects Windows-reserved combinations before IPC commit", () => {
    expect(buildMainCandidate([], "F12").error?.code).toBe("reserved_binding");
    expect(buildMainCandidate(["AltLeft"], "Tab").error?.code).toBe("reserved_binding");
    expect(buildMainCandidate(["MetaLeft"], "KeyL").error?.code).toBe("reserved_binding");
  });

  it("allows only right Ctrl, right Alt or right Shift as a lone modifier", () => {
    expect(buildModifierCandidate(["ControlLeft"]).error?.code).toBe("invalid_binding");
    expect(buildModifierCandidate(["ControlRight"]).candidate?.binding).toEqual({
      modifiers: [],
      trigger: { scanCode: 0x1d, extended: true },
    });
  });

  it("rejects candidates containing more than three physical keys", () => {
    const result = buildMainCandidate(
      ["ControlLeft", "AltLeft", "ShiftRight"],
      "KeyK",
    );
    expect(result.error?.code).toBe("invalid_binding");
  });
});
