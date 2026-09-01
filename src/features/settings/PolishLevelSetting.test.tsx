// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PolishLevel } from "../../domain";
import { PolishLevelSetting } from "./PolishLevelSetting";
import { POLISH_HYST, POLISH_THUMB } from "./polishTrackMath";

const TRACK_WIDTH = 340;
/** usable travel, i.e. the span the pointer maps onto 0..1 */
const TRAVEL = TRACK_WIDTH - POLISH_THUMB;

function clientXForRaw(raw: number): number {
  return POLISH_THUMB / 2 + (raw / 3) * TRAVEL;
}

function renderSetting(
  overrides: { value?: PolishLevel; saving?: boolean; error?: string } = {},
) {
  const onChange = vi.fn<(level: PolishLevel) => void>();
  const { container } = render(
    <PolishLevelSetting
      value={overrides.value ?? 2}
      saving={overrides.saving ?? false}
      error={overrides.error ?? ""}
      onChange={onChange}
    />,
  );
  const track = container.querySelector(".polish-track") as HTMLElement;
  return { onChange, track, container };
}

afterEach(cleanup);

describe("PolishLevelSetting semantics", () => {
  it("keeps a real range slider with the four-level contract", () => {
    renderSetting();
    const slider = screen.getByRole("slider", { name: "智能润色输出方式" });
    expect(slider).toHaveProperty("min", "0");
    expect(slider).toHaveProperty("max", "3");
    expect(slider).toHaveProperty("step", "1");
    expect(slider.getAttribute("aria-valuetext")).toBe("自然表达");
  });

  it("announces the committed level, not a continuous value", () => {
    renderSetting({ value: 0 });
    const slider = screen.getByRole("slider", { name: "智能润色输出方式" });
    expect(slider.getAttribute("aria-valuetext")).toBe("极速模式");
    expect(screen.getByText("适合高频短对话")).toBeTruthy();
  });

  it("hides the pixel field and decorations from assistive tech", () => {
    const { container } = renderSetting();
    for (const selector of [".polish-track-bed", ".polish-thumb", ".polish-scale"]) {
      expect(container.querySelector(selector)?.getAttribute("aria-hidden")).toBe("true");
    }
    // the track itself must stay reachable — it hosts the slider
    expect(container.querySelector(".polish-track")?.getAttribute("aria-hidden")).toBeNull();
  });

  it("keeps saving invisible: announced to AT, never shown or blocking", () => {
    const { container } = renderSetting({ saving: true });
    const slider = screen.getByRole("slider", { name: "智能润色输出方式" });
    // announced...
    expect(slider.getAttribute("aria-busy")).toBe("true");
    // ...but the control stays fully usable and visually unchanged
    expect(slider).toHaveProperty("disabled", false);
    expect(container.querySelector(".polish-saving-halo")).toBeNull();
    expect(container.querySelector(".polish-track.is-saving")).toBeNull();
  });

  it("shows a plain-language error and keeps the raw reason in the title", () => {
    renderSetting({ error: "boom" });
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("暂时没保存成功，请再试一次。");
    expect(alert.getAttribute("title")).toBe("boom");
  });

  it("renders no alert when there is no error", () => {
    renderSetting();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("PolishLevelSetting keyboard commits", () => {
  it("commits once for a keyboard change", () => {
    const { onChange } = renderSetting({ value: 2 });
    const slider = screen.getByRole("slider", { name: "智能润色输出方式" });
    fireEvent.change(slider, { target: { value: "0" } });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(0);
  });

  it("does not commit when the value did not actually change", () => {
    const { onChange } = renderSetting({ value: 2 });
    const slider = screen.getByRole("slider", { name: "智能润色输出方式" });
    fireEvent.change(slider, { target: { value: "2" } });
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("PolishLevelSetting without a layout box", () => {
  it("ignores pointer gestures instead of throwing or committing", () => {
    // happy-dom reports a zero-size rect; the component must degrade quietly
    const { onChange, track } = renderSetting();
    expect(() => {
      fireEvent.pointerDown(track, { clientX: 120, pointerId: 1 });
      fireEvent.pointerMove(track, { clientX: 300, pointerId: 1 });
      fireEvent.pointerUp(track, { clientX: 300, pointerId: 1 });
    }).not.toThrow();
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("slider", { name: "智能润色输出方式" })).toBeTruthy();
  });
});

describe("PolishLevelSetting pointer gestures", () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      left: 0,
      width: TRACK_WIDTH,
      top: 0,
      height: 26,
      right: TRACK_WIDTH,
      bottom: 26,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("commits exactly once per drag, no matter how many moves it takes", () => {
    const { onChange, track } = renderSetting({ value: 2 });
    fireEvent.pointerDown(track, { clientX: clientXForRaw(2), pointerId: 1 });
    for (const raw of [2.2, 2.5, 2.7, 2.9, 3]) {
      fireEvent.pointerMove(track, { clientX: clientXForRaw(raw), pointerId: 1 });
    }
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.pointerUp(track, { clientX: clientXForRaw(3), pointerId: 1 });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(3);
  });

  it("does not commit while the pointer is still down", () => {
    const { onChange, track } = renderSetting({ value: 0 });
    fireEvent.pointerDown(track, { clientX: clientXForRaw(0), pointerId: 1 });
    fireEvent.pointerMove(track, { clientX: clientXForRaw(3), pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("does not commit when the gesture ends back on the starting level", () => {
    const { onChange, track } = renderSetting({ value: 2 });
    fireEvent.pointerDown(track, { clientX: clientXForRaw(2), pointerId: 1 });
    fireEvent.pointerMove(track, { clientX: clientXForRaw(2.1), pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: clientXForRaw(2.1), pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("does not commit a cancelled gesture", () => {
    const { onChange, track } = renderSetting({ value: 1 });
    fireEvent.pointerDown(track, { clientX: clientXForRaw(1), pointerId: 1 });
    fireEvent.pointerMove(track, { clientX: clientXForRaw(3), pointerId: 1 });
    fireEvent.pointerCancel(track, { clientX: clientXForRaw(3), pointerId: 1 });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("commits a plain tap on the track", () => {
    const { onChange, track } = renderSetting({ value: 3 });
    fireEvent.pointerDown(track, { clientX: clientXForRaw(0), pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: clientXForRaw(0), pointerId: 1 });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(0);
  });

  it("still accepts a gesture while an earlier save is in flight", () => {
    // Persisting is background work; it must never block the next gesture.
    const { onChange, track } = renderSetting({ value: 2, saving: true });
    fireEvent.pointerDown(track, { clientX: clientXForRaw(0), pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: clientXForRaw(0), pointerId: 1 });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(0);
  });

  it("drives the whole render from one position variable", () => {
    // The variable lives on the HANDLE, not the track: only the handle reads
    // it, and setting it on an ancestor made the whole subtree recompute
    // style on every pointermove.
    const { track, container } = renderSetting({ value: 2 });
    const thumb = container.querySelector(".polish-thumb") as HTMLElement;
    expect(Number(thumb.style.getPropertyValue("--polish-pos"))).toBeCloseTo(2 / 3, 4);
    fireEvent.pointerDown(track, { clientX: clientXForRaw(0), pointerId: 1 });
    fireEvent.pointerUp(track, { clientX: clientXForRaw(0), pointerId: 1 });
    expect(Number(thumb.style.getPropertyValue("--polish-pos"))).toBeCloseTo(0, 4);
  });

  it("previews the level under the pointer, with hysteresis at the boundary", () => {
    const { track } = renderSetting({ value: 1 });
    expect(track.getAttribute("data-tier")).toBe("1");
    fireEvent.pointerDown(track, { clientX: clientXForRaw(1), pointerId: 1 });
    // just short of the boundary plus hysteresis: still level 1
    fireEvent.pointerMove(track, { clientX: clientXForRaw(1.5 + POLISH_HYST - 0.02), pointerId: 1 });
    expect(track.getAttribute("data-tier")).toBe("1");
    // past it: flips to level 2
    fireEvent.pointerMove(track, { clientX: clientXForRaw(1.5 + POLISH_HYST + 0.02), pointerId: 1 });
    expect(track.getAttribute("data-tier")).toBe("2");
  });
});
