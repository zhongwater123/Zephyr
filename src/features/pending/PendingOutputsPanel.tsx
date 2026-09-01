import { useEffect, useState } from "preact/hooks";
import type { PendingOutput } from "../../domain";

export function PendingOutputsPanel({
  outputs,
  onDeliver,
  onCopy,
  onDiscard,
}: {
  outputs: PendingOutput[];
  onDeliver: (id: string, confirmUncertain: boolean) => void;
  onCopy: (id: string) => void;
  onDiscard: (id: string) => void;
}) {
  const [armedId, setArmedId] = useState<string | null>(null);

  useEffect(() => setArmedId(null), [outputs]);

  return (
    <section className="console-block">
      <div className="console-title">待处理结果 · {outputs.length}/5</div>
      {outputs.length === 0 ? (
        <p className="config-message">没有因目标变化、校验或注入失败而保留的文本。</p>
      ) : (
        outputs.map((output) => {
          const uncertain = output.deliveryCertainty === "mayHaveBeenSubmitted";
          const armed = uncertain && armedId === output.id;
          return (
            <div className="pending-output" key={output.id}>
              <strong>{output.executableName}</strong>
              <p>{output.text}</p>
              <small>{output.reasonMessage}</small>
              {uncertain ? (
                <p className="config-message" role="alert">
                  文本可能已经输入，请先检查原目标窗口。系统不会自动再次发送。
                </p>
              ) : null}
              <div className="drawer-actions compact">
                <button
                  type="button"
                  disabled={!output.targetAvailable}
                  onClick={() => {
                    if (uncertain && !armed) {
                      setArmedId(output.id);
                    } else {
                      onDeliver(output.id, uncertain);
                    }
                  }}
                >
                  {armed ? "确认再次发送" : "发送到原窗口"}
                </button>
                {armed ? (
                  <button type="button" className="secondary" onClick={() => setArmedId(null)}>
                    取消
                  </button>
                ) : null}
                <button type="button" className="secondary" onClick={() => onCopy(output.id)}>复制文本</button>
                <button type="button" className="secondary danger" onClick={() => onDiscard(output.id)}>丢弃</button>
              </div>
            </div>
          );
        })
      )}
    </section>
  );
}
