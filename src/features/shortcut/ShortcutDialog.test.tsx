// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ShortcutPreview } from "../../domain";
import { ShortcutDialog } from "./ShortcutDialog";

afterEach(cleanup);

const baseProps = {
  open: true,
  currentShortcut: "Ctrl+Alt+A",
  draft: "",
  preview: null,
  checking: false,
  notice: "请按下要使用的快捷键。",
  onClose: vi.fn(),
  onCapture: vi.fn(),
  onExclusive: vi.fn(),
  onRetry: vi.fn(),
};

function occupiedPreview(): ShortcutPreview {
  return {
    previewId: 7,
    shortcut: "Ctrl+Alt+Space",
    normalized: "Ctrl+Alt+Space",
    mode: "standard",
    state: "occupied",
    reason: "该快捷键已被其他应用占用。",
  };
}

describe("ShortcutDialog", () => {
  it("starts with one shortcut capture action and no backend choice", () => {
    render(<ShortcutDialog {...baseProps} />);

    expect(screen.getByRole("button", { name: /按下新的快捷键/ })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /标准模式/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /独占模式/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "保存" })).toBeNull();
    expect(screen.queryByText("当前运行")).toBeNull();
  });

  it("offers exclusive takeover only after standard registration is occupied", () => {
    const onExclusive = vi.fn();
    const onRetry = vi.fn();
    render(
      <ShortcutDialog
        {...baseProps}
        draft="Ctrl+Alt+Space"
        preview={occupiedPreview()}
        onExclusive={onExclusive}
        onRetry={onRetry}
      />,
    );

    expect(screen.getByText("这个快捷键已被其他应用占用")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "使用独占模式" }));
    expect(onExclusive).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "换一个快捷键" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
