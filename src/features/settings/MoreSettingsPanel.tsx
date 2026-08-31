import { useEffect, useRef } from "preact/hooks";
import type { AppConfig, ConfigStatus, HotwordState, PendingOutput } from "../../domain";
import { isOfficialEndpoint } from "../../security-model";
import { PendingOutputsPanel } from "../pending/PendingOutputsPanel";
import { BehaviorSwitch } from "./BehaviorSwitch";

export type MoreSettingsSection =
  | "speech"
  | "organizer"
  | "compatibility"
  | "pending"
  | "privacy"
  | "diagnostics";

function DebouncedInput({
  label,
  description,
  value,
  disabled,
  error,
  type = "text",
  placeholder,
  onValue,
  onCommit,
}: {
  label: string;
  description?: string;
  value: string;
  disabled?: boolean;
  error?: string;
  type?: "text" | "password";
  placeholder?: string;
  onValue: (value: string) => void;
  onCommit: () => void;
}) {
  const timer = useRef<number | null>(null);
  const dirty = useRef(false);
  const commitRef = useRef(onCommit);

  function schedule(value: string) {
    dirty.current = true;
    onValue(value);
    if (type === "password") return;
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      dirty.current = false;
      commitRef.current();
    }, 600);
  }

  function commitNow() {
    if (!dirty.current) return;
    dirty.current = false;
    if (timer.current) window.clearTimeout(timer.current);
    commitRef.current();
  }

  useEffect(() => {
    commitRef.current = onCommit;
  }, [onCommit]);

  useEffect(() => () => {
    if (timer.current) window.clearTimeout(timer.current);
  }, []);

  return (
    <label className="setting-field">
      <span>{label}</span>
      {description ? <small>{description}</small> : null}
      <input
        type={type}
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        onInput={(event) => schedule(event.currentTarget.value)}
        onBlur={commitNow}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commitNow();
            event.currentTarget.blur();
          }
        }}
      />
      {error ? <em className="field-error" role="alert">{error}</em> : null}
    </label>
  );
}

