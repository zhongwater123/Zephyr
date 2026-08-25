import type { HistoryItem } from "../../domain";

export function historySummary(item: HistoryItem) {
  const text = item.text.replace(/\s+/g, " ").trim();
  return text.length > 56 ? `${text.slice(0, 56)}...` : text;
}

export function selectedHistoryItem(items: HistoryItem[], id: string | null) {
  return items.find((item) => item.id === id);
}
