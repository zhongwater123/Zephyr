import type { JSX } from "preact";
import { render } from "preact";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ZephyrAsciiField } from "./ZephyrAsciiField";
import "./styles.css";

type ProviderConfig = {
  base_url: string;
  auth_mode: string;
  resource_id: string;
  model: string;
  language: string;
};

type RecognitionBehaviorConfig = {
  enable_itn: boolean;
  enable_punc: boolean;
  enable_ddc: boolean;
  enable_accelerate_text: boolean;
};

type AppConfig = {
  enabled: boolean;
  shortcut: string;
  provider: ProviderConfig;
  recognition_behavior: RecognitionBehaviorConfig;
  history_enabled: boolean;
  hotwords_enabled: boolean;
  hotword_agent_enabled: boolean;
  hotword_agent_base_url: string;
  hotword_agent_model: string;
};

type ConfigStatus = {
  has_api_key: boolean;
  has_app_key: boolean;
  has_access_key: boolean;
  provider_ready: boolean;
  provider_message: string;
};

type ShortcutValidation = {
  shortcut: string;
  normalized: string;
  valid: boolean;
  available: boolean;
  reason?: string | null;
};

type VoiceStatePayload = {
  state: string;
  message: string;
  elapsed_ms?: number;
};

type PreInputPayload = {
  sessionId: number;
  seq: number;
  text: string;
  state: "recording" | "transcribing" | "finalizing" | "dismissing" | "error";
  confirmedChars?: number;
  message?: string;
};

type HistoryItem = {
  id: string;
  text: string;
  created_at: string;
  app_name?: string | null;
  app_title?: string | null;
  char_count: number;
};

type AppHotwordContext = {
  app_name: string;
  context: string;
};

type HotwordState = {
  hotwords_enabled: boolean;
  hotword_agent_enabled: boolean;
  hotword_agent_base_url: string;
  hotword_agent_model: string;
  has_hotword_agent_api_key: boolean;
  manual_hotwords: string[];
  agent_hotwords: string[];
  profile_context: string;
  app_contexts: AppHotwordContext[];
  pending_count: number;
  updated_at?: string | null;
  last_error?: string | null;
};

const maxOverlayCharacters = 180;

const defaultConfig: AppConfig = {
  enabled: true,
  shortcut: "Ctrl+Alt+Space",
  history_enabled: true,
  hotwords_enabled: true,
  hotword_agent_enabled: false,
  hotword_agent_base_url: "https://api.deepseek.com",
  hotword_agent_model: "deepseek-v4-flash",
  recognition_behavior: {
    enable_itn: true,
    enable_punc: true,
    enable_ddc: false,
    enable_accelerate_text: true,
  },
  provider: {
    base_url: "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async",
    auth_mode: "app_access",
    resource_id: "volc.bigasr.sauc.duration",
    model: "bigmodel",
    language: "zh-CN",
  },
};

const defaultConfigStatus: ConfigStatus = {
  has_api_key: false,
  has_app_key: false,
  has_access_key: false,
  provider_ready: false,
  provider_message: "配置尚未检查。",
};

const currentWindow = getCurrentWindow();

