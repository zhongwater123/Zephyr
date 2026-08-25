import type { JSX } from "preact";
import { useEffect, useRef } from "preact/hooks";
import type { ShortcutPreview } from "../../domain";

export function ShortcutDialog({
  open,
  currentShortcut,
  draft,
  preview,
  checking,
  notice,
  onClose,
  onCapture,
  onExclusive,
  onRetry,
}: {
  open: boolean;
  currentShortcut: string;
  draft: string;
  preview: ShortcutPreview | null;
  checking: boolean;
  notice: string;
  onClose: () => void;
  onCapture: (event: JSX.TargetedKeyboardEvent<HTMLButtonElement>) => void;
  onExclusive: () => void;
  onRetry: () => void;
}) {
  const captureRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (open && preview?.state !== "occupied") {
      window.setTimeout(() => captureRef.current?.focus(), 0);
    }
  }, [open, preview?.state]);
  if (!open) return null;

  const occupied = preview?.state === "occupied";
  const failed = preview?.state === "invalid";
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

        <p className="shortcut-current">
          当前：<kbd>{currentShortcut}</kbd>
        </p>

        <button type="button" className="shortcut-capture-box" ref={captureRef}
          onKeyDown={onCapture} aria-busy={checking}>
          <span>按下新的快捷键</span>
          <kbd>{draft || "等待输入"}</kbd>
          <small>系统会自动检查并保存。</small>
        </button>

        {occupied ? (
          <div className="shortcut-conflict" role="alert">
            <strong>这个快捷键已被其他应用占用</strong>
            <p>如果无法在对方应用中释放，可以让本应用优先响应这个组合键。</p>
            <div>
              <button type="button" onClick={onExclusive}>使用独占模式</button>
              <button type="button" className="secondary" onClick={onRetry}>换一个快捷键</button>
            </div>
          </div>
        ) : (
          <p className={`shortcut-state ${failed ? "blocked" : ""}`} role="status">
            {checking ? "正在检查…" : notice}
          </p>
        )}
      </div>
    </section>
  );
}
