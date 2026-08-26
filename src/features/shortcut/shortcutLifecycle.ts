import type {
  ShortcutLifecycleSnapshot,
  ShortcutOperationKind,
  ShortcutOperationPhase,
} from "../../domain";

export type ShortcutLifecycleState = {
  snapshot: ShortcutLifecycleSnapshot | null;
};

export type ShortcutLifecycleAction =
  | {
      type: "snapshot_received";
      snapshot: ShortcutLifecycleSnapshot;
      allowOperationChange?: boolean;
    }
  | { type: "session_closed" };

export type ShortcutLifecycleViewModel = {
  activeLabel: string;
  displayLabel: string;
  candidateLabel: string;
  runtimeState: ShortcutLifecycleSnapshot["runtime"]["state"];
  operationId: number | null;
  operationKind: ShortcutOperationKind | null;
  phase: ShortcutOperationPhase | null;
  message: string;
  errorCode: string | null;
  retryable: boolean;
  busy: boolean;
  capturing: boolean;
  terminal: boolean;
  failed: boolean;
  changed: boolean | null;
  canUndo: boolean;
};

const ACTIVE_PHASES: ReadonlySet<ShortcutOperationPhase> = new Set([
  "starting",
  "capturing",
  "validating",
  "applying",
]);

export const initialShortcutLifecycleState: ShortcutLifecycleState = {
  snapshot: null,
};

export function shortcutLifecycleReducer(
  state: ShortcutLifecycleState,
  action: ShortcutLifecycleAction,
): ShortcutLifecycleState {
  if (action.type === "session_closed") {
    return { snapshot: state.snapshot ? { ...state.snapshot, operation: null } : null };
  }
  const current = state.snapshot;
  const incoming = action.snapshot;
  if (current && incoming.sequence <= current.sequence) return state;

  const currentOperationId = current?.operation?.operationId ?? null;
  const incomingOperationId = incoming.operation?.operationId ?? null;
  if (
    !action.allowOperationChange
    && currentOperationId !== null
    && incomingOperationId !== currentOperationId
  ) {
    return state;
  }
  return { snapshot: incoming };
}

export function selectShortcutLifecycle(
  snapshot: ShortcutLifecycleSnapshot | null,
  fallbackLabel: string,
): ShortcutLifecycleViewModel {
  const activeLabel = snapshot?.runtime.activeLabel || fallbackLabel;
  const runtimeState = snapshot?.runtime.state ?? "disabled";
  const operation = snapshot?.operation ?? null;
  const phase = operation?.phase ?? null;
  const busy = phase !== null && ACTIVE_PHASES.has(phase);
  const candidateLabel = operation?.candidateLabel ?? "";
  const displayLabel = phase === "starting"
    ? activeLabel
    : busy
      ? candidateLabel
      : activeLabel;
  const runtimeFailed = runtimeState === "error";
  const failed = phase === "failed";

  let message = operation?.message || (runtimeFailed ? snapshot?.runtime.message ?? "" : "");
  if (phase === "succeeded" && operation?.kind === "capture") {
    message = "";
  }

  return {
    activeLabel,
    displayLabel,
    candidateLabel,
    runtimeState,
    operationId: operation?.operationId ?? null,
    operationKind: operation?.kind ?? null,
    phase,
    message,
    errorCode: operation?.errorCode ?? null,
    retryable: operation?.retryable ?? false,
    busy,
    capturing: phase === "capturing",
    terminal: phase === "succeeded" || phase === "failed" || phase === "cancelled",
    failed,
    changed: operation?.changed ?? null,
    canUndo:
      phase === "succeeded"
      && operation?.changed === true
      && (operation?.kind === "capture" || operation?.kind === "restore_default"),
  };
}