export function MoreSettingsPanel({
  section,
  config,
  configStatus,
  providerName,
  hotwordState,
  organizerSaving,
  organizerError,
  compatibilityExe,
  compatibilitySaving,
  historySaving,
  historyError,
  incidentRecoverySaving,
  incidentRecoveryError,
  diagnosticMessage,
  providerTestState,
  organizerTestState,
  pendingOutputs,
  onSection,
  onProviderTest,
  onOrganizerTest,
  onOrganizerEnabled,
  onOrganizerBaseUrl,
  onOrganizerModel,
  onOrganizerBaseUrlCommit,
  onOrganizerModelCommit,
  onCompatibilityExe,
  onAddCompatibility,
  onRemoveCompatibility,
  onHistoryEnabled,
  onIncidentRecoveryEnabled,
  onRevokeEndpoint,
  onCopyDiagnostics,
  onDeliverPending,
  onCopyPending,
  onDiscardPending,
}: {
  section: MoreSettingsSection;
  config: AppConfig;
  configStatus: ConfigStatus;
  providerName: string;
  hotwordState: HotwordState | null;
  organizerSaving: string;
  organizerError: string;
  compatibilityExe: string;
  compatibilitySaving: boolean;
  historySaving: boolean;
  historyError: string;
  incidentRecoverySaving: boolean;
  incidentRecoveryError: string;
  diagnosticMessage: string;
  providerTestState: string;
  organizerTestState: string;
  pendingOutputs: PendingOutput[];
  onSection: (section: MoreSettingsSection) => void;
  onProviderTest: () => void;
  onOrganizerTest: () => void;
  onOrganizerEnabled: (enabled: boolean) => void;
  onOrganizerBaseUrl: (value: string) => void;
  onOrganizerModel: (value: string) => void;
  onOrganizerBaseUrlCommit: () => void;
  onOrganizerModelCommit: () => void;
  onCompatibilityExe: (value: string) => void;
  onAddCompatibility: () => void;
  onRemoveCompatibility: (executableName: string) => void;
  onHistoryEnabled: (enabled: boolean) => void;
  onRevokeEndpoint: (origin: string) => void;
  onIncidentRecoveryEnabled: (enabled: boolean) => void;
  onCopyDiagnostics: () => void;
  onDeliverPending: (id: string, confirmUncertain: boolean) => void;
  onCopyPending: (id: string) => void;
  onDiscardPending: (id: string) => void;
}) {
  const sections: Array<{ id: MoreSettingsSection; label: string; icon: string }> = [
    { id: "speech", label: "语音服务", icon: "声" },
    { id: "organizer", label: "智能整理服务", icon: "智" },
    { id: "compatibility", label: "输入兼容性", icon: "入" },
    { id: "pending", label: "待处理结果", icon: "待" },
    { id: "privacy", label: "隐私与安全", icon: "隐" },
    { id: "diagnostics", label: "故障诊断", icon: "诊" },
  ];

  const recentError =
    configStatus.provider_ready
      ? hotwordState?.last_error || diagnosticMessage || "最近没有发现错误。"
      : configStatus.provider_message;

  return (
    <div className="more-settings-layout">
      <nav className="settings-nav" aria-label="更多设置">
        {sections.map((item) => (
          <button
            type="button"
            key={item.id}
            className={section === item.id ? "is-active" : ""}
            aria-current={section === item.id ? "page" : undefined}
            onClick={() => onSection(item.id)}
          >
            <span aria-hidden="true">{item.icon}</span>
            {item.label}
          </button>
        ))}
      </nav>

      <div className="panel-content settings-detail">
        {section === "speech" ? (
          <section className="panel-page" aria-labelledby="speech-settings-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="speech-settings-title">语音服务</h3>
                <p>语音识别由 Zephyr 的部署环境统一管理。</p>
              </div>
              <span className={"service-pill " + (configStatus.provider_ready ? "online" : "offline")}>
                {configStatus.provider_ready ? "运行正常" : "需要检查"}
              </span>
            </div>
            <div className="service-overview">
              <span className="service-mark" aria-hidden="true">Z</span>
              <div>
                <strong>{providerName || "语音识别服务"}</strong>
                <p>{configStatus.provider_message}</p>
              </div>
            </div>
            <div className="setting-note">
              <strong>由应用部署管理</strong>
              <p>服务地址、识别模型和凭据不会暴露在个人设置中，可避免误操作导致语音输入不可用。</p>
            </div>
            <div className="button-row">
              <button type="button" onClick={onProviderTest} disabled={providerTestState === "testing"}>
                {providerTestState === "testing" ? "正在检测…" : "重新检测"}
              </button>
            </div>
            {providerTestState && providerTestState !== "testing" ? <p className="inline-notice" role="status">{providerTestState}</p> : null}
          </section>
        ) : null}

        {section === "organizer" ? (
          <section className="panel-page" aria-labelledby="organizer-settings-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="organizer-settings-title">智能整理服务</h3>
                <p>用于从本地历史中提取词条，不参与实时语音识别。</p>
              </div>
              <span className={"service-pill " + (hotwordState?.has_hotword_agent_api_key ? "online" : "offline")}>
                {hotwordState?.has_hotword_agent_api_key ? "服务可用" : "暂不可用"}
              </span>
            </div>
            <div className="settings-form">
              <BehaviorSwitch
                label="启用自动整理"
                description="每积累 20 条历史记录后自动提取新词"
                checked={config.hotword_agent_enabled}
                disabled={organizerSaving === "enabled"}
                onChange={onOrganizerEnabled}
              />
              <DebouncedInput
                label="服务地址"
                description="仅支持经过授权的 HTTPS 主机"
                value={config.hotword_agent_base_url}
                disabled={organizerSaving === "base_url"}
                error={organizerSaving === "base_url" ? "" : organizerError}
                onValue={onOrganizerBaseUrl}
                onCommit={onOrganizerBaseUrlCommit}
              />
              <DebouncedInput
                label="模型"
                value={config.hotword_agent_model}
                disabled={organizerSaving === "model"}
                error={organizerSaving === "model" ? "" : organizerError}
                onValue={onOrganizerModel}
                onCommit={onOrganizerModelCommit}
              />
              <div className="setting-note">
                <strong>服务凭据由内部部署管理</strong>
                <p>员工无需输入、测试或轮换服务凭据；服务不可用时请联系内部维护人员。</p>
              </div>
            </div>
            <div className="button-row">
              <button type="button" onClick={onOrganizerTest} disabled={organizerTestState === "testing"}>
                {organizerTestState === "testing" ? "正在测试…" : "测试智能整理服务"}
              </button>
            </div>
            {organizerTestState && organizerTestState !== "testing" ? <p className="inline-notice" role="status">{organizerTestState}</p> : null}
          </section>
        ) : null}

        {section === "compatibility" ? (
          <section className="panel-page" aria-labelledby="compatibility-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="compatibility-title">输入兼容性</h3>
                <p>Zephyr 默认直接把文字输入到当前应用，不会改动剪贴板。</p>
              </div>
            </div>
            <div className="setting-note">
              <strong>自动输入方式</strong>
              <p>优先使用 Windows 原生文字输入。剪贴板兼容模式正在安全升级，暂时不能新增应用。</p>
              <details>
                <summary>技术说明</summary>
                <p>默认方式为 Unicode SendInput；旧兼容配置会安全失败并保留待处理文本，不会触碰剪贴板。</p>
              </details>
            </div>
            <div className="compatibility-add">
              <input
                value={compatibilityExe}
                aria-label="应用可执行文件名"
                placeholder="例如 legacy.exe"
                onInput={(event) => onCompatibilityExe(event.currentTarget.value)}
                onKeyDown={(event) => { if (event.key === "Enter") onAddCompatibility(); }}
              />
              <button type="button" onClick={onAddCompatibility} disabled>安全升级中</button>
            </div>
            <div className="compatibility-list">
              {config.injection_overrides.length ? config.injection_overrides.map((entry) => (
                <div className="simple-list-row" key={entry.executable_name}>
                  <span><strong>{entry.executable_name}</strong><small>剪贴板兼容模式</small></span>
                  <button type="button" className="text-button danger" onClick={() => onRemoveCompatibility(entry.executable_name)}>移除</button>
                </div>
              )) : <p className="empty-state">当前没有需要兼容模式的应用。</p>}
            </div>
          </section>
        ) : null}

        {section === "privacy" ? (
          <section className="panel-page" aria-labelledby="privacy-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="privacy-title">隐私与安全</h3>
                <p>控制哪些内容保留在本地，以及智能整理可连接的主机。</p>
              </div>
            </div>
            <div className="inline-settings">
              <BehaviorSwitch
                label="保存本地历史"
                description="语音输入内容保存在这台设备上，便于搜索与复用"
                checked={config.history_enabled}
                disabled={historySaving}
                onChange={onHistoryEnabled}
              />
              {historyError ? <p className="field-error" role="alert">{historyError}</p> : null}
            </div>
              <BehaviorSwitch
                label="启用异常恢复"
                description="明确授权后，失败会话的音频与转写可在本机短期保留 7 天；与正式历史开关相互独立"
                checked={config.incident_recovery_enabled}
                disabled={incidentRecoverySaving}
                onChange={onIncidentRecoveryEnabled}
              />
              {incidentRecoveryError ? <p className="field-error" role="alert">{incidentRecoveryError}</p> : null}
            <div className="setting-note">
              <strong>智能整理的数据用途</strong>
              <p>仅在启用智能整理时，将待整理的历史文本发送给所配置的服务，用于提取个人词条。实时识别由独立语音服务完成。</p>
            </div>
            <div className="subsection-heading"><h4>已授权的智能整理主机</h4></div>
            <div className="trusted-list">
              {config.trusted_endpoints.filter((entry) => entry.purpose === "hotword_agent").map((entry) => (
                <div className="simple-list-row" key={entry.origin}>
                  <span>
                    <strong>{entry.origin}</strong>
                    <small>{isOfficialEndpoint(entry.origin) ? "官方服务，无需撤销" : "可读取智能整理凭据"}</small>
                  </span>
                  {!isOfficialEndpoint(entry.origin) ? (
                    <button type="button" className="text-button danger" onClick={() => onRevokeEndpoint(entry.origin)}>撤销</button>
                  ) : null}
                </div>
              ))}
            </div>
          </section>
        ) : null}

        {section === "pending" ? (
          <section className="panel-page" aria-labelledby="pending-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="pending-title">待处理结果</h3>
                <p>检查未自动交付或交付状态不确定的文本。</p>
              </div>
            </div>
            <PendingOutputsPanel
              outputs={pendingOutputs}
              onDeliver={onDeliverPending}
              onCopy={onCopyPending}
              onDiscard={onDiscardPending}
            />
          </section>
        ) : null}

        {section === "diagnostics" ? (
          <section className="panel-page" aria-labelledby="diagnostics-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="diagnostics-title">故障诊断</h3>
                <p>分别检查语音服务和智能整理服务，便于定位问题。</p>
              </div>
              <code className="version-label">v0.1.0</code>
            </div>
            <div className="diagnostic-card">
              <span>最近状态</span>
              <strong>{recentError}</strong>
            </div>
            <div className="diagnostic-actions">
              <button type="button" onClick={onProviderTest} disabled={providerTestState === "testing"}>测试语音服务</button>
              <button type="button" className="secondary" onClick={onOrganizerTest} disabled={organizerTestState === "testing"}>测试智能整理服务</button>
              <button type="button" className="secondary" onClick={onCopyDiagnostics}>复制诊断信息</button>
            </div>
            {providerTestState && providerTestState !== "testing" ? <p className="inline-notice">语音服务：{providerTestState}</p> : null}
            {organizerTestState && organizerTestState !== "testing" ? <p className="inline-notice">智能整理：{organizerTestState}</p> : null}
          </section>
        ) : null}
      </div>
    </div>
  );
}
