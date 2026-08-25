export type ConfigValue = {
  type: "boolean";
  value: boolean;
};

export type ProviderConfigEnvelope = {
  providerId: string;
  schemaVersion: number;
  revision: number;
  values: Record<string, ConfigValue>;
};

export type OptionSpec = {
  id: string;
  controlKind: "toggle";
  label: string;
  description: string;
  defaultValue: ConfigValue;
  group: string;
  order: number;
  enabled: boolean;
  disabledReason?: string | null;
};

export type AsrOptionPool = {
  providerId: string;
  providerDisplayName: string;
  schemaVersion: number;
  revision: number;
  options: OptionSpec[];
  values: Record<string, ConfigValue>;
};
export type AppConfig = {
  schema_version: number;
  revision: number;
  enabled: boolean;
  shortcut: string;
  shortcut_mode: ShortcutMode;
  asr: ProviderConfigEnvelope;
  history_enabled: boolean;
  incident_recovery_enabled: boolean;
  incident_consent_version: number;
  incident_save_failed_audio: boolean;
  incident_save_failed_text: boolean;
  incident_retention_days: number;
  incident_storage_limit_mb: number;
  incident_success_rollup_days: number;
  hotwords_enabled: boolean;
  hotword_agent_enabled: boolean;
  hotword_agent_base_url: string;
  hotword_agent_model: string;
  trusted_endpoints: TrustedEndpoint[];
  injection_overrides: InjectionOverride[];
};

export type EndpointPurpose = "hotword_agent";

export type TrustedEndpoint = {
  origin: string;
  purpose: EndpointPurpose;
};

export type InjectionOverride = {
  executable_name: string;
  strategy: "unicode" | "clipboard_compatibility";
};

export type ConfigStatus = {
  provider_ready: boolean;
  provider_message: string;
  recovery_warning?: string | null;
};

export type PendingOutput = {
  id: string;
  sessionId: number;
  text: string;
  executableName: string;
  windowTitle?: string | null;
  createdAtUnixMs: number;
  expiresAtUnixMs: number;
  targetAvailable: boolean;
  reasonCode: string;
  reasonMessage: string;
};

export type CommandErrorPayload = {
  code: string;
  message: string;
  details?: {
    currentRevision?: number;
    currentConfig?: AppConfig;
    currentPool?: AsrOptionPool;
    [key: string]: unknown;
  };
};

export type ShortcutMode = "standard" | "exclusive_hook";

export type ShortcutPreviewState =
  | "reserved_standard"
  | "occupied"
  | "awaiting_hook_test"
  | "hook_verified"
  | "invalid";

export type ShortcutPreview = {
  previewId: number;
  shortcut: string;
  normalized: string;
  mode: ShortcutMode;
  state: ShortcutPreviewState;
  reason: string;
};

export type ShortcutRuntimeStatus = {
  shortcut: string;
  mode: ShortcutMode;
  backend: "register_hotkey" | "low_level_hook" | "none";
  state: "active" | "inactive" | "occupied" | "verifying" | "error";
  message: string;
};

export type VoiceStatePayload = {
  state: string;
  message: string;
  elapsed_ms?: number;
};

export type PreInputPayload = {
  sessionId: number;
  seq: number;
  text: string;
  state: "recording" | "transcribing" | "finalizing" | "dismissing" | "error";
  confirmedChars?: number;
  message?: string;
};

export type IncidentItem = {
  id: string;
  createdAtUtcMs: number;
  terminalOutcome: string;
  failureStage: string;
  failureCode: string;
  failureMessage: string;
  recoverability: string;
  partialText?: string | null;
  finalText?: string | null;
  audioAvailable: boolean;
  audioCompleteness?: string | null;
  pinned: boolean;
  expiresAtUtcMs?: number | null;
  targetApp?: string | null;
};

export type IncidentHealth = {
  available: boolean;
  degraded: boolean;
  controlEventsDropped: number;
  audioChunksDropped: number;
  lastError?: string | null;
};
export type HistoryItem = {
  id: string;
  text: string;
  created_at: string;
  app_name?: string | null;
  app_title?: string | null;
  char_count: number;
};

export type AppHotwordContext = {
  app_name: string;
  context: string;
};

export type HotwordState = {
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

export const defaultConfig: AppConfig = {
  schema_version: 5,
  revision: 0,
  enabled: true,
  shortcut: "Ctrl+Shift+Space",
  shortcut_mode: "standard",
  history_enabled: true,
  incident_recovery_enabled: false,
  incident_consent_version: 0,
  incident_save_failed_audio: true,
  incident_save_failed_text: true,
  incident_retention_days: 7,
  incident_storage_limit_mb: 512,
  incident_success_rollup_days: 30,
  hotwords_enabled: true,
  hotword_agent_enabled: false,
  hotword_agent_base_url: "https://api.deepseek.com",
  hotword_agent_model: "deepseek-v4-flash",
  trusted_endpoints: [
    { origin: "https://api.deepseek.com:443", purpose: "hotword_agent" },
  ],
  injection_overrides: [],
  asr: {
    providerId: "volcengine",
    schemaVersion: 1,
    revision: 0,
    values: {
      punctuation: { type: "boolean", value: true },
      text_normalization: { type: "boolean", value: true },
      semantic_smoothing: { type: "boolean", value: false },
      fast_first_result: { type: "boolean", value: true },
    },
  },
};

export const defaultConfigStatus: ConfigStatus = {
  provider_ready: false,
  provider_message: "正在检查识别服务。",
  recovery_warning: null,
};
