import type { PendingOutput } from "../../domain";

export function PendingOutputsPanel({
  outputs,
  onDeliver,
  onCopy,
  onDiscard,
}: {
  outputs: PendingOutput[];
  onDeliver: (id: string) => void;
  onCopy: (id: string) => void;
  onDiscard: (id: string) => void;
}) {
  return (
    <section className="console-block">
      <div className="console-title">待处理结果 · {outputs.length}/5</div>
      {outputs.length === 0 ? (
        <p className="config-message">没有因目标变化、校验或注入失败而保留的文本。</p>
      ) : (
        outputs.map((output) => (
          <div className="pending-output" key={output.id}>
            <strong>{output.executableName}</strong>
            <p>{output.text}</p>
            <small>{output.reasonMessage}</small>
            <div className="drawer-actions compact">
              <button type="button" disabled={!output.targetAvailable} onClick={() => onDeliver(output.id)}>发送到原窗口</button>
              <button type="button" className="secondary" onClick={() => onCopy(output.id)}>复制文本</button>
              <button type="button" className="secondary danger" onClick={() => onDiscard(output.id)}>丢弃</button>
            </div>
          </div>
        ))
      )}
    </section>
  );
}
