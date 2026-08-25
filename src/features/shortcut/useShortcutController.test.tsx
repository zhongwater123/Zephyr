// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { useState } from "preact/hooks";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultConfig, type AppConfig, type ShortcutPreview } from "../../domain";

const mocks = vi.hoisted(() => ({
  pending: new Map<string, (value: ShortcutPreview) => void>(),
  cancel: vi.fn(async () => undefined),
  commit: vi.fn(async (_previewId: number, _revision: number) => defaultConfig as AppConfig),
  preview: vi.fn(async (_previewId: number): Promise<ShortcutPreview> => {
    throw new Error("preview not configured");
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock("../../ipc/client", () => ({
  shortcutApi: {
    prepare: vi.fn(
      (shortcut: string, mode: "standard" | "exclusive_hook") =>
        new Promise<ShortcutPreview>((resolve) => mocks.pending.set(`${shortcut}:${mode}`, resolve)),
    ),
    commit: mocks.commit,
    cancel: mocks.cancel,
    preview: mocks.preview,
    status: vi.fn(async () => ({
      shortcut: "Ctrl+Shift+Space",
      mode: "standard",
      backend: "register_hotkey",
      state: "active",
      message: "标准快捷键已生效。",
    })),
  },
}));

import { useShortcutController } from "./useShortcutController";

afterEach(() => {
  mocks.pending.clear();
  mocks.cancel.mockClear();
  mocks.commit.mockClear();
  mocks.preview.mockReset();
  mocks.preview.mockRejectedValue(new Error("preview not configured"));
  cleanup();
});

function Harness() {
  const [config, setConfig] = useState(defaultConfig);
  const control = useShortcutController(config, setConfig, vi.fn(), String);
  return (
    <div>
      <button onClick={() => void control.prepareShortcutCandidate("Ctrl+A")}>first</button>
      <button onClick={() => void control.prepareShortcutCandidate("Ctrl+B")}>second</button>
      <button onClick={() => void control.prepareShortcutCandidate("Alt+V", "standard")}>occupied</button>
      <button onClick={control.takeExclusiveControl}>exclusive</button>
      <span data-testid="draft">{control.shortcutDraft}</span>
      <span data-testid="prepared">{control.shortcutPreview?.normalized ?? ""}</span>
    </div>
  );
}

function result(
  id: number,
  shortcut: string,
  mode: "standard" | "exclusive_hook" = "standard",
  state: ShortcutPreview["state"] = "reserved_standard",
): ShortcutPreview {
  return {
    previewId: id,
    shortcut,
    normalized: shortcut,
    mode,
    state,
    reason: "候选状态",
  };
}

describe("useShortcutController", () => {
  it("auto-commits the latest reserved shortcut and cancels a stale response", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "first" }));
    fireEvent.click(screen.getByRole("button", { name: "second" }));

    await act(async () => mocks.pending.get("Ctrl+B:standard")?.(result(2, "Ctrl+B")));
    expect(mocks.commit).toHaveBeenCalledWith(2, defaultConfig.revision);

    await act(async () => mocks.pending.get("Ctrl+A:standard")?.(result(1, "Ctrl+A")));
    expect(screen.getByTestId("draft").textContent).toBe("Ctrl+B");
    expect(mocks.cancel).toHaveBeenCalledWith(1);
  });

  it("commits exclusive mode directly after the user accepts an occupied shortcut", async () => {
    mocks.preview.mockResolvedValue(result(3, "Alt+V", "exclusive_hook", "hook_verified"));
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "occupied" }));
    await act(async () => mocks.pending
      .get("Alt+V:standard")
      ?.(result(2, "Alt+V", "standard", "occupied")));

    fireEvent.click(screen.getByRole("button", { name: "exclusive" }));
    await act(async () => mocks.pending
      .get("Alt+V:exclusive_hook")
      ?.(result(3, "Alt+V", "exclusive_hook", "hook_verified")));

    await waitFor(() => expect(mocks.preview).toHaveBeenCalledWith(3));
    await waitFor(() => expect(mocks.commit).toHaveBeenCalledWith(3, defaultConfig.revision));
  });
});
