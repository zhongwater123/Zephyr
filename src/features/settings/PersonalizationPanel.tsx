import { useEffect, useRef, useState } from "preact/hooks";
import type { AppHotwordContext, HistoryItem, HotwordState } from "../../domain";
import { historySummary } from "../history/model";
import { IncidentRecoveryPanel } from "../history/IncidentRecoveryPanel";
import { BehaviorSwitch } from "./BehaviorSwitch";

export type PersonalizationTab = "words" | "voice" | "apps" | "history";

function DebouncedTextarea({
  value,
  placeholder,
  ariaLabel,
  onChange,
  onCommit,
}: {
  value: string;
  placeholder: string;
  ariaLabel: string;
  onChange: (value: string) => void;
  onCommit: () => void;
}) {
  const timer = useRef<number | null>(null);
  const dirty = useRef(false);
  const commitRef = useRef(onCommit);

  function schedule(next: string) {
    dirty.current = true;
    onChange(next);
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      dirty.current = false;
      commitRef.current();
    }, 600);
  }

  useEffect(() => () => {
    if (timer.current) window.clearTimeout(timer.current);
  }, []);

  useEffect(() => {
    commitRef.current = onCommit;
  }, [onCommit]);

  return (
    <textarea
      value={value}
      aria-label={ariaLabel}
      placeholder={placeholder}
      onInput={(event) => schedule(event.currentTarget.value)}
      onBlur={() => {
        if (!dirty.current) return;
        dirty.current = false;
        if (timer.current) window.clearTimeout(timer.current);
        commitRef.current();
      }}
    />
  );
}

