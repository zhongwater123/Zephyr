import { useRef } from "preact/hooks";
import type { Dispatch, StateUpdater } from "preact/hooks";
import type { AppConfig } from "../domain";
import { commandErrorMessage, parseCommandError } from "../security-model";

export function useRevisionedConfigMutation(
  setConfig: Dispatch<StateUpdater<AppConfig>>,
  refreshStatus: () => void | Promise<void>,
) {
  const latestSequence = useRef(0);

  return {
    begin() {
      latestSequence.current += 1;
      return latestSequence.current;
    },
    isLatest(sequence: number) {
      return sequence === latestSequence.current;
    },
    describeError(error: unknown) {
      const payload = parseCommandError(error);
      if (payload?.code === "config_conflict" && payload.details?.currentConfig) {
        latestSequence.current += 1;
        setConfig(payload.details.currentConfig);
        void refreshStatus();
        return `${payload.message}（已载入 revision ${payload.details.currentRevision ?? "?"}）`;
      }
      if (
        payload?.code === "voice_reconciliation_failed" &&
        typeof payload.details?.committedRevision === "number"
      ) {
        void refreshStatus();
        return `${payload.message}（配置已提交 revision ${payload.details.committedRevision}，后续操作将重试协调）`;
      }
      return payload?.message ?? commandErrorMessage(error);
    },
  };
}
