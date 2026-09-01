import { describe, expect, it } from "vitest";
import {
  POLISH_HYST,
  POLISH_STOPS,
  POLISH_THUMB,
  magnetize,
  positionFromClientX,
  tierFor,
} from "./polishTrackMath";

describe("positionFromClientX", () => {
  it("reports no usable position when the element has no layout box", () => {
    expect(positionFromClientX(120, { left: 0, width: 0 })).toBeNull();
    expect(positionFromClientX(120, { left: 0, width: POLISH_THUMB })).toBeNull();
  });

  it("maps the handle-inset travel to 0..1 and clamps outside it", () => {
    const rect = { left: 0, width: 340 };
    expect(positionFromClientX(POLISH_THUMB / 2, rect)).toBeCloseTo(0, 6);
    expect(positionFromClientX(340 - POLISH_THUMB / 2, rect)).toBeCloseTo(1, 6);
    expect(positionFromClientX(-500, rect)).toBe(0);
    expect(positionFromClientX(9999, rect)).toBe(1);
  });

  it("accounts for the element's left offset", () => {
    expect(positionFromClientX(100 + POLISH_THUMB / 2, { left: 100, width: 340 })).toBeCloseTo(0, 6);
  });
});

describe("magnetize", () => {
  it("leaves the four real stops exactly where they are", () => {
    for (const stop of POLISH_STOPS) {
      expect(magnetize(stop)).toBeCloseTo(stop, 6);
    }
  });

  it("stays continuous at the midpoints, so the handle never jumps", () => {
    for (const raw of [0.5, 1.5, 2.5]) {
      expect(magnetize(raw / 3)).toBeCloseTo(raw / 3, 6);
    }
  });

  it("pulls an off-stop pointer toward the nearest stop", () => {
    // a quarter step past level 1 lands noticeably closer to level 1
    expect(magnetize(1.25 / 3) * 3).toBeCloseTo(1.154, 2);
    expect(magnetize(1.75 / 3) * 3).toBeCloseTo(1.846, 2);
  });

  it("never leaves 0..1", () => {
    for (const u of [-1, 0, 0.3, 0.99, 1, 2]) {
      const out = magnetize(u);
      expect(out).toBeGreaterThanOrEqual(0);
      expect(out).toBeLessThanOrEqual(1);
    }
  });
});

describe("tierFor", () => {
  it("keeps the current level inside the hysteresis band", () => {
    // boundary between 1 and 2 is raw 1.5; leaving 1 needs 1.5 + HYST
    expect(tierFor((1.5 + POLISH_HYST - 0.01) / 3, 1)).toBe(1);
    expect(tierFor((1.5 + POLISH_HYST + 0.01) / 3, 1)).toBe(2);
    // and symmetrically coming back down from 2
    expect(tierFor((1.5 - POLISH_HYST + 0.01) / 3, 2)).toBe(2);
    expect(tierFor((1.5 - POLISH_HYST - 0.01) / 3, 2)).toBe(1);
  });

  it("resolves the four levels at their own stops", () => {
    expect(tierFor(POLISH_STOPS[0], 3)).toBe(0);
    expect(tierFor(POLISH_STOPS[1], 0)).toBe(1);
    expect(tierFor(POLISH_STOPS[2], 0)).toBe(2);
    expect(tierFor(POLISH_STOPS[3], 0)).toBe(3);
  });

  it("clamps beyond both ends", () => {
    expect(tierFor(-0.4, 2)).toBe(0);
    expect(tierFor(1.4, 1)).toBe(3);
  });
});
