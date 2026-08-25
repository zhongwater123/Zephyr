import { useState } from "preact/hooks";
import type { HistoryItem } from "../../domain";
import { historyApi } from "../../ipc/client";

export function useHistoryController() {
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyQuery, setHistoryQuery] = useState("");
  const [historyItems, setHistoryItems] = useState<HistoryItem[]>([]);
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null);
  const [editingHistoryText, setEditingHistoryText] = useState("");
  const [historyNotice, setHistoryNotice] = useState("");
  const [historyLoading, setHistoryLoading] = useState(false);

  function selectHistoryItem(item: HistoryItem) {
    setSelectedHistoryId(item.id);
    setEditingHistoryText(item.text);
    setHistoryNotice("");
  }

  async function loadHistory(query = historyQuery) {
    setHistoryLoading(true);
    setHistoryNotice("");
    try {
      const items = await historyApi.list({ query: query || null, limit: 50, offset: 0 });
      setHistoryItems(items);
      if (items.length === 0) {
        setSelectedHistoryId(null);
        setEditingHistoryText("");
      } else if (!items.some((item) => item.id === selectedHistoryId)) {
        selectHistoryItem(items[0]);
      }
    } catch (error) {
      setHistoryNotice(String(error));
    } finally {
      setHistoryLoading(false);
    }
  }

  async function openHistoryPanel() {
    setHistoryOpen(true);
    setHistoryQuery("");
    await loadHistory("");
  }

  async function saveHistoryItem() {
    if (!selectedHistoryId) return;
    setHistoryNotice("");
    try {
      await historyApi.update(selectedHistoryId, editingHistoryText);
      setHistoryNotice("历史记录已更新。");
      await loadHistory();
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  async function copyHistoryItem() {
    if (!selectedHistoryId) return;
    setHistoryNotice("");
    try {
      await historyApi.copy(selectedHistoryId);
      setHistoryNotice("已复制到剪贴板。");
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  async function deleteHistoryItem() {
    if (!selectedHistoryId) return;
    setHistoryNotice("");
    try {
      await historyApi.delete(selectedHistoryId);
      setHistoryNotice("历史记录已删除。");
      setSelectedHistoryId(null);
      setEditingHistoryText("");
      await loadHistory();
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  async function clearAllHistory() {
    if (!window.confirm("确认清空全部历史记录？此操作不可撤销。")) return;
    setHistoryNotice("");
    try {
      await historyApi.clear();
      setHistoryItems([]);
      setSelectedHistoryId(null);
      setEditingHistoryText("");
      setHistoryNotice("历史记录已清空。");
    } catch (error) {
      setHistoryNotice(String(error));
    }
  }

  return {
    historyOpen,
    setHistoryOpen,
    historyQuery,
    setHistoryQuery,
    historyItems,
    selectedHistoryId,
    editingHistoryText,
    setEditingHistoryText,
    historyNotice,
    historyLoading,
    openHistoryPanel,
    loadHistory,
    selectHistoryItem,
    saveHistoryItem,
    copyHistoryItem,
    deleteHistoryItem,
    clearAllHistory,
  };
}
