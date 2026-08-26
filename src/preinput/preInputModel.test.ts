import { describe, expect, it } from "vitest";
import { getPreInputStateLabel, getPreInputTextSegments } from "./preInputModel";

describe("getPreInputTextSegments", () => {
  it("keeps confirmed and pending text separated", () => {
    expect(getPreInputTextSegments("已经确认等待识别", 4)).toEqual({
      hiddenPrefix: false,
      confirmedText: "已经确认",
      pendingText: "等待识别",
    });
  });

  it("keeps the newest characters and adjusts the confirmed boundary", () => {
    expect(getPreInputTextSegments("一二三四五六七八", 6, 4)).toEqual({
      hiddenPrefix: true,
      confirmedText: "五六",
      pendingText: "七八",
    });
  });

  it("counts Unicode code points instead of UTF-16 units", () => {
    expect(getPreInputTextSegments("A🌹BC", 2, 3)).toEqual({
      hiddenPrefix: true,
      confirmedText: "🌹",
      pendingText: "BC",
    });
  });
});

describe("getPreInputStateLabel", () => {
  it("provides one label for every visual state", () => {
    expect(getPreInputStateLabel("recording")).toBe("正在聆听");
    expect(getPreInputStateLabel("transcribing")).toBe("正在识别");
    expect(getPreInputStateLabel("finalizing")).toBe("正在写入");
    expect(getPreInputStateLabel("dismissing")).toBe("正在收起");
    expect(getPreInputStateLabel("error")).toBe("失败");
  });
});
