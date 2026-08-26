// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ShortcutDialog } from "./ShortcutDialog";
import { selectShortcutLifecycle } from "./shortcutLifecycle";

afterEach(cleanup);

const activeLabel = "左 Ctrl+左 Shift+Space";
const capturingView = selectShortcutLifecycle({
  sequence: 2,
  configRevision: 1,
  runtime: {
    state: "suspended",
    activeLabel,
    activeBinding: null,
    message: "原快捷键已暂停。",
  },
  operation: {
    operationId: 1,
    kind: "capture",
    phase: "capturing",
    message: "等待输入。",
    retryable: false,
  },
}, activeLabel);
const baseProps = {
  open: true,
  view: capturingView,
  requestPending: false,
  transportError: "",
  canRestoreDefault: false,
  onClose: vi.fn(),
  onCapture: vi.fn(),
  onRetry: vi.fn(),
  onRestoreDefault: vi.fn(),
};

describe("ShortcutDialog", () => {
  it("does not claim to be capturing while the backend handshake is starting", () => {
    const startingView = selectShortcutLifecycle({
      sequence: 3,
      configRevision: 1,
      runtime: {
        state: "suspended",
        activeLabel,
        activeBinding: null,
        message: "原快捷键已暂停。",
      },
      operation: {
        operationId: 2,
        kind: "capture",
        phase: "starting",
        message: "正在准备快捷键录制。",
        retryable: false,
      },
    }, activeLabel);
    render(<ShortcutDialog {...baseProps} view={startingView} requestPending />);
    expect(screen.getByText("正在准备换绑")).toBeTruthy();
    expect(screen.getByText("键盘 Hook 就绪后才会开始录入。")).toBeTruthy();
    expect(screen.queryByText("请按下新的快捷键")).toBeNull();
    expect(screen.getByText("正在准备…")).toBeTruthy();
  });

  it("shows one physical capture action without backend choices", () => {
    render(<ShortcutDialog {...baseProps} />);
    expect(screen.getByRole("button", { name: /请按下新的快捷键/ })).toBeTruthy();
    expect(screen.queryByText(/标准模式|独占模式|当前运行/)).toBeNull();
    expect(screen.queryByRole("button", { name: "保存" })).toBeNull();
  });

  it("offers restore default only when there is no newer change to undo", () => {
    const onRestoreDefault = vi.fn();
    render(<ShortcutDialog
      {...baseProps}
      view={selectShortcutLifecycle(null, "右 Ctrl+右 Shift+Space")}
      canRestoreDefault
      onRestoreDefault={onRestoreDefault}
    />);
    fireEvent.click(screen.getByRole("button", { name: "恢复默认" }));
    expect(onRestoreDefault).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "撤销本次更改" })).toBeNull();
  });

  it("offers retry without an extra success action after automatic save", () => {
    const onRetry = vi.fn();
    const savedLabel = "左 Ctrl+右 Alt+Space";
    const savedView = selectShortcutLifecycle({
      sequence: 3,
      configRevision: 2,
      runtime: {
        state: "active",
        activeLabel: savedLabel,
        activeBinding: null,
        message: "物理快捷键已启用。",
      },
      operation: {
        operationId: 1,
        kind: "capture",
        phase: "succeeded",
        candidateLabel: savedLabel,
        message: "快捷键已启用。",
        retryable: false,
        changed: true,
      },
    }, savedLabel);
    render(<ShortcutDialog
      {...baseProps}
      view={savedView}
      onRetry={onRetry}
    />);
    fireEvent.click(screen.getByRole("button", { name: "重新设置" }));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "撤销本次更改" })).toBeNull();
  });

  it("keeps unchanged automatic save silent and does not offer undo", () => {
    const unchangedView = selectShortcutLifecycle({
      sequence: 4,
      configRevision: 2,
      runtime: {
        state: "active",
        activeLabel,
        activeBinding: null,
        message: "物理快捷键已启用。",
      },
      operation: {
        operationId: 2,
        kind: "capture",
        phase: "succeeded",
        candidateLabel: activeLabel,
        message: "快捷键未发生变化。",
        retryable: false,
        changed: false,
      },
    }, activeLabel);
    render(<ShortcutDialog {...baseProps} view={unchangedView} />);
    expect(screen.queryByText("快捷键未发生变化。")).toBeNull();
    expect(screen.queryByRole("button", { name: "撤销本次更改" })).toBeNull();
  });
});
