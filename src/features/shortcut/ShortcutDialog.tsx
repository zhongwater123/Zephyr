import type { JSX } from "preact";
import { useEffect, useRef } from "preact/hooks";
import type { ShortcutLifecycleViewModel } from "./shortcutLifecycle";

export function ShortcutDialog({
  open,
  view,
  requestPending,
  transportError,
  canRestoreDefault,
  onClose,
  onCapture,
  onRetry,
  onRestoreDefault,
}: {
  open: boolean;
  view: ShortcutLifecycleViewModel;
  requestPending: boolean;
  transportError: string;
  canRestoreDefault: boolean;
  onClose: () => void;
  onCapture: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onRetry: () => void;
  onRestoreDefault: () => void;
}) {
  const captureRef = useRef<HTMLButtonElement>(null);
  const active = view.busy || requestPending;
  const preparing = view.phase === "starting" || (requestPending && !view.busy);
  const capturing = view.phase === "capturing";
  const notice = transportError || view.message;
  const validationWarning = capturing && !transportError && Boolean(view.errorCode);
  const failure = Boolean(transportError || (view.errorCode && !validationWarning));
  const issue = validationWarning || failure;
  useEffect(() => {
    if (open) window.setTimeout(() => captureRef.current?.focus(), 0);
  }, [open, active]);
  if (!open) return null;

  return (
    <section className="history-backdrop" onClick={onClose}>
      <div className="history-card shortcut-card" role="dialog" aria-label="设置快捷键"
        onClick={(event) => event.stopPropagation()}>
        <header className="history-header">
          <div>
            <p className="drawer-kicker">快捷键</p>
            <h2>设置快捷键</h2>
          </div>
          <button type="button" className="drawer-close" onClick={onClose}>关闭</button>
        </header>

        <p className="shortcut-current">当前：<kbd>{view.activeLabel}</kbd></p>

        <button type="button" className="shortcut-capture-box" ref={captureRef}
          onKeyDown={onCapture} aria-busy={active} aria-invalid={issue}>
          <span>{preparing
            ? "正在准备换绑"
            : capturing
              ? "请按下新的快捷键"
              : active
                ? "正在验证并应用快捷键"
                : view.phase === "failed" ? "换绑失败" : "快捷键状态"}</span>
          <kbd>{preparing
            ? "正在准备…"
            : view.displayLabel || (capturing ? "等待输入" : view.activeLabel)}</kbd>
          <small>{preparing
            ? "键盘 Hook 就绪后才会开始录入。"
            : capturing
              ? "按一次完整组合，全部松开后自动保存。"
              : "当前未录入新的按键。"}</small>
        </button>

        <p className={`shortcut-state${validationWarning ? " warning" : failure ? " blocked" : ""}`} role={issue ? "alert" : "status"}>
          {notice}
        </p>

        {!active ? (
          <div className="shortcut-dialog-actions">
            <button type="button" onClick={onRetry}>重新设置</button>
            {canRestoreDefault ? (
              <button type="button" className="secondary" onClick={onRestoreDefault}>恢复默认</button>
            ) : null}
          </div>
        ) : null}
      </div>
    </section>
  );
}
