import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { save } from "@tauri-apps/plugin-dialog";
import type { IncidentHealth, IncidentItem } from "../../domain";
import { incidentApi } from "../../ipc/client";


export function IncidentRecoveryPanel() {
  const [items, setItems] = useState<IncidentItem[]>([]);
  const [health, setHealth] = useState<IncidentHealth | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [audioUrl, setAudioUrl] = useState("");
  const audioUrlRef = useRef("");
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(false);
  const [includeText, setIncludeText] = useState(false);
  const [includeAudio, setIncludeAudio] = useState(false);
  const [includeLogs, setIncludeLogs] = useState(false);
  const selected = useMemo(
    () => items.find((item) => item.id === selectedId) ?? null,
    [items, selectedId],
  );

  async function refresh() {
    setLoading(true);
    try {
      const [next, nextHealth] = await Promise.all([incidentApi.list(), incidentApi.health()]);
      setItems(next);
      setHealth(nextHealth);
      setSelectedId((current) => next.some((item) => item.id === current) ? current : next[0]?.id ?? null);
    } catch (error) {
      setNotice("恢复记录读取失败：" + String(error));
    } finally {
      setLoading(false);
    }
  }

  function replaceAudioUrl(nextUrl: string) {
    const previousUrl = audioUrlRef.current;
    if (previousUrl && previousUrl !== nextUrl) URL.revokeObjectURL(previousUrl);
    audioUrlRef.current = nextUrl;
    setAudioUrl(nextUrl);
  }

  useEffect(() => {
    void refresh();
    return () => {
      const currentUrl = audioUrlRef.current;
      audioUrlRef.current = "";
      if (currentUrl) URL.revokeObjectURL(currentUrl);
    };
  }, []);

  async function playAudio() {
    if (!selected) return;
    try {
      const bytes = await incidentApi.audio(selected.id);
      const url = URL.createObjectURL(new Blob([Uint8Array.from(bytes).buffer], { type: "audio/wav" }));
      replaceAudioUrl(url);
    } catch (error) {
      setNotice("音频读取失败：" + String(error));
    }
  }

  async function exportAudio() {
    if (!selected) return;
    try {
      const path = await save({
        defaultPath: `zephyr-${selected.id}.wav`,
        filters: [{ name: "WAV 音频", extensions: ["wav"] }],
      });
      if (path) await incidentApi.saveAudio(selected.id, path);
    } catch (error) {
      setNotice("音频导出失败：" + String(error));
    }
  }

  async function exportReport() {
    if (!selected) return;
    try {
      const path = await save({
        defaultPath: `zephyr-incident-${selected.id}.zip`,
        filters: [{ name: "诊断 ZIP", extensions: ["zip"] }],
      });
      if (path) {
        await incidentApi.saveReport(selected.id, path, {
          includeText,
          includeAudio,
          includeLogExcerpt: includeLogs,
        });
      }
    } catch (error) {
      setNotice("诊断包导出失败：" + String(error));
    }
  }

  async function removeSelected() {
    if (!selected || !window.confirm("删除这条异常记录及其恢复材料？")) return;
    await incidentApi.remove(selected.id);
    replaceAudioUrl("");
    await refresh();
  }

  async function togglePinned() {
    if (!selected) return;
    await incidentApi.setPinned(selected.id, !selected.pinned);
    await refresh();
  }

  return (
    <section className="incident-recovery" aria-labelledby="incident-recovery-title">
      <div className="subsection-heading incident-heading">
        <div>
          <h4 id="incident-recovery-title">需要处理</h4>
          <p>识别或写入异常时留下的本地恢复材料，不会进入正式历史和热词学习。</p>
        </div>
        <button type="button" className="text-button" onClick={() => void refresh()}>刷新</button>
      </div>
      {health?.degraded ? (
        <p className="field-error" role="status">恢复存储已降级：{health.lastError || "容量已达上限，已暂停新材料。"}</p>
      ) : null}
      <div className="embedded-history incident-layout" aria-busy={loading}>
        <div className="embedded-history-list">
          {items.length ? items.map((item) => (
            <button
              type="button"
              key={item.id}
              className={item.id === selectedId ? "is-selected" : ""}
              onClick={() => { setSelectedId(item.id); replaceAudioUrl(""); }}
            >
              <span>{new Date(item.createdAtUtcMs).toLocaleString()}</span>
              <strong>{item.failureStage} · {item.failureCode}</strong>
              <small>{item.failureMessage}</small>
            </button>
          )) : <p className="empty-state">{loading ? "正在加载…" : "没有需要处理的异常记录。"}</p>}
        </div>
        <div className="embedded-history-detail">
          {selected ? (
            <>
              <div className="incident-summary">
                <strong>{selected.failureMessage}</strong>
                <small>恢复完整性：{selected.audioCompleteness || selected.recoverability} · {selected.targetApp || "未记录应用"}</small>
              </div>
              <textarea readOnly value={selected.finalText || selected.partialText || "没有可恢复的文本"} aria-label="可恢复文本" />
              {audioUrl ? <audio controls autoPlay src={audioUrl} /> : null}
              <div className="button-row">
                {(selected.finalText || selected.partialText) ? <button type="button" onClick={() => void incidentApi.copyText(selected.id)}>复制文本</button> : null}
                {selected.audioAvailable ? <button type="button" className="secondary" onClick={() => void playAudio()}>播放音频</button> : null}
                {selected.audioAvailable ? <button type="button" className="secondary" onClick={() => void exportAudio()}>导出 WAV</button> : null}
                <button type="button" className="secondary" onClick={() => void togglePinned()}>{selected.pinned ? "恢复自动过期" : "长期保留"}</button>
                <button type="button" className="text-button danger" onClick={() => void removeSelected()}>删除</button>
              </div>
              <div className="incident-export-options">
                <label><input type="checkbox" checked={includeText} onChange={(event) => setIncludeText(event.currentTarget.checked)} />附带转写文本</label>
                <label><input type="checkbox" checked={includeAudio} disabled={!selected.audioAvailable} onChange={(event) => setIncludeAudio(event.currentTarget.checked)} />附带原始音频</label>
                <label><input type="checkbox" checked={includeLogs} onChange={(event) => setIncludeLogs(event.currentTarget.checked)} />附带脱敏日志片段（最多 256KB）</label>
                <button type="button" className="secondary" onClick={() => void exportReport()}>生成诊断 ZIP</button>
              </div>
            </>
          ) : <p className="empty-state">选择一条异常记录查看恢复材料。</p>}
        </div>
      </div>
      {notice ? <p className="inline-notice" role="status">{notice}</p> : null}
    </section>
  );
}
