// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ShortcutCaptureField } from "./ShortcutCaptureField";
import { selectShortcutLifecycle } from "./shortcutLifecycle";

afterEach(cleanup);

const shortcut = "左 Ctrl+左 Alt+Space";
const activeSnapshot = {
  sequence: 1,
  configRevision: 1,
  runtime: {
    state: "active" as const,
    activeLabel: shortcut,
    activeBinding: null,
    message: "物理快捷键已启用。",
  },
  operation: null,
};

describe("ShortcutCaptureField", () => {
  it("shows preparation separately and hides old keycaps inside the capture field", () => {
    render(
      <ShortcutCaptureField
        view={selectShortcutLifecycle({
          ...activeSnapshot,
          sequence: 2,
          runtime: { ...activeSnapshot.runtime, state: "suspended" },
          operation: {
            operationId: 1,
            kind: "capture",
            phase: "starting",
            message: "正在准备快捷键录制。",
            retryable: false,
          },
        }, shortcut)}
        requestPending
        transportError=""
        onStart={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText("正在准备换绑…")).toBeTruthy();
    expect(screen.getByText("正在准备键盘 Hook，请稍候。")).toBeTruthy();
    expect(screen.queryByText(/输入会实时显示/)).toBeNull();
    expect(screen.getByText("正在准备")).toBeTruthy();
    expect(screen.queryByText("Space")).toBeNull();
  });

  it("uses the visible shortcut value as the single capture entry", () => {
    const onStart = vi.fn();
    render(
      <ShortcutCaptureField
        view={selectShortcutLifecycle(activeSnapshot, shortcut)}
        requestPending={false}
        transportError=""
        onStart={onStart}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText("左 Ctrl")).toBeTruthy();
    expect(screen.getByText("左 Alt")).toBeTruthy();
    expect(screen.getByText("Space")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /点击重新设置/ }));
    expect(onStart).toHaveBeenCalledOnce();
  });

  it("cancels on a second pointer click and suppresses its synthesized click", () => {
    const onStart = vi.fn();
    const onCancel = vi.fn();
    render(
      <ShortcutCaptureField
        view={selectShortcutLifecycle({
          ...activeSnapshot,
          sequence: 2,
          runtime: { ...activeSnapshot.runtime, state: "suspended" },
          operation: {
            operationId: 1,
            kind: "capture",
            phase: "capturing",
            message: "请按下新的物理快捷键。",
            retryable: false,
          },
        }, shortcut)}
        requestPending={false}
        transportError=""
        onStart={onStart}
        onCancel={onCancel}
      />,
    );
    const field = screen.getByRole("button", { name: /正在设置快捷键/ });
    expect(screen.getByText("输入快捷键")).toBeTruthy();
    fireEvent.pointerDown(field);
    fireEvent.click(field);
    expect(onStart).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledOnce();
    fireEvent.keyDown(field, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("cancels on outside pointer input without swallowing the requested action", () => {
    const onCancel = vi.fn();
    const outsideAction = vi.fn();
    render(
      <>
        <ShortcutCaptureField
          view={selectShortcutLifecycle({
            ...activeSnapshot,
            sequence: 2,
            runtime: { ...activeSnapshot.runtime, state: "suspended" },
            operation: {
              operationId: 1,
              kind: "capture",
              phase: "capturing",
              message: "请按下新的物理快捷键。",
              retryable: false,
            },
          }, shortcut)}
          requestPending={false}
          transportError=""
          onStart={vi.fn()}
          onCancel={onCancel}
        />
        <button type="button" onClick={outsideAction}>其他设置</button>
      </>,
    );
    fireEvent.pointerDown(screen.getByRole("button", { name: "其他设置" }));
    fireEvent.click(screen.getByRole("button", { name: "其他设置" }));
    expect(onCancel).toHaveBeenCalledOnce();
    expect(outsideAction).toHaveBeenCalledOnce();
  });

  it("shows the accumulated backend candidate while capturing", () => {
    render(
      <ShortcutCaptureField
        view={selectShortcutLifecycle({
          ...activeSnapshot,
          sequence: 3,
          runtime: { ...activeSnapshot.runtime, state: "suspended" },
          operation: {
            operationId: 1,
            kind: "capture",
            phase: "capturing",
            candidateLabel: "左 Ctrl+左 Shift+Space",
            message: "请按下新的物理快捷键。",
            retryable: false,
          },
        }, shortcut)}
        requestPending={false}
        transportError=""
        onStart={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText("左 Ctrl")).toBeTruthy();
    expect(screen.getByText("左 Shift")).toBeTruthy();
    expect(screen.getByText("Space")).toBeTruthy();
  });

  it("keeps the capture field locked during validation without locking the settings page", () => {
    const onCancel = vi.fn();
    const outsideAction = vi.fn();
    render(
      <>
        <ShortcutCaptureField
          view={selectShortcutLifecycle({
            ...activeSnapshot,
            sequence: 4,
            runtime: { ...activeSnapshot.runtime, state: "suspended" },
            operation: {
              operationId: 1,
              kind: "capture",
              phase: "validating",
              candidateLabel: "左 Ctrl+左 Shift+Space",
              message: "正在验证新的快捷键。",
              retryable: false,
            },
          }, shortcut)}
          requestPending={false}
          transportError=""
          onStart={vi.fn()}
          onCancel={onCancel}
        />
        <button type="button" onClick={outsideAction}>其他设置</button>
      </>,
    );
    fireEvent.click(screen.getByRole("button", { name: "其他设置" }));
    expect(onCancel).not.toHaveBeenCalled();
    expect(outsideAction).toHaveBeenCalledOnce();
  });

  it("keeps capturing after a rejected candidate and exposes an inline warning", () => {
    render(
      <ShortcutCaptureField
        view={selectShortcutLifecycle({
          ...activeSnapshot,
          sequence: 4,
          operation: {
            operationId: 2,
            kind: "capture",
            phase: "capturing",
            candidateLabel: "C",
            message: "快捷键至少需要一个修饰键。",
            errorCode: "invalid_binding",
            retryable: true,
          },
        }, shortcut)}
        requestPending={false}
        transportError=""
        onStart={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText("C")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toBe("快捷键至少需要一个修饰键。");
  });
});
