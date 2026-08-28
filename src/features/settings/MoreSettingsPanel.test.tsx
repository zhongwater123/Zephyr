// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultConfig, defaultConfigStatus } from "../../domain";
import { MoreSettingsPanel } from "./MoreSettingsPanel";

afterEach(cleanup);

describe("MoreSettingsPanel organizer settings", () => {
  it("keeps shared service credentials out of employee settings", () => {
    render(
      <MoreSettingsPanel
        section="organizer"
        config={defaultConfig}
        configStatus={defaultConfigStatus}
        providerName="语音识别服务"
        hotwordState={null}
        organizerSaving=""
        organizerError=""
        polishSaving={false}
        polishError=""
        compatibilityExe=""
        compatibilitySaving={false}
        historySaving={false}
        historyError=""
        incidentRecoverySaving={false}
        incidentRecoveryError=""
        diagnosticMessage=""
        providerTestState=""
        organizerTestState=""
        onSection={vi.fn()}
        onProviderTest={vi.fn()}
        onOrganizerTest={vi.fn()}
        onOrganizerEnabled={vi.fn()}
        onOrganizerBaseUrl={vi.fn()}
        onOrganizerModel={vi.fn()}
        onOrganizerBaseUrlCommit={vi.fn()}
        onOrganizerModelCommit={vi.fn()}
        onPolishLevel={vi.fn()}
        onCompatibilityExe={vi.fn()}
        onAddCompatibility={vi.fn()}
        onRemoveCompatibility={vi.fn()}
        onHistoryEnabled={vi.fn()}
        onIncidentRecoveryEnabled={vi.fn()}
        onRevokeEndpoint={vi.fn()}
        onCopyDiagnostics={vi.fn()}
      />,
    );

    expect(screen.queryByLabelText("API Key")).toBeNull();
    expect(screen.queryByPlaceholderText(/密钥/)).toBeNull();
    expect(screen.getByText("服务凭据由内部部署管理")).toBeTruthy();
    expect(screen.getByText("暂不可用")).toBeTruthy();
  });

  it("shows one global three-level polishing control with level two selected", () => {
    render(
      <MoreSettingsPanel
        section="writing"
        config={defaultConfig}
        configStatus={defaultConfigStatus}
        providerName="语音识别服务"
        hotwordState={null}
        organizerSaving=""
        organizerError=""
        polishSaving={false}
        polishError=""
        compatibilityExe=""
        compatibilitySaving={false}
        historySaving={false}
        historyError=""
        incidentRecoverySaving={false}
        incidentRecoveryError=""
        diagnosticMessage=""
        providerTestState=""
        organizerTestState=""
        onSection={vi.fn()}
        onProviderTest={vi.fn()}
        onOrganizerTest={vi.fn()}
        onOrganizerEnabled={vi.fn()}
        onOrganizerBaseUrl={vi.fn()}
        onOrganizerModel={vi.fn()}
        onOrganizerBaseUrlCommit={vi.fn()}
        onOrganizerModelCommit={vi.fn()}
        onPolishLevel={vi.fn()}
        onCompatibilityExe={vi.fn()}
        onAddCompatibility={vi.fn()}
        onRemoveCompatibility={vi.fn()}
        onHistoryEnabled={vi.fn()}
        onIncidentRecoveryEnabled={vi.fn()}
        onRevokeEndpoint={vi.fn()}
        onCopyDiagnostics={vi.fn()}
      />,
    );

    expect(screen.getByRole("radiogroup", { name: "润色强度" })).toBeTruthy();
    expect(screen.getByRole("radio", { name: /一档 · 轻度/ }).getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("radio", { name: /二档 · 标准/ }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("radio", { name: /三档 · 深度/ }).getAttribute("aria-checked")).toBe("false");
    expect(screen.queryByText("成稿画像")).toBeNull();
  });
});
