// @vitest-environment happy-dom

import { createRef } from "preact";
import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultConfig, type AsrOptionPool } from "../../domain";
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

afterEach(cleanup);

describe("SettingsSidebar", () => {
  it("keeps the C-end root hierarchy focused on the primary job", () => {
    const onLaunch = vi.fn();
    render(
      <SettingsSidebar
        open
        config={defaultConfig}
        configStatus={{ provider_ready: true, provider_message: "ok" }}
        voiceStatus={{ state: "Idle", message: "准备就绪" }}
        shortcutStatus={{
          shortcut: "Ctrl+Shift+Space",
          mode: "standard",
          backend: "register_hotkey",
          state: "active",
          message: "标准快捷键已生效。",
        }}
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
        onShortcut={vi.fn()}
        onOption={vi.fn()}
        onLaunch={onLaunch}
      />,
    );

    expect(screen.getByText("已就绪")).toBeTruthy();
    expect(screen.getByText("快捷键")).toBeTruthy();
    expect(screen.getByText("输入效果")).toBeTruthy();
    expect(screen.getByText("自动标点")).toBeTruthy();
    expect(screen.queryByText("供应商名称不应在一级出现")).toBeNull();
    expect(screen.queryByText(/待处理结果/)).toBeNull();
    expect(screen.queryByText(/所有修改均已自动保存/)).toBeNull();
    expect(screen.queryByRole("button", { name: "保存" })).toBeNull();
    expect(screen.queryByRole("button", { name: /冒烟测试/ })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /个性化/ }));
    expect(onLaunch).toHaveBeenCalledWith("personalization");
    fireEvent.click(screen.getByRole("button", { name: /更多设置/ }));
    expect(onLaunch).toHaveBeenCalledWith("more_settings");
  });

  it("shows service unavailable ahead of transient runtime state", () => {
    render(
      <SettingsSidebar
        open
        config={defaultConfig}
        configStatus={{ provider_ready: false, provider_message: "offline" }}
        voiceStatus={{ state: "Recording", message: "正在听" }}
        shortcutStatus={null}
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
        onShortcut={vi.fn()}
        onOption={vi.fn()}
        onLaunch={vi.fn()}
      />,
    );
    expect(screen.getByText("服务不可用")).toBeTruthy();
  });
});
