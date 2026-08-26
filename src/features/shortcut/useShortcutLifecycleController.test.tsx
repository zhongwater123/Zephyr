// @vitest-environment happy-dom

import { act } from "preact/test-utils";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { useState } from "preact/hooks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultConfig } from "../../domain";
import type { ShortcutLifecycleSnapshot } from "../../domain";

const initialSnapshot: ShortcutLifecycleSnapshot = {
  sequence: 1,
  configRevision: defaultConfig.revision,
  runtime: {
    state: "active",
    activeLabel: defaultConfig.shortcut,
    activeBinding: defaultConfig.shortcut_binding ?? null,
    message: "物理快捷键已启用。",
  },
  operation: null,
};

function captureSnapshot(
  sequence: number,
  phase: NonNullable<ShortcutLifecycleSnapshot["operation"]>["phase"],
  candidateLabel?: string,
): ShortcutLifecycleSnapshot {
  return {
    sequence,
    configRevision: phase === "succeeded" ? 1 : 0,
    runtime: {
      state: ["starting", "capturing", "validating", "applying"].includes(phase)
        ? "suspended"
        : "active",
      activeLabel: phase === "succeeded" ? "左 Ctrl+右 Alt+V" : defaultConfig.shortcut,
      activeBinding: defaultConfig.shortcut_binding ?? null,
      message: "物理快捷键已启用。",
    },
    operation: {
      operationId: 7,
      kind: "capture",
      phase,
      candidateLabel,
      message: phase === "failed"
        ? "无法保存新的快捷键。"
        : phase === "succeeded"
          ? "快捷键已启用。"
          : "正在处理。",
      errorCode: phase === "failed" ? "persistence_failed" : undefined,
      retryable: phase === "failed",
      changed: phase === "succeeded" ? true : undefined,
    },
  };
}

const mocks = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  startCapture: vi.fn(),
  cancelOperation: vi.fn(),
  get: vi.fn(),
  undo: vi.fn(),
  restoreDefault: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(name, handler);
    return vi.fn();
  }),
}));

vi.mock("../../ipc/client", () => ({
  shortcutLifecycleApi: {
    startCapture: mocks.startCapture,
    cancelOperation: mocks.cancelOperation,
    get: mocks.get,
    undo: mocks.undo,
    restoreDefault: mocks.restoreDefault,
  },
}));

import { useShortcutLifecycleController } from "./useShortcutLifecycleController";

beforeEach(() => {
  mocks.listeners.clear();
  vi.clearAllMocks();
  mocks.get.mockResolvedValue(initialSnapshot);
  mocks.startCapture.mockResolvedValue(captureSnapshot(2, "capturing"));
  mocks.cancelOperation.mockResolvedValue(captureSnapshot(9, "cancelled"));
  mocks.restoreDefault.mockResolvedValue(initialSnapshot);
  mocks.undo.mockResolvedValue(initialSnapshot);
});

afterEach(cleanup);

function Harness() {
  const [config, setConfig] = useState(defaultConfig);
  const control = useShortcutLifecycleController(config, setConfig, vi.fn(), String);
  return <div>
    <button onClick={() => void control.beginShortcutCapture()}>start</button>
    <button onClick={() => void control.cancelShortcutOperation()}>cancel</button>
    <button onClick={() => void control.closeShortcutSession()}>close</button>
    <span data-testid="shortcut">{config.shortcut}</span>
    <span data-testid="candidate">{control.shortcutView.displayLabel}</span>
    <span data-testid="phase">{control.shortcutView.phase ?? "idle"}</span>
    <span data-testid="message">{control.shortcutView.message}</span>
  </div>;
}

