import type { HistoryItem } from "../../domain";
import { historySummary, selectedHistoryItem } from "./model";

export function HistoryDialog({
  open,
  query,
  items,
  selectedId,
  editingText,
  notice,
  loading,
  onClose,
  onRefresh,
  onClear,
  onQuery,
  onSelect,
  onEditingText,
  onSave,
  onCopy,
  onDelete,
}: {
  open: boolean;
  query: string;
  items: HistoryItem[];
  selectedId: string | null;
  editingText: string;
  notice: string;
  loading: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onClear: () => void;
  onQuery: (query: string) => void;
  onSelect: (item: HistoryItem) => void;
  onEditingText: (text: string) => void;
  onSave: () => void;
  onCopy: () => void;
  onDelete: () => void;
}) {
  if (!open) return null;
  return (
    <section className="history-backdrop" onClick={onClose}>
      <div className="history-card" role="dialog" aria-label="历史记录" onClick={(event) => event.stopPropagation()}>
        <header className="history-header">
          <div><p className="drawer-kicker">Zephyr / 历史记录</p><h2>输入历史</h2></div>
          <div className="history-header-actions">
            <button type="button" className="secondary" onClick={onRefresh}>刷新</button>
            <button type="button" className="secondary danger" onClick={onClear}>清空</button>
            <button type="button" className="drawer-close" onClick={onClose}>关闭</button>
          </div>
        </header>
        <div className="history-search">
          <input value={query} placeholder="搜索文本、应用或窗口标题" onInput={(event) => onQuery(event.currentTarget.value)} />
        </div>
        <div className="history-layout">
          <div className="history-list" aria-busy={loading}>
            {items.length === 0 ? <p className="history-empty">{loading ? "正在加载..." : "暂无历史记录"}</p> : items.map((item) => (
              <button type="button" key={item.id} className={`history-item ${item.id === selectedId ? "selected" : ""}`} onClick={() => onSelect(item)}>
                <span className="history-time">{item.created_at}</span><strong>{item.app_name || "未知应用"}</strong>
                <span>{historySummary(item)}</span><small>{item.char_count} 字</small>
              </button>
            ))}
          </div>
          <div className="history-detail">
            {selectedId ? <>
              <textarea value={editingText} onInput={(event) => onEditingText(event.currentTarget.value)} />
              <div className="history-detail-meta">{selectedHistoryItem(items, selectedId)?.app_title || "无窗口标题"}</div>
              <div className="drawer-actions">
                <button type="button" onClick={onSave}>保存修改</button>
                <button type="button" className="secondary" onClick={onCopy}>复制</button>
                <button type="button" className="secondary danger" onClick={onDelete}>删除</button>
              </div>
            </> : <p className="history-empty">选择一条历史记录查看全文</p>}
          </div>
        </div>
        {notice ? <p className="notice">{notice}</p> : null}
      </div>
    </section>
  );
}
