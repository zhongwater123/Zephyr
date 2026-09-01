import type { PolishLevel } from "../../domain";

/** Normalized track positions of the four real levels. */
export const POLISH_STOPS = [0, 1 / 3, 2 / 3, 1] as const;

/** Handle width in px; the handle's travel is inset by it so it never overhangs. */
export const POLISH_THUMB = 20;

/**
 * Hysteresis half-band, in level units, applied when deciding which level a
 * continuous position belongs to. Without it a pointer resting near a boundary
 * makes the name, description and field flip back and forth.
 */
export const POLISH_HYST = 0.14;

/**
 * Pointer x -> normalized position in [0, 1], or null when the element has no
 * usable layout box yet (happy-dom reports a zero rect, so callers must treat
 * null as "ignore this gesture" rather than clamping to 0).
 */
export function positionFromClientX(
  clientX: number,
  rect: { left: number; width: number },
): number | null {
  const usable = rect.width - POLISH_THUMB;
  if (!(usable > 0)) return null;
  const raw = (clientX - rect.left - POLISH_THUMB / 2) / usable;
  return Math.min(1, Math.max(0, raw));
}

/**
 * Bend a raw pointer position toward the nearest stop so the handle feels
 * magnetic while dragging. Identity at the four stops and exactly continuous
 * at the midpoints, so there is never a visible jump — it only redistributes
 * where the handle lingers.
 */
export function magnetize(u: number): number {
  const raw = u * 3;
  const nearest = Math.min(3, Math.max(0, Math.round(raw)));
  const d = raw - nearest;
  const shaped = Math.sign(d) * 0.5 * Math.pow(Math.abs(d) * 2, 1.7);
  return Math.min(1, Math.max(0, (nearest + shaped) / 3));
}

/**
 * Which level a continuous position reads as, given the level it is currently
 * showing. Leaving the current level requires crossing its boundary by
 * POLISH_HYST, which is what stops boundary chatter.
 */
export function tierFor(u: number, current: PolishLevel): PolishLevel {
  const raw = u * 3;
  const nearest = Math.min(3, Math.max(0, Math.round(raw))) as PolishLevel;
  if (nearest === current) return current;
  const boundary = (current + nearest) / 2;
  if (nearest > current) return raw >= boundary + POLISH_HYST ? nearest : current;
  return raw <= boundary - POLISH_HYST ? nearest : current;
}
