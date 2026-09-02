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
  shortcut_binding?: ShortcutBinding | null;
  shortcut_trigger_mode: ShortcutTriggerMode;
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
  polish_level: PolishLevel;
  trusted_endpoints: TrustedEndpoint[];
  injection_overrides: InjectionOverride[];
};

export type ShortcutTriggerMode = "hold" | "toggle";

export type EndpointPurpose = "hotword_agent" | "text_processing";

export type PolishLevel = 0 | 1 | 2 | 3;

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
  global_shortcut_supported: boolean;
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
  deliveryCertainty: "retryable" | "mayHaveBeenSubmitted";
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

export type PhysicalKeyId = {
  scanCode: number;
  extended: boolean;
};

export type ShortcutBinding = {
  modifiers: Array<{
    kind: "control" | "alt" | "shift" | "win";
    side: "any" | "left" | "right";
  }>;
  trigger: PhysicalKeyId;
};

export type ShortcutRuntimeState = "active" | "suspended" | "disabled" | "error";
export type ShortcutErrorCode =
  | "invalid_binding"
  | "reserved_binding"
  | "revision_conflict"
  | "hook_unavailable"
  | "persistence_failed"
  | "capture_timeout"
  | "release_timeout"
  | "hook_interrupted"
  | "runtime_rollback_failed";

export type ShortcutEditSession = {
  editId: number;
  traceId: string;
  configRevision: number;
  activeLabel: string;
  activeBinding: ShortcutBinding | null;
  runtimeState: ShortcutRuntimeState;
  errorCode?: ShortcutErrorCode;
  message: string;
};

export type ShortcutEditOutcome = {
  success: boolean;
  editId: number;
  traceId: string;
  configRevision: number;
  activeLabel: string;
  activeBinding: ShortcutBinding | null;
  runtimeState: ShortcutRuntimeState;
  changed: boolean;
  errorCode?: ShortcutErrorCode;
  message: string;
};

export type ShortcutEditInterrupted = {
  outcome: ShortcutEditOutcome;
};

export type ShortcutTraceEvent =
  | "ui_capture_started"
  | "dom_keydown"
  | "dom_keyup"
  | "candidate_rejected"
  | "candidate_finalized"
  | "begin_acknowledged"
  | "commit_dispatched"
  | "commit_completed"
  | "optimistic_rollback"
  | "cancel_requested"
  | "focus_lost"
  | "edit_interrupted";

export type ShortcutEditTraceInput = {
  traceId: string;
  editId?: number | null;
  eventSeq: number;
  elapsedMs: number;
  event: ShortcutTraceEvent;
  code?: string | null;
  key?: string | null;
  location?: number | null;
  repeat?: boolean | null;
  ctrl?: boolean | null;
  alt?: boolean | null;
  shift?: boolean | null;
  meta?: boolean | null;
  altGraph?: boolean | null;
  heldCodes?: string[];
  candidateLabel?: string | null;
  candidateBinding?: ShortcutBinding | null;
  reasonCode?: string | null;
};
export type VoiceState =
  | "Idle"
  | "Starting"
  | "Recording"
  | "Transcribing"
  | "Pasting"
  | "Disabled"
  | "Error";

export type VoiceStatePayload = {
  state: VoiceState;
  message: string;
  elapsed_ms?: number;
};

export type PreInputPayload = {
  sessionId: number;
  seq: number;
  text: string;
  state: "starting" | "recording" | "transcribing" | "finalizing" | "dismissing" | "error";
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
  schema_version: 10,
  revision: 0,
  enabled: true,
  shortcut: "左 Ctrl+左 Shift+Space",
  shortcut_binding: {
    modifiers: [
      { kind: "control", side: "left" },
      { kind: "shift", side: "left" },
    ],
    trigger: { scanCode: 57, extended: false },
  },
  shortcut_trigger_mode: "hold",
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
  polish_level: 2,
  trusted_endpoints: [
    { origin: "https://api.deepseek.com:443", purpose: "hotword_agent" },
    { origin: "https://api.deepseek.com:443", purpose: "text_processing" },
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
  global_shortcut_supported: true,
};
