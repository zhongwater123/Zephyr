// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AsrOptionPool } from "../../domain";
import { OptionPoolRenderer } from "./OptionPoolRenderer";

const pool: AsrOptionPool = {
  providerId: "volcengine",
  providerDisplayName: "火山引擎",
  schemaVersion: 1,
  revision: 3,
  options: [
    {
      id: "punctuation",
      controlKind: "toggle",
      label: "自动标点",
      description: "自动补全标点",
      defaultValue: { type: "boolean", value: true },
      group: "recognition_behavior",
      order: 0,
      enabled: true,
      disabledReason: null,
    },
  ],
  values: { punctuation: { type: "boolean", value: false } },
};

afterEach(cleanup);

describe("OptionPoolRenderer", () => {
  it("renders specs without knowing provider wire names", () => {
    const onChange = vi.fn();
    render(<OptionPoolRenderer pool={pool} saving={false} onChange={onChange} />);
    const toggle = screen.getByRole("checkbox");
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.click(toggle);
    expect(onChange).toHaveBeenCalledWith("punctuation", {
      type: "boolean",
      value: true,
    });
    expect(screen.queryByText("enable_punc")).toBeNull();
  });

  it("fails closed for unknown control kinds", () => {
    render(
      <OptionPoolRenderer
        pool={{
          ...pool,
          options: [{ ...pool.options[0], controlKind: "unknown" as "toggle" }],
        }}
        saving={false}
        onChange={vi.fn()}
      />,
    );
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByText(/不支持的控件/)).not.toBeNull();
  });
});
