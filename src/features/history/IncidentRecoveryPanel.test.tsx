// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IncidentItem } from "../../domain";
import { IncidentRecoveryPanel } from "./IncidentRecoveryPanel";

const mocks = vi.hoisted(() => ({
  list: vi.fn(), health: vi.fn(), audio: vi.fn(), copyText: vi.fn(),
  saveAudio: vi.fn(), saveReport: vi.fn(), remove: vi.fn(), setPinned: vi.fn(),
  saveDialog: vi.fn(),
}));

vi.mock("../../ipc/client", () => ({ incidentApi: {
  list: mocks.list, health: mocks.health, audio: mocks.audio, copyText: mocks.copyText,
  saveAudio: mocks.saveAudio, saveReport: mocks.saveReport, remove: mocks.remove,
  setPinned: mocks.setPinned,
} }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: mocks.saveDialog }));

const incident: IncidentItem = {
  id: "attempt-current", createdAtUtcMs: 1_700_000_000_000,
  terminalOutcome: "failed", failureStage: "asr", failureCode: "asr_timeout",
  failureMessage: "识别超时", recoverability: "text_and_audio",
  partialText: "部分文本", finalText: null, audioAvailable: true,
  audioCompleteness: "complete", pinned: false,
  expiresAtUtcMs: 1_800_000_000_000, targetApp: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.list.mockResolvedValue([incident]);
  mocks.health.mockResolvedValue({
    available: true, degraded: false, controlEventsDropped: 0,
    audioChunksDropped: 0, lastError: null,
  });
  mocks.audio.mockResolvedValue(new Uint8Array([1, 2, 3, 4]));
  mocks.saveDialog.mockResolvedValue("D:\\exports\\incident.zip");
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true, value: vi.fn(() => "blob:incident-audio"),
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true, value: vi.fn(),
  });
});

afterEach(cleanup);

describe("IncidentRecoveryPanel current contracts", () => {
  it("revokes a generated audio Blob URL when the panel unmounts", async () => {
    const view = render(<IncidentRecoveryPanel />);
    await screen.findByText("asr · asr_timeout");
    fireEvent.click(screen.getByRole("button", { name: "播放音频" }));
    await waitFor(() => expect(URL.createObjectURL).toHaveBeenCalledTimes(1));
    view.unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:incident-audio");
  });

  it("exports without sensitive attachments by default", async () => {
    render(<IncidentRecoveryPanel />);
    await screen.findByText("asr · asr_timeout");
    fireEvent.click(screen.getByRole("button", { name: "生成诊断 ZIP" }));
    await waitFor(() => expect(mocks.saveReport).toHaveBeenCalledWith(
      "attempt-current", "D:\\exports\\incident.zip",
      { includeText: false, includeAudio: false, includeLogExcerpt: false },
    ));
  });
});
