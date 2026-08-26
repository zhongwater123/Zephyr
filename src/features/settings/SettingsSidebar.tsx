import type { JSX, RefObject } from "preact";
import type { AppConfig, AsrOptionPool, ConfigStatus, ConfigValue, VoiceStatePayload } from "../../domain";
import { ShortcutCaptureField } from "../shortcut/ShortcutCaptureField";
import type { ShortcutBindingViewModel } from "../shortcut/useShortcutBindingController";
import { BehaviorSwitch } from "./BehaviorSwitch";
import { OptionPoolRenderer } from "./OptionPoolRenderer";

function runtimePresentation(
  config: AppConfig,
  service: ConfigStatus,
  status: VoiceStatePayload,
) {
  if (!service.provider_ready || status.state === "Error") {
    return { label: "服务不可用", detail: status.state === "Error" ? status.message : "语音服务暂时无法连接", tone: "danger" };
  }
  if (!config.enabled || status.state === "Disabled") {
    return { label: "已暂停", detail: "开启后即可通过快捷键输入", tone: "paused" };
  }
  if (status.state === "Recording") return { label: "正在聆听", detail: "松开快捷键后开始识别", tone: "active" };
  if (status.state === "Transcribing") return { label: "识别中", detail: "正在把语音转换为文字", tone: "active" };
  if (status.state === "Pasting") return { label: "正在输入", detail: "文字即将出现在当前应用", tone: "active" };
  return { label: "已就绪", detail: "按住快捷键开始说话", tone: "ready" };
}

export function SettingsSidebar({
  open,
  config,
  configStatus,
  voiceStatus,
  shortcutView,

  optionPool,
  optionSaving,
  optionSavingMap,
  optionErrors,
  enabledSaving,
  enabledError,
  menuRef,
  personalizationRef,
  moreSettingsRef,
  onClose,
  onEnabled,
  onShortcutCapture,
  onShortcutCancel,
  onShortcutKeyDown,
  onShortcutKeyUp,
  onOption,
  onLaunch,
}: {
  open: boolean;
  config: AppConfig;
  configStatus: ConfigStatus;
  voiceStatus: VoiceStatePayload;
  shortcutView: ShortcutBindingViewModel;

  optionPool: AsrOptionPool | null;
  optionSaving: boolean;
  optionSavingMap: Record<string, boolean>;
  optionErrors: Record<string, string>;
  enabledSaving: boolean;
  enabledError: string;
  menuRef: RefObject<HTMLButtonElement>;
  personalizationRef: RefObject<HTMLButtonElement>;
  moreSettingsRef: RefObject<HTMLButtonElement>;
  onClose: () => void;
  onEnabled: (enabled: boolean) => void;
  onShortcutCapture: () => void;
  onShortcutCancel: (source?: string) => void;
  onShortcutKeyDown: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onShortcutKeyUp: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onOption: (optionId: string, value: ConfigValue) => void;
  onLaunch: (panel: "personalization" | "more_settings") => void;
}) {
  const runtime = runtimePresentation(config, configStatus, voiceStatus);

  return (
    <aside id="config-drawer" className={"settings-sidebar " + (open ? "is-open" : "")} aria-hidden={!open}>
      <header className="settings-sidebar-header">
        <div>
          <p className="drawer-kicker">Zephyr / 语音输入</p>
          <h1>语音输入</h1>
        </div>
        <button type="button" className="icon-button" aria-label="关闭设置" onClick={onClose}>
          <span aria-hidden="true">×</span>
        </button>
      </header>

      <div className="settings-sidebar-scroll">
        <section className={"runtime-status-card " + runtime.tone} aria-live="polite">
          <div className="runtime-status-heading">
            <span className="runtime-status-indicator" aria-hidden="true" />
            <div>
              <strong>{runtime.label}</strong>
              <p>{runtime.detail}</p>
            </div>
          </div>
          <BehaviorSwitch
            label="语音输入"
            description={config.enabled ? "在任意应用中使用快捷键输入" : "当前不会监听快捷键"}
            checked={config.enabled}
            disabled={enabledSaving}
            onChange={onEnabled}
          />
          {enabledError ? <p className="field-error" role="alert">{enabledError}</p> : null}
        </section>

        <ShortcutCaptureField
          view={shortcutView}
          onStart={onShortcutCapture}
          onCancel={onShortcutCancel}
          onKeyDown={onShortcutKeyDown}
          onKeyUp={onShortcutKeyUp}
        />

        <OptionPoolRenderer
          pool={optionPool}
          saving={optionSaving}
          savingOptions={optionSavingMap}
          errors={optionErrors}
          onChange={onOption}
        />

        <section className="launch-section" aria-labelledby="launch-title">
          <div className="section-heading">
            <h2 id="launch-title">按你的方式使用</h2>
            <p>词库、习惯和服务设置集中管理</p>
          </div>
          <div className="launch-card-grid">
            <button
              ref={personalizationRef}
              type="button"
              className="launch-card"
              onClick={() => onLaunch("personalization")}
            >
              <span className="launch-icon" aria-hidden="true">Aa</span>
              <strong>个性化</strong>
              <small>词库、表达习惯与历史</small>
              <span className="launch-arrow" aria-hidden="true">↗</span>
            </button>
            <button
              ref={moreSettingsRef}
              type="button"
              className="launch-card"
              onClick={() => onLaunch("more_settings")}
            >
              <span className="launch-icon" aria-hidden="true">···</span>
              <strong>更多设置</strong>
              <small>服务、兼容性与隐私</small>
              <span className="launch-arrow" aria-hidden="true">↗</span>
            </button>
          </div>
        </section>
      </div>
    </aside>
  );
}
