// @vitest-environment happy-dom

import { createRef } from "preact";
import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  defaultConfig,
  type AsrOptionPool,
  type ShortcutTriggerMode,
  type VoiceState,
} from "../../domain";
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
type PolishLevelHandler = (level: 1 | 2 | 3) => void;
type TriggerModeHandler = (mode: ShortcutTriggerMode) => void;

function renderSidebar(overrides: {
  providerReady?: boolean;
  voiceState?: VoiceState;
  triggerMode?: ShortcutTriggerMode;
  optionPool?: AsrOptionPool | null;
  onLaunch?: LaunchHandler;
  onShortcutCapture?: ShortcutCaptureHandler;
  onPolishLevel?: PolishLevelHandler;
  onTriggerMode?: TriggerModeHandler;
} = {}) {
  const onLaunch = overrides.onLaunch ?? vi.fn<LaunchHandler>();
  const onShortcutCapture = overrides.onShortcutCapture ?? vi.fn<ShortcutCaptureHandler>();
  const onPolishLevel = overrides.onPolishLevel ?? vi.fn<PolishLevelHandler>();
  const onTriggerMode = overrides.onTriggerMode ?? vi.fn<TriggerModeHandler>();
  render(
    <SettingsSidebar
      open
      config={{ ...defaultConfig, shortcut_trigger_mode: overrides.triggerMode ?? "hold" }}
      configStatus={{
        provider_ready: overrides.providerReady ?? true,
        provider_message: overrides.providerReady === false ? "offline" : "ok",
      }}
      voiceStatus={{
        state: overrides.voiceState ?? "Idle",
        message: overrides.voiceState === "Recording" ? "正在听" : "准备就绪",
      }}
      shortcutView={idleShortcutView}
      optionPool={overrides.optionPool === undefined ? pool : overrides.optionPool}
      optionSaving={false}
      optionSavingMap={{}}
      optionErrors={{}}
      polishSaving={false}
      polishError=""
      triggerModeSaving={false}
      triggerModeError=""
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
      onPolishLevel={onPolishLevel}
      onTriggerMode={onTriggerMode}
      onLaunch={onLaunch}
    />,
  );
  return { onLaunch, onShortcutCapture, onPolishLevel, onTriggerMode };
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

  it("offers accessible mutually exclusive trigger modes", () => {
    const onTriggerMode = vi.fn<TriggerModeHandler>();
    renderSidebar({ onTriggerMode });

    expect(screen.getByRole("radio", { name: /按住说话/ })).toHaveProperty("checked", true);
    fireEvent.click(screen.getByRole("radio", { name: /按一下开始/ }));
    expect(onTriggerMode).toHaveBeenCalledWith("toggle");
  });

  it("locks trigger mode and shortcut editing throughout an active voice session", () => {
    const onShortcutCapture = vi.fn<ShortcutCaptureHandler>();
    renderSidebar({ voiceState: "Starting", onShortcutCapture });

    expect(screen.getByText("正在启动")).toBeTruthy();
    expect(screen.getByRole("radio", { name: /按住说话/ })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: /本次语音结束后/ })).toHaveProperty("disabled", true);
    expect(screen.getByText("本次语音结束后可修改。")).toBeTruthy();
    expect(onShortcutCapture).not.toHaveBeenCalled();
  });

  it("shows toggle-specific ready instructions", () => {
    renderSidebar({ triggerMode: "toggle" });
    expect(screen.getByText("按一下快捷键开始说话")).toBeTruthy();
    expect(screen.getByText(/按一下快捷键开始语音输入/)).toBeTruthy();
  });

  it("puts a plain-language polishing slider at the bottom of input effects", () => {
    const onPolishLevel = vi.fn<PolishLevelHandler>();
    renderSidebar({ onPolishLevel });

    const inputEffects = screen.getByText("输入效果").closest("section");
    const slider = screen.getByRole("slider", { name: "文字整理程度" });

    expect(inputEffects?.contains(slider)).toBe(true);
    expect(inputEffects?.textContent?.indexOf("智能润色")).toBeGreaterThan(
      inputEffects?.textContent?.indexOf("自动标点") ?? -1,
    );
    expect(slider.getAttribute("aria-valuetext")).toBe("自然整理");
    expect(screen.getByText("说完后，希望文字整理到什么程度？")).toBeTruthy();
    expect(screen.getByText(/可以直接发送或使用/)).toBeTruthy();
    expect(screen.queryByText(/Prompt|模型|介入程度/)).toBeNull();

    fireEvent.change(slider, { target: { value: "3" } });
    expect(onPolishLevel).toHaveBeenCalledWith(3);
  });

  it("keeps polishing available while ASR options are still loading", () => {
    renderSidebar({ optionPool: null });

    expect(screen.getByText("正在加载识别选项…")).toBeTruthy();
    expect(screen.getByRole("slider", { name: "文字整理程度" })).toBeTruthy();
  });
});
