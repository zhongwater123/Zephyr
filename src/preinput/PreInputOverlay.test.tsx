// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreInputPayload } from "../domain";
import { PreInputOverlay } from "./PreInputOverlay";
import { usePreInputPayload } from "./usePreInputPayload";

vi.mock("./usePreInputPayload", () => ({
  usePreInputPayload: vi.fn(),
}));

afterEach(cleanup);

const payload: PreInputPayload = {
  sessionId: 1,
  seq: 1,
  text: "已经确认等待识别",
  state: "transcribing",
  confirmedChars: 4,
};

describe("PreInputOverlay", () => {
  it("renders the current payload and follows controller visibility", () => {
    vi.mocked(usePreInputPayload).mockReturnValue({ payload, visible: true });
    const { container, rerender } = render(<PreInputOverlay />);

    const shell = container.querySelector(".preinput-shell");
    expect(shell?.classList.contains("visible")).toBe(true);
    expect(shell?.getAttribute("data-state")).toBe("transcribing");
    expect(screen.getByText("正在识别")).toBeTruthy();
    expect(screen.getByText("已经确认")).toBeTruthy();
    expect(screen.getByText("等待识别")).toBeTruthy();

    vi.mocked(usePreInputPayload).mockReturnValue({ payload, visible: false });
    rerender(<PreInputOverlay />);
    expect(container.querySelector(".preinput-shell")?.classList.contains("visible")).toBe(false);
  });
});
