import { describe, expect, it } from "vitest";
import type { ShortcutLifecycleSnapshot } from "../../domain";
import {
  initialShortcutLifecycleState,
  selectShortcutLifecycle,
  shortcutLifecycleReducer,
} from "./shortcutLifecycle";

function snapshot(
  sequence: number,
  operation: ShortcutLifecycleSnapshot["operation"],
): ShortcutLifecycleSnapshot {
  return {
    sequence,
    configRevision: 4,
    runtime: {
      state: operation && ["starting", "capturing", "validating", "applying"].includes(operation.phase)
        ? "suspended"
        : "active",
      activeLabel: "左 Ctrl+C",
      activeBinding: null,
      message: "物理快捷键已启用。",
    },
    operation,
  };
}

describe("shortcut lifecycle reducer", () => {
  it("rejects stale sequences and a different operation", () => {
    const current = snapshot(5, {
      operationId: 2,
      kind: "capture",
      phase: "capturing",
      candidateLabel: "左 Ctrl",
      message: "录制中",
      retryable: false,
    });
    const state = { snapshot: current };
    expect(shortcutLifecycleReducer(state, {
      type: "snapshot_received",
      snapshot: snapshot(4, current.operation),
    })).toBe(state);
    expect(shortcutLifecycleReducer(state, {
      type: "snapshot_received",
      snapshot: snapshot(6, {
        operationId: 1,
        kind: "capture",
        phase: "failed",
        message: "迟到失败",
        retryable: true,
      }),
    })).toBe(state);
  });

  it("allows an acknowledged command to start a new operation", () => {
    const terminal = {
      snapshot: snapshot(5, {
        operationId: 2,
        kind: "capture" as const,
        phase: "failed" as const,
        message: "失败",
        retryable: true,
      }),
    };
    const next = snapshot(6, {
      operationId: 3,
      kind: "capture",
      phase: "capturing",
      message: "录制中",
      retryable: false,
    });
    expect(shortcutLifecycleReducer(terminal, {
      type: "snapshot_received",
      snapshot: next,
      allowOperationChange: true,
    }).snapshot).toBe(next);
  });

  it("rolls an application failure back to the authoritative active shortcut", () => {
    const failed = snapshot(8, {
      operationId: 4,
      kind: "capture",
      phase: "failed",
      candidateLabel: "左 Ctrl+V",
      message: "无法保存新的快捷键。",
      errorCode: "persistence_failed",
      retryable: true,
    });
    const view = selectShortcutLifecycle(failed, "fallback");
    expect(view.displayLabel).toBe("左 Ctrl+C");
    expect(view.runtimeState).toBe("active");
    expect(view.message).toBe("无法保存新的快捷键。");
    expect(view.retryable).toBe(true);
  });

  it("keeps a rejected candidate visible and leaves capture active", () => {
    const rejected = snapshot(9, {
      operationId: 5,
      kind: "capture",
      phase: "capturing",
      candidateLabel: "C",
      message: "快捷键至少需要一个修饰键。",
      errorCode: "invalid_binding",
      retryable: true,
    });
    const view = selectShortcutLifecycle(rejected, "fallback");
    expect(view.displayLabel).toBe("C");
    expect(view.capturing).toBe(true);
    expect(view.failed).toBe(false);
    expect(view.message).toBe("快捷键至少需要一个修饰键。");
    expect(view.retryable).toBe(true);
  });

  it("keeps the old label visible while starting and does not claim it works on runtime error", () => {
    const starting = selectShortcutLifecycle(snapshot(9, {
      operationId: 5,
      kind: "capture",
      phase: "starting",
      message: "正在准备快捷键录制。",
      retryable: false,
    }), "fallback");
    expect(starting.displayLabel).toBe("左 Ctrl+C");
    expect(starting.capturing).toBe(false);

    const failed = snapshot(10, {
      operationId: 5,
      kind: "capture",
      phase: "failed",
      message: "键盘 Hook 不可用。",
      errorCode: "hook_unavailable",
      retryable: true,
    });
    failed.runtime = {
      ...failed.runtime,
      state: "error",
      message: "键盘 Hook 不可用。",
    };
    expect(selectShortcutLifecycle(failed, "fallback").message).not.toContain("仍有效");
  });

  it("clears only the terminal operation when the settings session closes", () => {
    const terminal = snapshot(8, {
      operationId: 4,
      kind: "capture",
      phase: "cancelled",
      message: "已取消",
      retryable: false,
    });
    const closed = shortcutLifecycleReducer(
      { snapshot: terminal },
      { type: "session_closed" },
    );
    expect(closed.snapshot?.operation).toBeNull();
    expect(closed.snapshot?.runtime.activeLabel).toBe("左 Ctrl+C");
    expect(initialShortcutLifecycleState.snapshot).toBeNull();
  });

  it("exposes unchanged success without enabling undo", () => {
    const unchanged = snapshot(9, {
      operationId: 5,
      kind: "capture",
      phase: "succeeded",
      candidateLabel: "左 Ctrl+C",
      message: "快捷键未发生变化。",
      retryable: false,
      changed: false,
    });
    const view = selectShortcutLifecycle(unchanged, "fallback");
    expect(view.changed).toBe(false);
    expect(view.canUndo).toBe(false);
    expect(view.message).toBe("");
  });
});