function App() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [configStatus, setConfigStatus] = useState<ConfigStatus>(defaultConfigStatus);
  const [apiKey, setApiKey] = useState("");
  const [appKey, setAppKey] = useState("");
  const [accessKey, setAccessKey] = useState("");
  const [status, setStatus] = useState<VoiceStatePayload>({
    state: "Idle",
    message: "就绪",
  });
  const [notice, setNotice] = useState("");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [serviceExpanded, setServiceExpanded] = useState(false);
  const [hotwordExpanded, setHotwordExpanded] = useState(false);
  const [hotwordApiExpanded, setHotwordApiExpanded] = useState(false);
  const [smokeStatus, setSmokeStatus] = useState<"idle" | "running" | "passed" | "failed">("idle");
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyQuery, setHistoryQuery] = useState("");
  const [historyItems, setHistoryItems] = useState<HistoryItem[]>([]);
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null);
  const [editingHistoryText, setEditingHistoryText] = useState("");
  const [historyNotice, setHistoryNotice] = useState("");
  const [historyLoading, setHistoryLoading] = useState(false);
  const [shortcutOpen, setShortcutOpen] = useState(false);
  const [shortcutDraft, setShortcutDraft] = useState("");
  const [shortcutValidation, setShortcutValidation] = useState<ShortcutValidation | null>(null);
  const [shortcutNotice, setShortcutNotice] = useState("");
  const [shortcutChecking, setShortcutChecking] = useState(false);
  const shortcutDialogRef = useRef<HTMLDivElement>(null);
  const [hotwordOpen, setHotwordOpen] = useState(false);
  const [hotwordState, setHotwordState] = useState<HotwordState | null>(null);
  const [hotwordApiKey, setHotwordApiKey] = useState("");
  const [newHotwordText, setNewHotwordText] = useState("");
  const [hotwordEdits, setHotwordEdits] = useState<Record<string, string>>({});
  const [profileContextText, setProfileContextText] = useState("");
  const [appContextName, setAppContextName] = useState("");
  const [appContextText, setAppContextText] = useState("");
  const [appContextEdits, setAppContextEdits] = useState<Record<string, AppHotwordContext>>({});
  const [hotwordNotice, setHotwordNotice] = useState("");
  const [hotwordLoading, setHotwordLoading] = useState(false);

  useEffect(() => {
    invoke<AppConfig>("get_config")
      .then((nextConfig) => {
        setConfig({
          ...nextConfig,
          history_enabled: nextConfig.history_enabled ?? true,
          hotwords_enabled: nextConfig.hotwords_enabled ?? true,
          hotword_agent_enabled: nextConfig.hotword_agent_enabled ?? false,
          hotword_agent_base_url: nextConfig.hotword_agent_base_url || "https://api.deepseek.com",
          hotword_agent_model: nextConfig.hotword_agent_model || "deepseek-v4-flash",
          recognition_behavior: {
            ...defaultConfig.recognition_behavior,
            ...nextConfig.recognition_behavior,
          },
          provider: {
            auth_mode: "app_access",
            ...nextConfig.provider,
          },
        });
      })
      .catch((error) => setNotice(String(error)));
    refreshConfigStatus();
    refreshHotwordState();

    const unlisten = listen<VoiceStatePayload>("voice_state_changed", (event) => {
      setStatus(event.payload);
    });

    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    if (!drawerOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDrawerOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [drawerOpen]);

  useEffect(() => {
    if (!shortcutOpen) return;
    window.setTimeout(() => shortcutDialogRef.current?.focus(), 0);
  }, [shortcutOpen]);

  const statusTone = useMemo(() => {
    if (status.state === "Error") return "danger";
    if (status.state === "Recording" || status.state === "Transcribing") return "active";
    return "idle";
  }, [status.state]);
  const hotwordAgentOnline = Boolean(
    hotwordState?.has_hotword_agent_api_key && !hotwordState?.last_error
  );
  const isHotwordReady = !config.hotword_agent_enabled || hotwordAgentOnline;
  const isRuntimeReady = config.enabled && configStatus.provider_ready && isHotwordReady;
  const topStatusMessage =
    status.state === "Idle"
      ? isRuntimeReady
        ? "已就绪"
        : config.enabled
          ? "待配置"
          : "已暂停"
      : status.message;

  async function refreshConfigStatus() {
    try {
      const nextStatus = await invoke<ConfigStatus>("get_config_status");
      setConfigStatus(nextStatus);
    } catch (error) {
      setConfigStatus({
        has_api_key: false,
        has_app_key: false,
        has_access_key: false,
        provider_ready: false,
        provider_message: String(error),
      });
    }
  }

  function applyHotwordState(nextState: HotwordState) {
    setHotwordState(nextState);
    setHotwordEdits(Object.fromEntries(mergeHotwords(nextState).map((word) => [word, word])));
    setProfileContextText(nextState.profile_context);
    setAppContextEdits(
      Object.fromEntries(nextState.app_contexts.map((item) => [item.app_name, { ...item }]))
    );
    setConfig((current) => ({
      ...current,
      hotwords_enabled: nextState.hotwords_enabled,
      hotword_agent_enabled: nextState.hotword_agent_enabled,
      hotword_agent_base_url: nextState.hotword_agent_base_url,
      hotword_agent_model: nextState.hotword_agent_model,
    }));
  }

  async function refreshHotwordState() {
    try {
      const nextState = await invoke<HotwordState>("get_hotword_state");
      applyHotwordState(nextState);
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function saveConfig() {
    setNotice("");
    try {
      const saved = await invoke<AppConfig>("save_config", {
        config,
        apiKey: apiKey || null,
        appKey: appKey || null,
        accessKey: accessKey || null,
        hotwordAgentApiKey: hotwordApiKey || null,
      });
      setConfig(saved);
      setApiKey("");
      setAppKey("");
      setAccessKey("");
      setHotwordApiKey("");
      await refreshConfigStatus();
      await refreshHotwordState();
      setNotice("配置已保存。");
    } catch (error) {
      setNotice(String(error));
    }
  }

  async function persistHotwordSettings() {
    const nextState = await invoke<HotwordState>("save_hotword_settings", {
      settings: {
        hotwords_enabled: config.hotwords_enabled,
        hotword_agent_enabled: config.hotword_agent_enabled,
        hotword_agent_base_url: config.hotword_agent_base_url,
        hotword_agent_model: config.hotword_agent_model,
      },
      apiKey: hotwordApiKey || null,
    });
    setHotwordApiKey("");
    applyHotwordState(nextState);
    return nextState;
  }

  async function openHotwordPanel() {
    setHotwordOpen(true);
    await refreshHotwordState();
  }

  async function addHotword() {
    setHotwordNotice("");
    const word = newHotwordText.trim();
    if (!word) {
      setHotwordNotice("请输入热词。");
      return;
    }
    try {
      const nextState = await invoke<HotwordState>("add_hotword", { word });
      setNewHotwordText("");
      applyHotwordState(nextState);
      setHotwordNotice("热词已添加。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function updateHotword(word: string) {
    setHotwordNotice("");
    const newWord = (hotwordEdits[word] ?? word).trim();
    if (!newWord) {
      setHotwordNotice("热词不能为空。");
      return;
    }
    try {
      const nextState = await invoke<HotwordState>("update_hotword", {
        oldWord: word,
        newWord,
      });
      applyHotwordState(nextState);
      setHotwordNotice("热词已更新。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function deleteHotword(word: string) {
    setHotwordNotice("");
    try {
      const nextState = await invoke<HotwordState>("delete_hotword", { word });
      applyHotwordState(nextState);
      setHotwordNotice("热词已删除。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function organizeHotwordsNow() {
    setHotwordNotice("");
    setHotwordLoading(true);
    const pendingBefore = hotwordState?.pending_count ?? 0;
    try {
      await persistHotwordSettings();
      const nextState = await invoke<HotwordState>("organize_hotwords_now");
      applyHotwordState(nextState);
      setHotwordNotice(
        pendingBefore > 0 ? "已完成一轮热词整理。" : "没有待整理的历史记录；当前热词库已是最新。"
      );
    } catch (error) {
      setHotwordNotice(String(error));
      await refreshHotwordState();
    } finally {
      setHotwordLoading(false);
    }
  }

  async function saveProfileContext() {
    setHotwordNotice("");
    try {
      const nextState = await invoke<HotwordState>("update_profile_context", {
        text: profileContextText,
      });
      applyHotwordState(nextState);
      setHotwordNotice("个人上下文已保存。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function saveAppContext() {
    setHotwordNotice("");
    try {
      const nextState = await invoke<HotwordState>("update_app_context", {
        appName: appContextName,
        context: appContextText,
      });
      applyHotwordState(nextState);
      setAppContextName("");
      setAppContextText("");
      setHotwordNotice("应用上下文已保存。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function deleteAppContext(appName: string) {
    setHotwordNotice("");
    try {
      const nextState = await invoke<HotwordState>("delete_app_context", { appName });
      applyHotwordState(nextState);
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  function updateAppContextDraft(appName: string, context: string) {
    setAppContextEdits((current) => ({
      ...current,
      [appName]: {
        app_name: appName,
        context,
      },
    }));
  }

  async function saveExistingAppContext(appName: string) {
    setHotwordNotice("");
    const context = appContextEdits[appName]?.context ?? "";
    try {
      const nextState = await invoke<HotwordState>("update_app_context", {
        appName,
        context,
      });
      applyHotwordState(nextState);
      setHotwordNotice("应用上下文已保存。");
    } catch (error) {
      setHotwordNotice(String(error));
    }
  }

  async function setEnabled(enabled: boolean) {
    setNotice("");
    try {
      await invoke("set_enabled", { enabled });
      setConfig((current) => ({ ...current, enabled }));
    } catch (error) {
      setNotice(String(error));
    }
  }

  function openShortcutPanel() {
    setShortcutOpen(true);
    setShortcutDraft(config.shortcut);
    setShortcutValidation(null);
    setShortcutNotice("请按下至少两个按键，例如 Ctrl+Space。");
  }

  async function validateShortcutCandidate(candidate: string) {
    setShortcutDraft(candidate);
    setShortcutNotice("");
    setShortcutChecking(true);
    try {
      const validation = await invoke<ShortcutValidation>("validate_shortcut", { shortcut: candidate });
      setShortcutValidation(validation);
      setShortcutNotice(validation.reason || (validation.available ? "快捷键可用。" : ""));
    } catch (error) {
      setShortcutValidation({
        shortcut: candidate,
        normalized: "",
        valid: false,
        available: false,
        reason: String(error),
      });
      setShortcutNotice(String(error));
    } finally {
      setShortcutChecking(false);
    }
  }

  async function saveShortcut() {
    const candidate = shortcutValidation?.normalized || shortcutDraft;
    if (!candidate || !shortcutValidation?.available) return;
    setShortcutNotice("");
    try {
      const saved = await invoke<AppConfig>("save_shortcut", { shortcut: candidate });
      setConfig(saved);
      setShortcutOpen(false);
      setNotice("快捷键已更新。");
    } catch (error) {
      setShortcutNotice(String(error));
    }
  }

  async function resetShortcut() {
    setShortcutNotice("");
    try {
      const saved = await invoke<AppConfig>("reset_shortcut");
      setConfig(saved);
      setShortcutDraft(saved.shortcut);
      setShortcutValidation({
        shortcut: saved.shortcut,
        normalized: saved.shortcut,
        valid: true,
        available: true,
        reason: "已恢复默认快捷键。",
      });
      setShortcutNotice("已恢复默认快捷键。");
    } catch (error) {
      setShortcutNotice(String(error));
    }
  }

  function clearShortcutDraft() {
    setShortcutDraft("");
    setShortcutValidation(null);
    setShortcutNotice("请按下新的组合键。");
  }

  function captureShortcut(event: JSX.TargetedKeyboardEvent<HTMLDivElement>) {
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      setShortcutOpen(false);
      return;
    }
    if (event.key === "Backspace") {
      clearShortcutDraft();
      return;
    }

    const candidate = shortcutFromKeyboardEvent(event);
    if (!candidate) {
      return;
    }
    void validateShortcutCandidate(candidate);
  }

  async function runSmokeTest() {
    setNotice("");
    setSmokeStatus("running");
    try {
      const saved = await invoke<AppConfig>("save_config", {
        config,
        apiKey: apiKey || null,
        appKey: appKey || null,
        accessKey: accessKey || null,
        hotwordAgentApiKey: hotwordApiKey || null,
      });
      setConfig({
        ...saved,
        recognition_behavior: {
          ...defaultConfig.recognition_behavior,
          ...saved.recognition_behavior,
        },
      });
      setApiKey("");
      setAppKey("");
      setAccessKey("");
      setHotwordApiKey("");
      const asrText = await invoke<string>("test_provider");
      const hotwordText = await invoke<string>("test_hotword_agent");
      await refreshConfigStatus();
      await refreshHotwordState();
      setSmokeStatus("passed");
      setNotice(`冒烟测试通过：${asrText}；${hotwordText}`);
    } catch (error) {
      setSmokeStatus("failed");
      setNotice(String(error));
    }
  }

  function updateProvider(next: Partial<ProviderConfig>) {
    setConfig((current) => ({
      ...current,
      provider: { ...current.provider, ...next },
    }));
  }

  async function saveRecognitionBehavior(next: Partial<RecognitionBehaviorConfig>) {
    setNotice("");
    const previousConfig = config;
    const nextConfig = {
      ...config,
      recognition_behavior: {
        ...config.recognition_behavior,
        ...next,
      },
    };
    setConfig(nextConfig);
    try {
      const saved = await invoke<AppConfig>("save_config", {
        config: nextConfig,
        apiKey: null,
        appKey: null,
        accessKey: null,
        hotwordAgentApiKey: null,
      });
      setConfig({
        ...saved,
        recognition_behavior: {
          ...defaultConfig.recognition_behavior,
          ...saved.recognition_behavior,
        },
      });
      setNotice("识别行为已保存。");
    } catch (error) {
      setConfig(previousConfig);
      setNotice(String(error));
    }
  }

  async function openHistoryPanel() {
    setHistoryOpen(true);
    setHistoryQuery("");
    await loadHistory("");
  }

  async function loadHistory(query = historyQuery) {
    setHistoryLoading(true);
    setHistoryNotice("");
    try {
      const items = await invoke<HistoryItem[]>("list_history", {
        query: query || null,
        limit: 50,
        offset: 0,
      });
      setHistoryItems(items);
      if (items.length === 0) {
        setSelectedHistoryId(null);
        setEditingHistoryText("");
      } else if (!items.some((item) => item.id === selectedHistoryId)) {
        selectHistoryItem(items[0]);
      }
    } catch (error) {
      setHistoryNotice(String(error));
    } finally {
      setHistoryLoading(false);
    }
  }

  function selectHistoryItem(item: HistoryItem) {
    setSelectedHistoryId(item.id);
    setEditingHistoryText(item.text);
    setHistoryNotice("");
  }

  async function saveHistoryItem() {
    if (!selectedHistoryId) return;
    setHistoryNotice("");
    try {
      await invoke("update_history", { id: selectedHistoryId, text: editingHistoryText });
      setHistoryNotice("历史记录已更新。");
      await loadHistory();
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  async function copyHistoryItem() {
    if (!selectedHistoryId) return;
    setHistoryNotice("");
    try {
      await invoke("copy_history_text", { id: selectedHistoryId });
      setHistoryNotice("已复制到剪贴板。");
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  async function deleteHistoryItem() {
    if (!selectedHistoryId) return;
    setHistoryNotice("");
    try {
      await invoke("delete_history", { id: selectedHistoryId });
      setHistoryNotice("历史记录已删除。");
      setSelectedHistoryId(null);
      setEditingHistoryText("");
      await loadHistory();
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  async function clearAllHistory() {
    if (!window.confirm("确认清空全部历史记录？此操作不可撤销。")) return;
    setHistoryNotice("");
    try {
      await invoke("clear_history");
      setHistoryItems([]);
      setSelectedHistoryId(null);
      setEditingHistoryText("");
      setHistoryNotice("历史记录已清空。");
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  function startWindowDrag(event: JSX.TargetedPointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    void currentWindow.startDragging();
  }

  function toggleWindowMaximize(event: JSX.TargetedMouseEvent<HTMLDivElement>) {
    if (event.detail === 2) void currentWindow.toggleMaximize();
  }

  return (
    <main className={`zephyr-app ${drawerOpen ? "drawer-open" : ""}`}>
      <div className="window-drag-strip" onPointerDown={startWindowDrag} onClick={toggleWindowMaximize} />
      <div className="window-controls" aria-label="窗口控制">
        <button type="button" aria-label="最小化" onClick={() => void currentWindow.minimize()}>
          <span className="window-icon minimize" aria-hidden="true" />
        </button>
        <button type="button" aria-label="最大化或还原" onClick={() => void currentWindow.toggleMaximize()}>
          <span className="window-icon maximize" aria-hidden="true" />
        </button>
        <button type="button" aria-label="关闭" onClick={() => void currentWindow.close()}>
          <span className="window-icon close" aria-hidden="true" />
        </button>
      </div>
      <section className="zephyr-stage" onClick={() => drawerOpen && setDrawerOpen(false)}>
        <ZephyrAsciiField state={status.state} muted={drawerOpen} shortcut={config.shortcut} />
        <button
          type="button"
          className="config-toggle"
          aria-label="打开设置"
          aria-expanded={drawerOpen}
          aria-controls="config-drawer"
          onClick={(event) => {
            event.stopPropagation();
            setDrawerOpen((open) => !open);
          }}
        >
          <svg
            className="menu-icon"
            viewBox="0 0 24 24"
            aria-hidden="true"
            focusable="false"
          >
            <path d="M5 7h14M5 12h14M5 17h14" />
          </svg>
        </button>
      </section>

      <aside
        id="config-drawer"
        className="config-drawer"
        aria-hidden={!drawerOpen}
        onClick={(event) => event.stopPropagation()}
      >
        <header className="drawer-header">
          <div>
            <p className="drawer-kicker">Zephyr / 设置</p>
            <h1>云端语音输入</h1>
          </div>
          <button type="button" className="drawer-close" onClick={() => setDrawerOpen(false)}>
            关闭
          </button>
        </header>

        <section className="console-block" aria-live="polite">
          <div className={`status-row ${statusTone}`}>
            <span>状态</span>
            <strong>{topStatusMessage}</strong>
          </div>
          <div className="status-row">
            <span>快捷键</span>
            <button type="button" className="shortcut-inline-button" onClick={openShortcutPanel}>
              <span>{config.shortcut}</span>
              <small>点击编辑</small>
            </button>
          </div>
        </section>

        <section className="console-block">
          <button
            type="button"
            className="collapsible-title"
            aria-expanded={serviceExpanded}
            onClick={() => setServiceExpanded((expanded) => !expanded)}
          >
            <span>识别服务</span>
            <small>{configStatus.provider_ready ? "已就绪" : "待配置"}</small>
          </button>
          {serviceExpanded ? (
            <div className="collapsible-content">
              <label>
                服务地址
                <input
                  value={config.provider.base_url}
                  onInput={(event) => updateProvider({ base_url: event.currentTarget.value })}
                />
              </label>

              <label>
                鉴权方式
                <select
                  value={config.provider.auth_mode}
                  onChange={(event) => updateProvider({ auth_mode: event.currentTarget.value })}
                >
                  <option value="app_access">应用密钥 + 访问密钥</option>
                  <option value="api_key">接口密钥</option>
                </select>
              </label>

              {config.provider.auth_mode === "api_key" ? (
                <label>
                  接口密钥
                  <input
                    type="password"
                    value={apiKey}
                    placeholder={configStatus.has_api_key ? "已保存；留空不变" : "粘贴接口密钥"}
                    onInput={(event) => setApiKey(event.currentTarget.value)}
                  />
                </label>
              ) : (
                <div className="drawer-grid">
                  <label>
                    应用密钥
                    <input
                      type="password"
                      value={appKey}
                      placeholder={configStatus.has_app_key ? "已保存；留空不变" : "粘贴应用密钥"}
                      onInput={(event) => setAppKey(event.currentTarget.value)}
                    />
                  </label>
                  <label>
                    访问密钥
                    <input
                      type="password"
                      value={accessKey}
                      placeholder={configStatus.has_access_key ? "已保存；留空不变" : "粘贴访问密钥"}
                      onInput={(event) => setAccessKey(event.currentTarget.value)}
                    />
                  </label>
                </div>
              )}

              <label>
                资源标识
                <input
                  value={config.provider.resource_id}
                  placeholder="volc.bigasr.sauc.duration"
                  onInput={(event) => updateProvider({ resource_id: event.currentTarget.value })}
                />
              </label>

              <div className="drawer-grid">
                <label>
                  模型
                  <input
                    value={config.provider.model}
                    onInput={(event) => updateProvider({ model: event.currentTarget.value })}
                  />
                </label>
                <label>
                  语言
                  <input
                    value={config.provider.language}
                    onInput={(event) => updateProvider({ language: event.currentTarget.value })}
                  />
                </label>
              </div>
            </div>
          ) : null}
        </section>

        <section className="console-block">
          <div className="console-title">识别行为</div>
          <div className="behavior-switch-list" aria-label="识别行为设置">
            <BehaviorSwitch
              label="标点"
              description="自动补全逗号、句号等标点"
              checked={config.recognition_behavior.enable_punc}
              onChange={(checked) => void saveRecognitionBehavior({ enable_punc: checked })}
            />
            <BehaviorSwitch
              label="文本规范化"
              description="将日期、数字等口语表达转为书面形式"
              checked={config.recognition_behavior.enable_itn}
              onChange={(checked) => void saveRecognitionBehavior({ enable_itn: checked })}
            />
            <BehaviorSwitch
              label="语义顺滑"
              description="减少口语停顿词和重复表达"
              checked={config.recognition_behavior.enable_ddc}
              onChange={(checked) => void saveRecognitionBehavior({ enable_ddc: checked })}
            />
            <BehaviorSwitch
              label="首字加速"
              description="尽快返回开头文字，可能略降首字准确率"
              checked={config.recognition_behavior.enable_accelerate_text}
              onChange={(checked) =>
                void saveRecognitionBehavior({ enable_accelerate_text: checked })
              }
            />
          </div>
        </section>

        <section className="console-block">
          <div className="console-title">历史记录</div>
          <div className="history-settings-row">
            <label className="switch">
              <input
                type="checkbox"
                checked={config.history_enabled}
                onChange={(event) =>
                  setConfig((current) => ({
                    ...current,
                    history_enabled: event.currentTarget.checked,
                  }))
                }
              />
              <span>{config.history_enabled ? "记录历史" : "停止记录"}</span>
            </label>
            <button type="button" className="secondary" onClick={openHistoryPanel}>
              打开历史
            </button>
          </div>
        </section>

        <section className="console-block">
          <button
            type="button"
            className="collapsible-title"
            aria-expanded={hotwordExpanded}
            onClick={() => setHotwordExpanded((expanded) => !expanded)}
          >
            <span className="title-with-status">
              <span
                className={`status-dot ${hotwordAgentOnline ? "online" : ""}`}
                title={hotwordAgentOnline ? "LLM 已配置且最近无错误" : "LLM 未连通或存在最近错误"}
                aria-hidden="true"
              />
              <span>热词与上下文</span>
            </span>
            <small>{hotwordAgentOnline ? "已连通" : "待配置"}</small>
          </button>
          {hotwordExpanded ? (
            <div className="collapsible-content">
              <label className="switch">
                <input
                  type="checkbox"
                  checked={config.hotwords_enabled}
                  onChange={(event) =>
                    setConfig((current) => ({
                      ...current,
                      hotwords_enabled: event.currentTarget.checked,
                    }))
                  }
                />
                <span>{config.hotwords_enabled ? "语音识别时注入热词" : "不注入热词"}</span>
              </label>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={config.hotword_agent_enabled}
                  onChange={(event) =>
                    setConfig((current) => ({
                      ...current,
                      hotword_agent_enabled: event.currentTarget.checked,
                    }))
                  }
                />
                <span>{config.hotword_agent_enabled ? "每 20 条历史自动整理" : "仅手动整理"}</span>
              </label>
              <button
                type="button"
                className="collapsible-subtitle"
                aria-expanded={hotwordApiExpanded}
                onClick={() => setHotwordApiExpanded((expanded) => !expanded)}
              >
                <span>DeepSeek 配置</span>
                <small>{hotwordState?.has_hotword_agent_api_key ? "已保存密钥" : "待填写"}</small>
              </button>
              {hotwordApiExpanded ? (
                <div className="collapsible-content nested">
                  <label>
                    DeepSeek 地址
                    <input
                      value={config.hotword_agent_base_url}
                      onInput={(event) =>
                        setConfig((current) => ({
                          ...current,
                          hotword_agent_base_url: event.currentTarget.value,
                        }))
                      }
                    />
                  </label>
                  <label>
                    DeepSeek 模型
                    <input
                      value={config.hotword_agent_model}
                      onInput={(event) =>
                        setConfig((current) => ({
                          ...current,
                          hotword_agent_model: event.currentTarget.value,
                        }))
                      }
                    />
                  </label>
                  <label>
                    DeepSeek 密钥
                    <input
                      type="password"
                      value={hotwordApiKey}
                      placeholder={hotwordState?.has_hotword_agent_api_key ? "已保存；留空不变" : "粘贴 DeepSeek API Key"}
                      onInput={(event) => setHotwordApiKey(event.currentTarget.value)}
                    />
                  </label>
                </div>
              ) : null}
              <div className="behavior-grid" aria-label="热词状态">
                <span>待整理</span>
                <strong>{hotwordState?.pending_count ?? 0} 条</strong>
                <span>最近整理</span>
                <strong>{hotwordState?.updated_at || "暂无"}</strong>
              </div>
              <div className="drawer-actions compact">
                <button type="button" className="secondary" onClick={organizeHotwordsNow} disabled={hotwordLoading}>
                  整理热词
                </button>
                <button type="button" className="secondary" onClick={openHotwordPanel}>
                  管理词库
                </button>
              </div>
              {hotwordState?.last_error ? <p className="config-message">最近错误：{hotwordState.last_error}</p> : null}
            </div>
          ) : null}
        </section>

        <footer className="drawer-actions">
          <button type="button" onClick={saveConfig}>
            保存
          </button>
          <button
            type="button"
            className={`secondary ${smokeStatus === "passed" ? "tested" : ""}`}
            onClick={runSmokeTest}
            disabled={smokeStatus === "running"}
          >
            {smokeStatus === "passed"
              ? "✓ 已测试"
              : smokeStatus === "running"
                ? "测试中"
                : smokeStatus === "failed"
                  ? "重新测试"
                  : "冒烟测试"}
          </button>
        </footer>

        {notice ? <p className="notice">{notice}</p> : null}
      </aside>

      {historyOpen ? (
        <section className="history-backdrop" onClick={() => setHistoryOpen(false)}>
          <div className="history-card" role="dialog" aria-label="历史记录" onClick={(event) => event.stopPropagation()}>
            <header className="history-header">
              <div>
                <p className="drawer-kicker">Zephyr / 历史记录</p>
                <h2>输入历史</h2>
              </div>
              <div className="history-header-actions">
                <button type="button" className="secondary" onClick={() => loadHistory()}>
                  刷新
                </button>
                <button type="button" className="secondary danger" onClick={clearAllHistory}>
                  清空
                </button>
                <button type="button" className="drawer-close" onClick={() => setHistoryOpen(false)}>
                  关闭
                </button>
              </div>
            </header>

            <div className="history-search">
              <input
                value={historyQuery}
                placeholder="搜索文本、应用或窗口标题"
                onInput={(event) => {
                  const query = event.currentTarget.value;
                  setHistoryQuery(query);
                  void loadHistory(query);
                }}
              />
            </div>

            <div className="history-layout">
              <div className="history-list" aria-busy={historyLoading}>
                {historyItems.length === 0 ? (
                  <p className="history-empty">{historyLoading ? "正在加载..." : "暂无历史记录"}</p>
                ) : (
                  historyItems.map((item) => (
                    <button
                      type="button"
                      key={item.id}
                      className={`history-item ${item.id === selectedHistoryId ? "selected" : ""}`}
                      onClick={() => selectHistoryItem(item)}
                    >
                      <span className="history-time">{item.created_at}</span>
                      <strong>{item.app_name || "未知应用"}</strong>
                      <span>{historySummary(item)}</span>
                      <small>{item.char_count} 字</small>
                    </button>
                  ))
                )}
              </div>

              <div className="history-detail">
                {selectedHistoryId ? (
                  <>
                    <textarea
                      value={editingHistoryText}
                      onInput={(event) => setEditingHistoryText(event.currentTarget.value)}
                    />
                    <div className="history-detail-meta">
                      {selectedHistoryItem(historyItems, selectedHistoryId)?.app_title || "无窗口标题"}
                    </div>
                    <div className="drawer-actions">
                      <button type="button" onClick={saveHistoryItem}>
                        保存修改
                      </button>
                      <button type="button" className="secondary" onClick={copyHistoryItem}>
                        复制
                      </button>
                      <button type="button" className="secondary danger" onClick={deleteHistoryItem}>
                        删除
                      </button>
                    </div>
                  </>
                ) : (
                  <p className="history-empty">选择一条历史记录查看全文</p>
                )}
              </div>
            </div>

            {historyNotice ? <p className="notice">{historyNotice}</p> : null}
          </div>
        </section>
      ) : null}

      {shortcutOpen ? (
        <section className="history-backdrop" onClick={() => setShortcutOpen(false)}>
          <div
            className="history-card shortcut-card"
            role="dialog"
            aria-label="设置快捷键"
            tabIndex={0}
            ref={shortcutDialogRef}
            onKeyDown={captureShortcut}
            onClick={(event) => event.stopPropagation()}
          >
            <header className="history-header">
              <div>
                <p className="drawer-kicker">Zephyr / 快捷键</p>
                <h2>设置快捷键</h2>
              </div>
              <button type="button" className="drawer-close" onClick={() => setShortcutOpen(false)}>
                关闭
              </button>
            </header>

            <div className="shortcut-capture-box">
              <span>当前快捷键</span>
              <strong>{config.shortcut}</strong>
              <span>新的快捷键</span>
              <kbd>{shortcutDraft || "等待输入"}</kbd>
            </div>

            <p className={`shortcut-state ${shortcutValidation?.available ? "available" : shortcutValidation ? "blocked" : ""}`}>
              {shortcutChecking ? "正在检测..." : shortcutNotice}
            </p>

            <div className="shortcut-suggestions" aria-label="推荐快捷键">
              {["Ctrl+Space", "Alt+V", "Ctrl+Alt+Space", "Ctrl+Shift+Space"].map((item) => (
                <button type="button" className="secondary" key={item} onClick={() => validateShortcutCandidate(item)}>
                  {item}
                </button>
              ))}
            </div>

            <footer className="drawer-actions">
              <button type="button" onClick={saveShortcut} disabled={!shortcutValidation?.available}>
                保存
              </button>
              <button type="button" className="secondary" onClick={clearShortcutDraft}>
                清空
              </button>
              <button type="button" className="secondary" onClick={resetShortcut}>
                恢复默认
              </button>
              <button type="button" className="secondary" onClick={() => setShortcutOpen(false)}>
                取消
              </button>
            </footer>
          </div>
        </section>
      ) : null}

      {hotwordOpen ? (
        <section className="history-backdrop" onClick={() => setHotwordOpen(false)}>
          <div className="history-card hotword-card" role="dialog" aria-label="热词库" onClick={(event) => event.stopPropagation()}>
            <header className="history-header">
              <div>
                <p className="drawer-kicker">Zephyr / 热词管理</p>
                <h2>热词库</h2>
              </div>
              <div className="history-header-actions">
                <button type="button" className="secondary" onClick={refreshHotwordState}>
                  刷新
                </button>
                <button type="button" className="secondary" onClick={organizeHotwordsNow} disabled={hotwordLoading}>
                  整理热词
                </button>
                <button type="button" className="drawer-close" onClick={() => setHotwordOpen(false)}>
                  关闭
                </button>
              </div>
            </header>

            <div className="hotword-layout">
              <section className="hotword-panel hotword-panel-wide">
                <div className="console-title">热词管理</div>
                <div className="hotword-add-row">
                  <input
                    value={newHotwordText}
                    placeholder="添加热词，例如：Zephyr"
                    onInput={(event) => setNewHotwordText(event.currentTarget.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void addHotword();
                    }}
                  />
                  <button type="button" onClick={addHotword}>
                    添加
                  </button>
                </div>
                <div className="hotword-edit-list">
                  {hotwordState && mergeHotwords(hotwordState).length ? (
                    mergeHotwords(hotwordState).map((word) => (
                      <div className="hotword-edit-item" key={word}>
                        <input
                          value={hotwordEdits[word] ?? word}
                          onInput={(event) =>
                            setHotwordEdits((current) => ({
                              ...current,
                              [word]: event.currentTarget.value,
                            }))
                          }
                        />
                        <button type="button" onClick={() => updateHotword(word)}>
                          保存
                        </button>
                        <button type="button" className="danger" onClick={() => deleteHotword(word)}>
                          删除
                        </button>
                      </div>
                    ))
                  ) : (
                    <p className="history-empty">暂无热词</p>
                  )}
                </div>
              </section>

              <section className="hotword-panel">
                <div className="console-title">个人上下文</div>
                <textarea
                  className="hotword-textarea compact"
                  value={profileContextText}
                  placeholder="例如：用户经常讨论语音输入、Rust、Windows 桌面工具。"
                  onInput={(event) => setProfileContextText(event.currentTarget.value)}
                />
                <div className="drawer-actions compact">
                  <button type="button" onClick={saveProfileContext}>
                    保存上下文
                  </button>
                </div>
              </section>

              <section className="hotword-panel">
                <div className="console-title">应用上下文</div>
                <p className="config-message">为不同应用保存场景说明，语音识别时会按当前前台应用注入。</p>
                <div className="drawer-grid">
                  <label>
                    应用名
                    <input
                      value={appContextName}
                      placeholder="Code.exe"
                      onInput={(event) => setAppContextName(event.currentTarget.value)}
                    />
                  </label>
                  <label>
                    场景
                    <input
                      value={appContextText}
                      placeholder="在代码编辑器中输入技术方案"
                      onInput={(event) => setAppContextText(event.currentTarget.value)}
                    />
                  </label>
                </div>
                <div className="drawer-actions compact">
                  <button type="button" onClick={saveAppContext}>
                    添加应用上下文
                  </button>
                </div>
                <div className="app-context-list">
                  {hotwordState?.app_contexts.length ? (
                    hotwordState.app_contexts.map((item) => (
                      <div className="app-context-item editable" key={item.app_name}>
                        <strong>{item.app_name}</strong>
                        <textarea
                          className="hotword-textarea mini"
                          value={appContextEdits[item.app_name]?.context ?? item.context}
                          onInput={(event) =>
                            updateAppContextDraft(item.app_name, event.currentTarget.value)
                          }
                        />
                        <div className="drawer-actions compact">
                          <button type="button" onClick={() => saveExistingAppContext(item.app_name)}>
                            保存
                          </button>
                          <button type="button" className="secondary danger" onClick={() => deleteAppContext(item.app_name)}>
                            删除
                          </button>
                        </div>
                      </div>
                    ))
                  ) : (
                    <p className="history-empty">暂无应用上下文</p>
                  )}
                </div>
              </section>
            </div>

            {hotwordNotice ? <p className="notice">{hotwordNotice}</p> : null}
          </div>
        </section>
      ) : null}
    </main>
  );
}

function BehaviorSwitch({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className={`behavior-switch ${checked ? "" : "disabled"}`}>
      <span className="behavior-switch-copy">
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
    </label>
  );
}

function historySummary(item: HistoryItem) {
  const text = item.text.replace(/\s+/g, " ").trim();
  return text.length > 56 ? `${text.slice(0, 56)}...` : text;
}

function selectedHistoryItem(items: HistoryItem[], id: string | null) {
  return items.find((item) => item.id === id);
}

function mergeHotwords(state: HotwordState) {
  return Array.from(new Set([...state.manual_hotwords, ...state.agent_hotwords])).filter(Boolean);
}

type ShortcutKeyEvent = Pick<KeyboardEvent, "altKey" | "code" | "ctrlKey" | "key" | "metaKey" | "shiftKey">;

function shortcutFromKeyboardEvent(event: ShortcutKeyEvent) {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Win");

  const key = keyLabelFromCode(event.code, event.key);
  if (key) parts.push(key);

  return parts.join("+");
}

function keyLabelFromCode(code: string, key: string) {
  if (["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"].includes(code)) {
    return "";
  }
  if (code === "Space") return "Space";
  if (code.startsWith("Key") && code.length === 4) return code.slice(3).toUpperCase();
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;

  const named: Record<string, string> = {
    Tab: "Tab",
    Enter: "Enter",
    Escape: "Escape",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
  };
  if (named[code]) return named[code];
  if (key.length === 1 && /[a-z0-9]/i.test(key)) return key.toUpperCase();
  return "";
}

function PreInputOverlay() {
  const [payload, setPayload] = useState<PreInputPayload>({
    sessionId: 0,
    text: "",
    state: "recording",
    confirmedChars: 0,
    message: "正在聆听",
    seq: 0,
  });
  const [visible, setVisible] = useState(false);
  const latestSession = useRef(0);
  const latestSeq = useRef(0);
  const closedSession = useRef(0);

  useEffect(() => {
    document.documentElement.classList.add("preinput-root");
    document.body.classList.add("preinput-body");
    let disposed = false;
    let fastSyncTimer: number | undefined;
    let steadySyncStartTimer: number | undefined;

    const acceptPayload = (nextPayload: PreInputPayload) => {
      if (nextPayload.sessionId < latestSession.current) return;
      if (nextPayload.sessionId === closedSession.current) return;
      if (nextPayload.sessionId > latestSession.current) {
        latestSession.current = nextPayload.sessionId;
        latestSeq.current = 0;
      }
      if (nextPayload.seq <= latestSeq.current) return;
      latestSeq.current = nextPayload.seq;
      setPayload(nextPayload);
      setVisible(true);
    };

    const unlistenShow = listen<PreInputPayload>("preinput_show", (event) => {
      acceptPayload(event.payload);
    });
    const unlistenUpdate = listen<PreInputPayload>("preinput_update", (event) => {
      acceptPayload(event.payload);
    });
    const unlistenHide = listen<PreInputPayload>("preinput_hide", (event) => {
      if (event.payload.sessionId >= latestSession.current) {
        latestSession.current = event.payload.sessionId;
        latestSeq.current = event.payload.seq;
        closedSession.current = event.payload.sessionId;
      }
      setVisible(false);
    });
    const syncPayload = async () => {
      try {
        const nextPayload = await invoke<PreInputPayload | null>("get_preinput_payload");
        if (disposed) return;
        if (nextPayload) {
          acceptPayload(nextPayload);
        } else {
          setVisible(false);
        }
      } catch {
        // 后端尚未就绪时，悬浮预输入框保持安静。
      }
    };
    syncPayload();
    fastSyncTimer = window.setInterval(syncPayload, 50);
    steadySyncStartTimer = window.setTimeout(() => {
      if (fastSyncTimer !== undefined) {
        window.clearInterval(fastSyncTimer);
        fastSyncTimer = undefined;
      }
    }, 1000);

    return () => {
      disposed = true;
      if (fastSyncTimer !== undefined) window.clearInterval(fastSyncTimer);
      if (steadySyncStartTimer !== undefined) window.clearTimeout(steadySyncStartTimer);
      document.documentElement.classList.remove("preinput-root");
      document.body.classList.remove("preinput-body");
      unlistenShow.then((dispose) => dispose());
      unlistenUpdate.then((dispose) => dispose());
      unlistenHide.then((dispose) => dispose());
    };
  }, []);

  const text = payload.text || "";
  const characters = Array.from(text);
  const hiddenPrefixChars = Math.max(0, characters.length - maxOverlayCharacters);
  const visibleCharacters = characters.slice(hiddenPrefixChars);
  const confirmedChars = Math.min(payload.confirmedChars ?? 0, characters.length);
  const visibleConfirmedChars = Math.max(0, confirmedChars - hiddenPrefixChars);
  const confirmedText = visibleCharacters.slice(0, visibleConfirmedChars).join("");
  const pendingText = visibleCharacters.slice(visibleConfirmedChars).join("");

  return (
    <div className={`preinput-shell ${visible ? "visible" : ""} ${payload.state}`}>
      <div className="preinput-topline">
        <span className="preinput-dot" />
        <span>{payload.message || stateLabel(payload.state)}</span>
      </div>
      <div className="preinput-text" aria-live="polite">
        {text ? (
          <>
            {hiddenPrefixChars > 0 ? <span className="prefix-fade">...</span> : null}
            <span className="confirmed">{confirmedText}</span>
            <span>{pendingText}</span>
          </>
        ) : (
          <RoseCurveLoader />
        )}
      </div>
    </div>
  );
}

const ROSE_PARTICLE_COUNT = 54;

function RoseCurveLoader() {
  const groupRef = useRef<SVGGElement | null>(null);
  const pathRef = useRef<SVGPathElement | null>(null);
  const particleRefs = useRef<Array<SVGCircleElement | null>>([]);

  useEffect(() => {
    let frameId = 0;
    const startedAt = performance.now();

    const renderFrame = (now: number) => {
      const elapsed = now - startedAt;
      const progress = (elapsed % 3600) / 3600;
      const detailScale = getRoseDetailScale(elapsed);
      const rotation = -((elapsed % 22000) / 22000) * 360;

      groupRef.current?.setAttribute("transform", `rotate(${rotation.toFixed(2)} 50 50)`);
      pathRef.current?.setAttribute("d", buildRosePath(detailScale));

      particleRefs.current.forEach((node, index) => {
        if (!node) return;
        const particle = getRoseParticle(index, progress, detailScale);
        node.setAttribute("cx", particle.x.toFixed(2));
        node.setAttribute("cy", particle.y.toFixed(2));
        node.setAttribute("r", particle.radius.toFixed(2));
        node.setAttribute("opacity", particle.opacity.toFixed(3));
      });

      frameId = requestAnimationFrame(renderFrame);
    };

    frameId = requestAnimationFrame(renderFrame);
    return () => cancelAnimationFrame(frameId);
  }, []);

  return (
    <span className="rose-loader" aria-label="正在聆听">
      <svg className="rose-curve" viewBox="0 0 100 100" aria-hidden="true">
        <g ref={groupRef}>
          <path ref={pathRef} className="rose-track" />
          {Array.from({ length: ROSE_PARTICLE_COUNT }, (_, index) => (
            <circle
              key={index}
              ref={(node) => {
                particleRefs.current[index] = node;
              }}
              className="rose-particle"
            />
          ))}
        </g>
      </svg>
      <span className="rose-label">正在聆听</span>
    </span>
  );
}

function normalizeRoseProgress(progress: number) {
  return ((progress % 1) + 1) % 1;
}

function getRoseDetailScale(elapsed: number) {
  const pulseProgress = (elapsed % 4300) / 4300;
  return 0.52 + ((Math.sin(pulseProgress * Math.PI * 2 + 0.55) + 1) / 2) * 0.48;
}

function getRosePoint(progress: number, detailScale: number) {
  const t = progress * Math.PI * 2;
  const a = 9.2 + detailScale * 0.6;
  const r = a * (0.72 + detailScale * 0.28) * Math.cos(4 * t);

  return {
    x: 50 + Math.cos(t) * r * 3.25,
    y: 50 + Math.sin(t) * r * 3.25,
  };
}

function buildRosePath(detailScale: number, steps = 360) {
  return Array.from({ length: steps + 1 }, (_, index) => {
    const point = getRosePoint(index / steps, detailScale);
    return `${index === 0 ? "M" : "L"} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`;
  }).join(" ");
}

function getRoseParticle(index: number, progress: number, detailScale: number) {
  const tailOffset = index / (ROSE_PARTICLE_COUNT - 1);
  const point = getRosePoint(normalizeRoseProgress(progress - tailOffset * 0.34), detailScale);
  const fade = Math.pow(1 - tailOffset, 0.58);

  return {
    x: point.x,
    y: point.y,
    radius: 0.62 + fade * 2.1,
    opacity: 0.03 + fade * 0.82,
  };
}

function stateLabel(state: PreInputPayload["state"]) {
  switch (state) {
    case "transcribing":
      return "正在识别";
    case "finalizing":
      return "正在写入";
    case "dismissing":
      return "正在收起";
    case "error":
      return "失败";
    case "recording":
    default:
      return "正在聆听";
  }
}

const params = new URLSearchParams(window.location.search);
const isPreinput = params.get("window") === "preinput";

render(isPreinput ? <PreInputOverlay /> : <App />, document.getElementById("app")!);
