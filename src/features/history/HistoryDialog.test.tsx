// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HistoryDialog } from "./HistoryDialog";

afterEach(cleanup);

describe("HistoryDialog", () => {
  it("renders loading and error states without inventing a successful result", () => {
    const props = {
      open: true,
      query: "",
      items: [],
      selectedId: null,
      editingText: "",
      onClose: vi.fn(),
      onRefresh: vi.fn(),
      onClear: vi.fn(),
      onQuery: vi.fn(),
      onSelect: vi.fn(),
      onEditingText: vi.fn(),
      onSave: vi.fn(),
      onCopy: vi.fn(),
      onDelete: vi.fn(),
    };
    const { rerender } = render(<HistoryDialog {...props} loading notice="" />);
    expect(screen.getByText("正在加载...")).toBeTruthy();

    rerender(<HistoryDialog {...props} loading={false} notice="数据库读取失败" />);
    expect(screen.getByText("暂无历史记录")).toBeTruthy();
    expect(screen.getByText("数据库读取失败")).toBeTruthy();
  });
});