export function PersonalizationPanel({
  tab,
  state,
  loading,
  notice,
  newWord,
  edits,
  profileText,
  appName,
  appText,
  appEdits,
  historyEnabled,
  historySaving,
  historyError,
  historyQuery,
  historyItems,
  selectedHistoryId,
  editingHistoryText,
  historyNotice,
  historyLoading,
  onTab,
  onHistoryEnabled,
  onHotwordsEnabled,
  onOrganizerEnabled,
  onOpenServiceSettings,
  onRefreshWords,
  onOrganize,
  onNewWord,
  onAddWord,
  onEditWord,
  onUpdateWord,
  onDeleteWord,
  onPromoteAgentWord,
  onDeleteAgentWord,
  onProfileText,
  onSaveProfile,
  onAppName,
  onAppText,
  onSaveApp,
  onAppDraft,
  onSaveExistingApp,
  onDeleteApp,
  onHistoryQuery,
  onLoadHistory,
  onSelectHistory,
  onEditingHistoryText,
  onSaveHistory,
  onCopyHistory,
  onDeleteHistory,
  onClearHistory,
}: {
  tab: PersonalizationTab;
  state: HotwordState | null;
  loading: boolean;
  notice: string;
  newWord: string;
  edits: Record<string, string>;
  profileText: string;
  appName: string;
  appText: string;
  appEdits: Record<string, AppHotwordContext>;
  historyEnabled: boolean;
  historySaving: boolean;
  historyError: string;
  historyQuery: string;
  historyItems: HistoryItem[];
  selectedHistoryId: string | null;
  editingHistoryText: string;
  historyNotice: string;
  historyLoading: boolean;
  onTab: (tab: PersonalizationTab) => void;
  onHistoryEnabled: (enabled: boolean) => void;
  onHotwordsEnabled: (enabled: boolean) => void;
  onOrganizerEnabled: (enabled: boolean) => void;
  onOpenServiceSettings: () => void;
  onRefreshWords: () => void;
  onOrganize: () => void;
  onNewWord: (value: string) => void;
  onAddWord: () => void;
  onEditWord: (word: string, value: string) => void;
  onUpdateWord: (word: string) => void;
  onDeleteWord: (word: string) => void;
  onPromoteAgentWord: (word: string) => void;
  onDeleteAgentWord: (word: string) => void;
  onProfileText: (value: string) => void;
  onSaveProfile: () => void;
  onAppName: (value: string) => void;
  onAppText: (value: string) => void;
  onSaveApp: () => void;
  onAppDraft: (appName: string, context: string) => void;
  onSaveExistingApp: (appName: string) => void;
  onDeleteApp: (appName: string) => void;
  onHistoryQuery: (value: string) => void;
  onLoadHistory: () => void;
  onSelectHistory: (item: HistoryItem) => void;
  onEditingHistoryText: (value: string) => void;
  onSaveHistory: () => void;
  onCopyHistory: () => void;
  onDeleteHistory: () => void;
  onClearHistory: () => void;
}) {
  const [profileSaving, setProfileSaving] = useState(false);
  const selected = historyItems.find((item) => item.id === selectedHistoryId);

  async function saveProfileWithState() {
    setProfileSaving(true);
    try {
      await Promise.resolve(onSaveProfile());
    } finally {
      setProfileSaving(false);
    }
  }

  const tabs: Array<{ id: PersonalizationTab; label: string }> = [
    { id: "words", label: "我的词库" },
    { id: "voice", label: "表达习惯" },
    { id: "apps", label: "应用场景" },
    { id: "history", label: "历史记录" },
  ];

  return (
    <div className="personalization-layout">
      <nav className="panel-tabs" aria-label="个性化设置">
        {tabs.map((item) => (
          <button
            key={item.id}
            type="button"
            className={tab === item.id ? "is-active" : ""}
            aria-current={tab === item.id ? "page" : undefined}
            onClick={() => onTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>

      <div className="panel-content">
        {tab === "words" ? (
          <section className="panel-page" aria-labelledby="words-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="words-title">我的词库</h3>
                <p>让人名、术语和常用表达被更准确地识别。</p>
              </div>
              <button type="button" className="text-button" onClick={onRefreshWords}>刷新</button>
            </div>

            <div className="inline-settings">
              <BehaviorSwitch
                label="使用个人词库"
                description="识别时自动带入手动和整理后的词条"
                checked={state?.hotwords_enabled ?? false}
                disabled={loading}
                onChange={onHotwordsEnabled}
              />
              <BehaviorSwitch
                label="自动整理新词"
                description="每积累 20 条历史记录，智能提取可能有用的词条"
                checked={state?.hotword_agent_enabled ?? false}
                disabled={loading}
                onChange={onOrganizerEnabled}
              />
              <div className="organize-row">
                <span>
                  <strong>{state?.pending_count ?? 0} 条待整理</strong>
                  <small>{state?.updated_at ? "最近整理：" + state.updated_at : "尚未进行自动整理"}</small>
                </span>
                <button type="button" className="secondary" onClick={onOrganize} disabled={loading}>立即整理</button>
              </div>
              <p className="helper-copy">
                自动整理的服务配置可在
                <button type="button" className="inline-link" onClick={onOpenServiceSettings}>更多设置 / 智能整理服务</button>
                中管理。
              </p>
            </div>

            <div className="word-add-row">
              <input
                value={newWord}
                aria-label="添加词条"
                placeholder="输入人名、术语或常用表达"
                onInput={(event) => onNewWord(event.currentTarget.value)}
                onKeyDown={(event) => { if (event.key === "Enter") onAddWord(); }}
              />
              <button type="button" onClick={onAddWord}>添加</button>
            </div>

            <div className="word-groups">
              <section>
                <div className="subsection-heading">
                  <h4>手动添加</h4>
                  <span>{state?.manual_hotwords.length ?? 0}</span>
                </div>
                <div className="editable-list">
                  {state?.manual_hotwords.length ? state.manual_hotwords.map((word) => (
                    <div className="editable-row" key={word}>
                      <input value={edits[word] ?? word} aria-label={"编辑词条 " + word} onInput={(event) => onEditWord(word, event.currentTarget.value)} />
                      <button type="button" className="secondary" onClick={() => onUpdateWord(word)}>更新</button>
                      <button type="button" className="icon-text danger" onClick={() => onDeleteWord(word)}>删除</button>
                    </div>
                  )) : <p className="empty-state">还没有手动词条。</p>}
                </div>
              </section>

              <section>
                <div className="subsection-heading">
                  <h4>智能整理</h4>
                  <span>{state?.agent_hotwords.length ?? 0}</span>
                </div>
                <div className="organized-word-list">
                  {state?.agent_hotwords.length ? state.agent_hotwords.map((word) => (
                    <div className="organized-word" key={word}>
                      <span>{word}</span>
                      <div>
                        <button type="button" className="text-button" onClick={() => onPromoteAgentWord(word)}>转为手动</button>
                        <button type="button" className="text-button danger" onClick={() => onDeleteAgentWord(word)}>删除</button>
                      </div>
                    </div>
                  )) : <p className="empty-state">整理后出现的新词会显示在这里。</p>}
                </div>
              </section>
            </div>
            {notice ? <p className="inline-notice" role="status">{notice}</p> : null}
          </section>
        ) : null}

        {tab === "voice" ? (
          <section className="panel-page" aria-labelledby="voice-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="voice-title">表达习惯</h3>
                <p>告诉 Zephyr 你的工作内容、常用说法和偏好的表达方式。</p>
              </div>
              <span className="save-state">{profileSaving ? "保存中…" : ""}</span>
            </div>
            <label className="large-field">
              <span>关于我</span>
              <small>例如：我从事软件开发，经常讨论 Rust、ASR 和桌面端交互。</small>
              <DebouncedTextarea
                value={profileText}
                ariaLabel="个人表达习惯"
                placeholder="写下有助于理解你表达的背景…"
                onChange={onProfileText}
                onCommit={() => void saveProfileWithState()}
              />
            </label>
            <p className="helper-copy">停止输入 600ms 后自动保存。</p>
            {notice ? <p className="inline-notice" role="status">{notice}</p> : null}
          </section>
        ) : null}

        {tab === "apps" ? (
          <section className="panel-page" aria-labelledby="apps-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="apps-title">应用场景</h3>
                <p>按应用补充语境，让同一句话在不同工作场景中更准确。</p>
              </div>
            </div>
            <div className="app-context-create">
              <label>
                <span>应用名称</span>
                <input value={appName} placeholder="例如 Code.exe" onInput={(event) => onAppName(event.currentTarget.value)} />
              </label>
              <label>
                <span>场景说明</span>
                <input value={appText} placeholder="例如：编写技术方案和代码" onInput={(event) => onAppText(event.currentTarget.value)} />
              </label>
              <button type="button" onClick={onSaveApp}>添加场景</button>
            </div>
            <div className="context-card-list">
              {state?.app_contexts.length ? state.app_contexts.map((item) => (
                <article className="context-card" key={item.app_name}>
                  <div className="subsection-heading">
                    <h4>{item.app_name}</h4>
                    <button type="button" className="text-button danger" onClick={() => onDeleteApp(item.app_name)}>删除</button>
                  </div>
                  <DebouncedTextarea
                    value={appEdits[item.app_name]?.context ?? item.context}
                    ariaLabel={item.app_name + " 场景说明"}
                    placeholder="描述在这个应用中的常见输入内容"
                    onChange={(value) => onAppDraft(item.app_name, value)}
                    onCommit={() => onSaveExistingApp(item.app_name)}
                  />
                </article>
              )) : <p className="empty-state">还没有应用场景。</p>}
            </div>
            {notice ? <p className="inline-notice" role="status">{notice}</p> : null}
          </section>
        ) : null}

        {tab === "history" ? (
          <section className="panel-page history-page" aria-labelledby="history-title">
            <div className="panel-page-heading">
              <div>
                <h3 id="history-title">历史记录</h3>
                <p>查找、复用或修正曾经输入的内容。</p>
              </div>
              <BehaviorSwitch
                label="记录输入历史"
                description="关闭后，新的语音输入不会保存到本地历史"
                checked={historyEnabled}
                disabled={historySaving}
                onChange={onHistoryEnabled}
              />
            </div>
            {historyError ? <p className="field-error" role="alert">{historyError}</p> : null}
            <IncidentRecoveryPanel />
            <div className="history-toolbar">
              <input
                type="search"
                value={historyQuery}
                aria-label="搜索历史记录"
                placeholder="搜索文本、应用或窗口标题"
                onInput={(event) => onHistoryQuery(event.currentTarget.value)}
                onKeyDown={(event) => { if (event.key === "Enter") onLoadHistory(); }}
              />
              <button type="button" className="secondary" onClick={onLoadHistory}>搜索</button>
              <button type="button" className="text-button danger" onClick={onClearHistory}>清空</button>
            </div>
            <div className="embedded-history">
              <div className="embedded-history-list" aria-busy={historyLoading}>
                {historyItems.length ? historyItems.map((item) => (
                  <button
                    type="button"
                    key={item.id}
                    className={item.id === selectedHistoryId ? "is-selected" : ""}
                    onClick={() => onSelectHistory(item)}
                  >
                    <span>{item.created_at}</span>
                    <strong>{item.app_name || "未知应用"}</strong>
                    <small>{historySummary(item)}</small>
                  </button>
                )) : <p className="empty-state">{historyLoading ? "正在加载…" : "暂无历史记录"}</p>}
              </div>
              <div className="embedded-history-detail">
                {selected ? (
                  <>
                    <textarea value={editingHistoryText} aria-label="历史记录正文" onInput={(event) => onEditingHistoryText(event.currentTarget.value)} />
                    <small>{selected.app_title || "无窗口标题"} · {selected.char_count} 字</small>
                    <div className="button-row">
                      <button type="button" onClick={onSaveHistory}>保存修改</button>
                      <button type="button" className="secondary" onClick={onCopyHistory}>复制</button>
                      <button type="button" className="text-button danger" onClick={onDeleteHistory}>删除</button>
                    </div>
                  </>
                ) : <p className="empty-state">选择一条记录查看全文。</p>}
              </div>
            </div>
            {historyNotice ? <p className="inline-notice" role="status">{historyNotice}</p> : null}
          </section>
        ) : null}
      </div>
    </div>
  );
}
