// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PendingOutput } from "../../domain";
import { PendingOutputsPanel } from "./PendingOutputsPanel";

afterEach(cleanup);

const output: PendingOutput = {
  id: "pending-1",
  sessionId: 7,
  text: "待处理文本",
  executableName: "Code.exe",
  createdAtUnixMs: 1,
  expiresAtUnixMs: 10_000,
  targetAvailable: false,
  reasonCode: "target_changed",
  reasonMessage: "目标窗口已变化",
  deliveryCertainty: "retryable",
};

describe("PendingOutputsPanel", () => {
  it("keeps delivery disabled when the original target is unavailable", () => {
    render(
      <PendingOutputsPanel
        outputs={[output]}
        onDeliver={vi.fn()}
        onCopy={vi.fn()}
        onDiscard={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "发送到原窗口" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByText("目标窗口已变化")).toBeTruthy();
  });

  it("routes explicit copy and discard actions by pending id", () => {
    const onCopy = vi.fn();
    const onDiscard = vi.fn();
    render(
      <PendingOutputsPanel
        outputs={[{ ...output, targetAvailable: true }]}
        onDeliver={vi.fn()}
        onCopy={onCopy}
        onDiscard={onDiscard}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "复制文本" }));
    fireEvent.click(screen.getByRole("button", { name: "丢弃" }));
    expect(onCopy).toHaveBeenCalledWith("pending-1");
    expect(onDiscard).toHaveBeenCalledWith("pending-1");
  });

  it("requires an inline second confirmation for uncertain delivery", () => {
    const onDeliver = vi.fn();
    render(
      <PendingOutputsPanel
        outputs={[{
          ...output,
          targetAvailable: true,
          deliveryCertainty: "mayHaveBeenSubmitted",
        }]}
        onDeliver={onDeliver}
        onCopy={vi.fn()}
        onDiscard={vi.fn()}
      />,
    );

    expect(screen.getByText(/文本可能已经输入/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "发送到原窗口" }));
    expect(onDeliver).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认再次发送" }));
    expect(onDeliver).toHaveBeenCalledWith("pending-1", true);
  });
});
