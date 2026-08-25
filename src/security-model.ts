import type {
  AppConfig,
  CommandErrorPayload,
  EndpointPurpose,
  PendingOutput,
} from "./domain";

export function parseCommandError(error: unknown): CommandErrorPayload | null {
  if (error && typeof error === "object" && "code" in error && "message" in error) {
    return error as CommandErrorPayload;
  }
  if (typeof error === "string") {
    try {
      const parsed = JSON.parse(error) as unknown;
      if (parsed && typeof parsed === "object" && "code" in parsed && "message" in parsed) {
        return parsed as CommandErrorPayload;
      }
    } catch {
      return null;
    }
  }
  return null;
}

export function commandErrorMessage(error: unknown) {
  return parseCommandError(error)?.message ?? String(error);
}

export function normalizeOrigin(endpoint: string) {
  try {
    const url = new URL(endpoint);
    if (url.protocol !== "https:" && url.protocol !== "wss:") return "";
    const port = url.port || "443";
    return `${url.protocol}//${url.hostname.toLowerCase()}:${port}`;
  } catch {
    return "";
  }
}

export function endpointIsTrusted(
  config: AppConfig,
  endpoint: string,
  purpose: EndpointPurpose,
) {
  const origin = normalizeOrigin(endpoint);
  return config.trusted_endpoints.some(
    (entry) => entry.origin === origin && entry.purpose === purpose,
  );
}

export function isOfficialEndpoint(origin: string) {
  return origin === "https://api.deepseek.com:443";
}

export function configAfterLoadFailure(config: AppConfig): AppConfig {
  return { ...config, enabled: false };
}

export function conflictConfig(error: unknown): AppConfig | null {
  const payload = parseCommandError(error);
  return payload?.code === "config_conflict" && payload.details?.currentConfig
    ? payload.details.currentConfig
    : null;
}

export function isLatestMutation(sequence: number, latestSequence: number) {
  return sequence === latestSequence;
}

export function canDeliverPendingOutput(output: PendingOutput, nowUnixMs: number) {
  return output.targetAvailable && output.expiresAtUnixMs > nowUnixMs;
}
