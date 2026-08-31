// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreInputPayload } from "../domain";
import { PreInputOverlay } from "./PreInputOverlay";
import { usePreInputPayload } from "./usePreInputPayload";

vi.mock("./usePreInputPayload", () => ({
  usePreInputPayload: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const payload: PreInputPayload = {
  sessionId: 1,
  seq: 1,
  text: "已经确认等待识别",
  state: "transcribing",
  confirmedChars: 4,
};

describe("PreInputOverlay", () => {
  it("announces Starting separately from listening", () => {
    vi.mocked(usePreInputPayload).mockReturnValue({
      payload: {
        ...payload,
        text: "",
        state: "starting",
        message: "正在启动麦克风",
      },
      visible: true,
    });
    render(<PreInputOverlay />);

    expect(screen.getByLabelText("正在启动麦克风")).toBeTruthy();
    expect(screen.getByText("正在启动麦克风")).toBeTruthy();
    expect(screen.queryByText("正在聆听")).toBeNull();
  });

  it("renders the current payload and follows controller visibility", () => {
    vi.mocked(usePreInputPayload).mockReturnValue({ payload, visible: true });
    const { container, rerender } = render(<PreInputOverlay />);

    const shell = container.querySelector(".preinput-shell");
    expect(shell?.classList.contains("visible")).toBe(true);
    expect(shell?.getAttribute("data-state")).toBe("transcribing");
    expect(container.querySelector(".preinput-status")).toBeNull();
    expect(screen.queryByText("正在识别")).toBeNull();
    expect(container.querySelectorAll(".preinput-loader__particle")).toHaveLength(54);
    expect(container.querySelectorAll(".preinput-loader__particle--active")).toHaveLength(8);
    expect(screen.getByText("已经确认")).toBeTruthy();
    expect(screen.getByText("等待识别")).toBeTruthy();
    expect(container.querySelector(".preinput-text__reveal")?.textContent).toBe("已经确认");

    vi.mocked(usePreInputPayload).mockReturnValue({ payload, visible: false });
    rerender(<PreInputOverlay />);
    expect(container.querySelector(".preinput-shell")?.classList.contains("visible")).toBe(false);
  });

  it("follows the latest incremental text with linear animation", () => {
    const frames = new Map<number, FrameRequestCallback>();
    let nextFrameId = 1;
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      const frameId = nextFrameId++;
      frames.set(frameId, callback);
      return frameId;
    });
    vi.spyOn(window, "cancelAnimationFrame").mockImplementation((frameId) => {
      frames.delete(frameId);
    });

    const runFrameBatch = (time: number) => {
      const pendingFrames = [...frames.values()];
      expect(pendingFrames.length).toBeGreaterThan(0);
      frames.clear();
      pendingFrames.forEach((callback) => callback(time));
    };

    vi.mocked(usePreInputPayload).mockReturnValue({ payload, visible: true });
    const { container, rerender } = render(<PreInputOverlay />);
    const viewport = container.querySelector<HTMLElement>(".preinput-text__copy");
    expect(viewport).toBeTruthy();
    let scrollHeight = 240;
    Object.defineProperty(viewport, "scrollHeight", {
      configurable: true,
      get: () => scrollHeight,
    });
    Object.defineProperty(viewport, "clientHeight", { configurable: true, value: 32 });
    if (viewport) viewport.scrollTop = 0;

    vi.mocked(usePreInputPayload).mockReturnValue({
      payload: { ...payload, seq: 2, text: `${payload.text}最新增量` },
      visible: true,
    });
    rerender(<PreInputOverlay />);

    expect(viewport?.scrollTop).toBe(0);
    expect(container.querySelector(".preinput-text__reveal")?.textContent).toBe("最新增量");
    runFrameBatch(0);
    const firstPosition = viewport?.scrollTop ?? 0;
    runFrameBatch(16);
    const secondPosition = viewport?.scrollTop ?? 0;

    expect(firstPosition).toBeGreaterThan(0);
    expect(secondPosition).toBeGreaterThan(firstPosition);
    expect(secondPosition - firstPosition).toBeCloseTo(firstPosition, 5);

    scrollHeight = 280;
    vi.mocked(usePreInputPayload).mockReturnValue({
      payload: { ...payload, seq: 3, text: `${payload.text}最新增量继续` },
      visible: true,
    });
    rerender(<PreInputOverlay />);
    runFrameBatch(32);
    const thirdPosition = viewport?.scrollTop ?? 0;
    expect(thirdPosition - secondPosition).toBeCloseTo(firstPosition, 5);

    for (let frame = 3; frame < 100 && frames.size > 0; frame += 1) {
      runFrameBatch(frame * 16);
    }

    expect(viewport?.scrollTop).toBe(248);
    expect(container.querySelector(".preinput-text__flow")?.textContent).toContain(
      "等待识别最新增量继续",
    );
    expect(container.querySelector(".preinput-text__reveal")?.textContent).toBe("继续");
  });
});
