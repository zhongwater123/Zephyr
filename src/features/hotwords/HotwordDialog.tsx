import type { AppHotwordContext, HotwordState } from "../../domain";
import { mergeHotwords } from "./model";

export function HotwordDialog({
  open, state, loading, notice, newWord, edits, profileText, appName, appText, appEdits,
  onClose, onRefresh, onOrganize, onNewWord, onAdd, onEdit, onUpdate, onDelete,
  onProfileText, onSaveProfile, onAppName, onAppText, onSaveApp, onAppDraft,
  onSaveExistingApp, onDeleteApp,
}: {
  open: boolean;
  state: HotwordState | null;
  loading: boolean;
  notice: string;
  newWord: string;
  edits: Record<string, string>;
  profileText: string;
  appName: string;
  appText: string;
  appEdits: Record<string, AppHotwordContext>;
  onClose: () => void;
  onRefresh: () => void;
  onOrganize: () => void;
  onNewWord: (value: string) => void;
  onAdd: () => void;
  onEdit: (word: string, value: string) => void;
  onUpdate: (word: string) => void;
  onDelete: (word: string) => void;
  onProfileText: (value: string) => void;
  onSaveProfile: () => void;
  onAppName: (value: string) => void;
  onAppText: (value: string) => void;
  onSaveApp: () => void;
  onAppDraft: (appName: string, context: string) => void;
  onSaveExistingApp: (appName: string) => void;
  onDeleteApp: (appName: string) => void;
}) {
  if (!open) return null;
  const words = state ? mergeHotwords(state) : [];
  return (
    <section className="history-backdrop" onClick={onClose}>
      <div className="history-card hotword-card" role="dialog" aria-label="热词库" onClick={(event) => event.stopPropagation()}>
        <header className="history-header">
          <div><p className="drawer-kicker">Zephyr / 热词管理</p><h2>热词库</h2></div>
          <div className="history-header-actions">
            <button type="button" className="secondary" onClick={onRefresh}>刷新</button>
            <button type="button" className="secondary" onClick={onOrganize} disabled={loading}>整理热词</button>
            <button type="button" className="drawer-close" onClick={onClose}>关闭</button>
          </div>
        </header>
        <div className="hotword-layout">
          <section className="hotword-panel hotword-panel-wide">
            <div className="console-title">热词管理</div>
            <div className="hotword-add-row">
              <input value={newWord} placeholder="添加热词，例如：Zephyr" onInput={(event) => onNewWord(event.currentTarget.value)} onKeyDown={(event) => { if (event.key === "Enter") onAdd(); }} />
              <button type="button" onClick={onAdd}>添加</button>
            </div>
            <div className="hotword-edit-list">
              {words.length ? words.map((word) => (
                <div className="hotword-edit-item" key={word}>
                  <input value={edits[word] ?? word} onInput={(event) => onEdit(word, event.currentTarget.value)} />
                  <button type="button" onClick={() => onUpdate(word)}>保存</button>
                  <button type="button" className="danger" onClick={() => onDelete(word)}>删除</button>
                </div>
              )) : <p className="history-empty">暂无热词</p>}
            </div>
          </section>
          <section className="hotword-panel">
            <div className="console-title">个人上下文</div>
            <textarea className="hotword-textarea compact" value={profileText} placeholder="例如：用户经常讨论语音输入、Rust、Windows 桌面工具。" onInput={(event) => onProfileText(event.currentTarget.value)} />
            <div className="drawer-actions compact"><button type="button" onClick={onSaveProfile}>保存上下文</button></div>
          </section>
          <section className="hotword-panel">
            <div className="console-title">应用上下文</div>
            <p className="config-message">为不同应用保存场景说明，语音识别时会按当前前台应用注入。</p>
            <div className="drawer-grid">
              <label>应用名<input value={appName} placeholder="Code.exe" onInput={(event) => onAppName(event.currentTarget.value)} /></label>
              <label>场景<input value={appText} placeholder="在代码编辑器中输入技术方案" onInput={(event) => onAppText(event.currentTarget.value)} /></label>
            </div>
            <div className="drawer-actions compact"><button type="button" onClick={onSaveApp}>添加应用上下文</button></div>
            <div className="app-context-list">
              {state?.app_contexts.length ? state.app_contexts.map((item) => (
                <div className="app-context-item editable" key={item.app_name}>
                  <strong>{item.app_name}</strong>
                  <textarea className="hotword-textarea mini" value={appEdits[item.app_name]?.context ?? item.context} onInput={(event) => onAppDraft(item.app_name, event.currentTarget.value)} />
                  <div className="drawer-actions compact">
                    <button type="button" onClick={() => onSaveExistingApp(item.app_name)}>保存</button>
                    <button type="button" className="secondary danger" onClick={() => onDeleteApp(item.app_name)}>删除</button>
                  </div>
                </div>
              )) : <p className="history-empty">暂无应用上下文</p>}
            </div>
          </section>
        </div>
        {notice ? <p className="notice">{notice}</p> : null}
      </div>
    </section>
  );
}
