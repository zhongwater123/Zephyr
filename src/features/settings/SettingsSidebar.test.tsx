// @vitest-environment happy-dom

import { createRef } from "preact";
import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultConfig, type AsrOptionPool } from "../../domain";
import type { ShortcutBindingViewModel } from "../shortcut/useShortcutBindingController";
import { SettingsSidebar } from "./SettingsSidebar";

const pool: AsrOptionPool = {
  providerId: "volcengine",
  providerDisplayName: "供应商名称不应在一级出现",
  schemaVersion: 1,
  revision: 2,
  options: [
    {
      id: "punctuation",
      controlKind: "toggle",
      label: "自动标点",
      description: "自动补全逗号、句号等标点",
      defaultValue: { type: "boolean", value: true },
      group: "recognition_behavior",
      order: 0,
      enabled: true,
    },
  ],
  values: { punctuation: { type: "boolean", value: true } },
};

const idleShortcutView: ShortcutBindingViewModel = {
  phase: "idle",
  activeLabel: defaultConfig.shortcut,
  displayLabel: defaultConfig.shortcut,
  message: "",
  isCapturing: false,
  committing: false,
};

afterEach(cleanup);

type LaunchHandler = (panel: "personalization" | "more_settings") => void;
type ShortcutCaptureHandler = () => void;

function renderSidebar(overrides: {
  providerReady?: boolean;
  voiceState?: "Idle" | "Recording";
  onLaunch?: LaunchHandler;
  onShortcutCapture?: ShortcutCaptureHandler;
} = {}) {
  const onLaunch = overrides.onLaunch ?? vi.fn<LaunchHandler>();
  const onShortcutCapture = overrides.onShortcutCapture ?? vi.fn<ShortcutCaptureHandler>();
  render(
    <SettingsSidebar
      open
      config={defaultConfig}
      configStatus={{
        provider_ready: overrides.providerReady ?? true,
        provider_message: overrides.providerReady === false ? "offline" : "ok",
      }}
      voiceStatus={{
        state: overrides.voiceState ?? "Idle",
        message: overrides.voiceState === "Recording" ? "正在听" : "准备就绪",
      }}
      shortcutView={idleShortcutView}
      optionPool={pool}
      optionSaving={false}
      optionSavingMap={{}}
      optionErrors={{}}
      enabledSaving={false}
      enabledError=""
      menuRef={createRef()}
      personalizationRef={createRef()}
      moreSettingsRef={createRef()}
      onClose={vi.fn()}
      onEnabled={vi.fn()}
      onShortcutCapture={onShortcutCapture}
      onShortcutCancel={vi.fn()}
      onShortcutKeyDown={vi.fn()}
      onShortcutKeyUp={vi.fn()}
      onOption={vi.fn()}
      onLaunch={onLaunch}
    />,
  );
  return { onLaunch, onShortcutCapture };
}

describe("SettingsSidebar", () => {
  it("keeps the C-end root hierarchy focused on the primary job", () => {
    const onLaunch = vi.fn<LaunchHandler>();
    const onShortcutCapture = vi.fn<ShortcutCaptureHandler>();
    renderSidebar({ onLaunch, onShortcutCapture });

    expect(screen.getByText("已就绪")).toBeTruthy();
    expect(screen.getByText("语音输入快捷键")).toBeTruthy();
    expect(screen.getByText("输入效果")).toBeTruthy();
    expect(screen.getByText("自动标点")).toBeTruthy();
    expect(screen.queryByText("供应商名称不应在一级出现")).toBeNull();
    expect(screen.queryByText(/待处理结果/)).toBeNull();
    expect(screen.queryByText(/所有修改均已自动保存/)).toBeNull();
    expect(screen.queryByRole("button", { name: "保存" })).toBeNull();
    expect(screen.queryByRole("button", { name: /冒烟测试/ })).toBeNull();
    expect(onShortcutCapture).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    expect(onShortcutCapture).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: /个性化/ }));
    expect(onLaunch).toHaveBeenCalledWith("personalization");
    fireEvent.click(screen.getByRole("button", { name: /更多设置/ }));
    expect(onLaunch).toHaveBeenCalledWith("more_settings");
  });

  it("shows service unavailable ahead of transient runtime state", () => {
    renderSidebar({ providerReady: false, voiceState: "Recording" });
    expect(screen.getByText("服务不可用")).toBeTruthy();
  });
});