describe("useShortcutLifecycleController", () => {
  it("enters capture only after the backend snapshot acknowledges it", async () => {
    let resolveStart: ((snapshot: ShortcutLifecycleSnapshot) => void) | undefined;
    mocks.startCapture.mockImplementationOnce(
      () => new Promise((resolve) => {
        resolveStart = resolve;
      }),
    );
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    expect(screen.getByTestId("phase").textContent).toBe("idle");
    await act(async () => resolveStart?.(captureSnapshot(2, "capturing")));
    expect(screen.getByTestId("phase").textContent).toBe("capturing");
  });

  it("shows live candidates and rolls application failure back to the active shortcut", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(screen.getByTestId("phase").textContent).toBe("capturing"));
    await act(async () => {
      mocks.listeners.get("shortcut_lifecycle_changed")?.({
        payload: captureSnapshot(3, "capturing", "左 Ctrl+右 Alt+V"),
      });
    });
    expect(screen.getByTestId("candidate").textContent).toBe("左 Ctrl+右 Alt+V");
    await act(async () => {
      mocks.listeners.get("shortcut_lifecycle_changed")?.({
        payload: captureSnapshot(4, "failed", "左 Ctrl+右 Alt+V"),
      });
    });
    expect(screen.getByTestId("candidate").textContent).toBe(defaultConfig.shortcut);
    expect(screen.getByTestId("message").textContent).toBe("无法保存新的快捷键。");
  });

  it("keeps an invalid candidate in the same capture operation for immediate retry", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(screen.getByTestId("phase").textContent).toBe("capturing"));
    const rejected = captureSnapshot(4, "capturing", "C");
    if (rejected.operation) {
      rejected.operation.message = "快捷键至少需要一个修饰键。";
      rejected.operation.errorCode = "invalid_binding";
      rejected.operation.retryable = true;
    }
    await act(async () => {
      mocks.listeners.get("shortcut_lifecycle_changed")?.({
        payload: rejected,
      });
    });
    expect(screen.getByTestId("phase").textContent).toBe("capturing");
    expect(screen.getByTestId("candidate").textContent).toBe("C");
    expect(screen.getByTestId("message").textContent).toBe("快捷键至少需要一个修饰键。");
  });

  it("recovers a missed terminal event through lifecycle reconciliation", async () => {
    mocks.get
      .mockResolvedValueOnce(initialSnapshot)
      .mockResolvedValueOnce(captureSnapshot(4, "succeeded", "左 Ctrl+右 Alt+V"));
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(mocks.get).toHaveBeenCalledWith(7));
    await waitFor(() => expect(screen.getByTestId("phase").textContent).toBe("succeeded"));
    expect(screen.getByTestId("shortcut").textContent).toBe("左 Ctrl+右 Alt+V");
  });

  it("recovers a missed candidate event through lifecycle reconciliation", async () => {
    mocks.get
      .mockResolvedValueOnce(initialSnapshot)
      .mockResolvedValueOnce(captureSnapshot(4, "capturing", "左 Ctrl+右 Shift+Space"));
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(mocks.get).toHaveBeenCalledWith(7));
    await waitFor(() => {
      expect(screen.getByTestId("candidate").textContent).toBe("左 Ctrl+右 Shift+Space");
    });
  });

  it("surfaces an acknowledged Hook failure without claiming the old binding is active", async () => {
    const failure = captureSnapshot(2, "failed");
    failure.runtime = {
      ...failure.runtime,
      state: "error",
      message: "键盘 Hook 不可用。",
    };
    if (failure.operation) {
      failure.operation.errorCode = "hook_unavailable";
      failure.operation.message = "键盘 Hook 不可用。";
    }
    mocks.startCapture.mockResolvedValueOnce(failure);
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(screen.getByTestId("phase").textContent).toBe("failed"));
    expect(screen.getByTestId("message").textContent).toBe("键盘 Hook 不可用。");
  });

  it("rejects out-of-order events for the current operation", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(screen.getByTestId("phase").textContent).toBe("capturing"));
    await act(async () => {
      mocks.listeners.get("shortcut_lifecycle_changed")?.({
        payload: captureSnapshot(5, "validating", "左 Ctrl+V"),
      });
      mocks.listeners.get("shortcut_lifecycle_changed")?.({
        payload: captureSnapshot(4, "capturing", "C"),
      });
    });
    expect(screen.getByTestId("phase").textContent).toBe("validating");
    expect(screen.getByTestId("candidate").textContent).toBe("左 Ctrl+V");
  });

  it("cancels an active operation returned by a late start response", async () => {
    let resolveStart: ((snapshot: ShortcutLifecycleSnapshot) => void) | undefined;
    mocks.startCapture.mockImplementationOnce(
      () => new Promise((resolve) => {
        resolveStart = resolve;
      }),
    );
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    fireEvent.click(screen.getByRole("button", { name: "close" }));
    await act(async () => resolveStart?.(captureSnapshot(2, "capturing")));
    await waitFor(() => expect(mocks.cancelOperation).toHaveBeenCalledWith(7));
  });

  it("cancels the active operation when the controller unmounts", async () => {
    const rendered = render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "start" }));
    await waitFor(() => expect(screen.getByTestId("phase").textContent).toBe("capturing"));
    rendered.unmount();
    await waitFor(() => expect(mocks.cancelOperation).toHaveBeenCalledWith(7));
  });
});
