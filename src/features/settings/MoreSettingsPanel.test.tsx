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
});
