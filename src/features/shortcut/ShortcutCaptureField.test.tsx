// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ShortcutCaptureField } from "./ShortcutCaptureField";
import type { ShortcutBindingViewModel } from "./useShortcutBindingController";

afterEach(cleanup);

const activeLabel = "左 Ctrl+左 Alt+Space";

function view(
  phase: ShortcutBindingViewModel["phase"],
  displayLabel = activeLabel,
  message = "",
): ShortcutBindingViewModel {
  return {
    phase,
    activeLabel,
    displayLabel,
    message,
    isCapturing: phase === "capturing" || phase === "warning",
    committing: phase === "committing",
  };
}

type StartHandler = () => void;
type CancelHandler = (source?: string) => void;

function renderField(
  shortcutView: ShortcutBindingViewModel,
  handlers: {
    onStart?: StartHandler;
    onCancel?: CancelHandler;
  } = {},
) {
  const onStart = handlers.onStart ?? vi.fn<StartHandler>();
  const onCancel = handlers.onCancel ?? vi.fn<CancelHandler>();
  render(
    <ShortcutCaptureField
      view={shortcutView}
      onStart={onStart}
      onCancel={onCancel}
      onKeyDown={vi.fn()}
      onKeyUp={vi.fn()}
    />,
  );
  return { onStart, onCancel };
}

describe("ShortcutCaptureField", () => {
  it("uses the visible shortcut value as the single edit entry", () => {
    const { onStart } = renderField(view("idle"));
    expect(screen.getByText("左 Ctrl")).toBeTruthy();
    expect(screen.getByText("左 Alt")).toBeTruthy();
    expect(screen.getByText("Space")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    expect(onStart).toHaveBeenCalledOnce();
  });

  it("shows capture styling immediately and cancels on a second pointer press", () => {
    const { onStart, onCancel } = renderField(view("capturing", ""));
    const field = screen.getByRole("button", { name: /正在录入快捷键/ });

    expect(screen.getByText("正在录入")).toBeTruthy();
    expect(screen.getByText(/主键按下后自动保存/)).toBeTruthy();
    expect(screen.queryByText("Space")).toBeNull();

    fireEvent.pointerDown(field);
    fireEvent.click(field);
    expect(onStart).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("cancels when the user points anywhere outside the field", () => {
    const { onCancel } = renderField(view("capturing", "左 Ctrl"));
    fireEvent.pointerDown(document.body);
    expect(onCancel).toHaveBeenCalledWith("focus_lost");
  });

  it("renders each modifier supplied by the DOM capture controller", () => {
    renderField(view("capturing", "左 Ctrl+右 Shift"));
    expect(screen.getByText("左 Ctrl")).toBeTruthy();
    expect(screen.getByText("右 Shift")).toBeTruthy();
  });

  it("locks the field while the optimistic value is being committed", () => {
    const { onStart, onCancel } = renderField(view("committing", "左 Ctrl+右 Shift+K"));
    const field = screen.getByRole("button", { name: /正在应用快捷键/ });
    expect(field).toHaveProperty("disabled", true);
    fireEvent.pointerDown(field);
    expect(onStart).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("keeps an invalid candidate visible with an inline warning", () => {
    renderField(view("warning", "左 Ctrl", "单独使用修饰键时，仅支持右 Ctrl、右 Alt 或右 Shift。"));
    expect(screen.getByText("左 Ctrl")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("仅支持右 Ctrl");
  });
});
