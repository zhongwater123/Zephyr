/**
 * Pixel energy field for the smart-polish track.
 *
 * A small per-cell simulation drawn on a 2D canvas. The handle is a nozzle:
 * energy ignites there and creeps left, and each cell keeps a brightness that
 * decays every frame, which is what produces a trailing plume rather than a
 * static gradient. Roughly 5 x 56 cells, so a frame is a few hundred
 * `fillRect` calls — cheap enough to run uncapped, and it never touches
 * component state, so nothing here can trigger a re-render.
 *
 * The cell grid is deliberate, not an artifact: quantizing the field into
 * hard-edged squares is what makes it read as pixels.
 */

export const CELL = 6;
export const CELL_INSET = 1;
export const ROWS = 5;

/** Per-level plume character. Index = PolishLevel. */
const LEVEL_ENERGY = [0, 0.42, 0.72, 1] as const;

type Cell = {
  /** stable per-cell randomness, drives delay, speed and every phase */
  h: number;
  /** second independent hash, for sparks */
  h2: number;
  /** current brightness, carried across frames — this is the trail */
  b: number;
};

function hash(n: number): number {
  const x = Math.sin(n * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
}

/**
 * Teal ramp for a LIGHT track: energy reads as more saturated and deeper, not
 * as white-hot. A white-hot core is invisible on Zephyr's paper background —
 * contrast has to increase with energy, so the hottest cells are the most
 * vivid, not the palest.
 */
const TAIL: [number, number, number] = [146, 206, 214];
const MID: [number, number, number] = [16, 176, 202];
const CORE: [number, number, number] = [6, 122, 152];

function ramp(temp: number): [number, number, number] {
  const t = Math.min(1, Math.max(0, temp));
  const a = t < 0.5 ? TAIL : MID;
  const b = t < 0.5 ? MID : CORE;
  const k = t < 0.5 ? t * 2 : (t - 0.5) * 2;
  const eased = t < 0.5 ? k : Math.pow(k, 2.4);
  return [
    a[0] + (b[0] - a[0]) * eased,
    a[1] + (b[1] - a[1]) * eased,
    a[2] + (b[2] - a[2]) * eased,
  ];
}

export type FieldOptions = {
  /** Continuous handle position, 0..1. Read every frame; may change mid-drag. */
  getPosition: () => number;
  /** Committed level 0..3, sets the plume's energy ceiling. */
  getLevel: () => number;
  /** When true, draw one static frame and stop animating. */
  getStatic: () => boolean;
};

export type FieldHandle = {
  /** Re-read the canvas size and rebuild the grid. Call on resize. */
  resize: () => void;
  /** Draw a single frame immediately (used for the static path). */
  drawOnce: () => void;
  start: () => void;
  stop: () => void;
  destroy: () => void;
};

export function createPolishField(
  canvas: HTMLCanvasElement,
  options: FieldOptions,
): FieldHandle {
  const ctx = canvas.getContext("2d");
  let cells: Cell[] = [];
  let cols = 0;
  let dpr = 1;
  let raf = 0;
  let running = false;
  let last = 0;
  let elapsed = 0;

  function build() {
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (!ctx || w <= 0 || h <= 0) {
      cols = 0;
      cells = [];
      return;
    }
    dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1));
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    cols = Math.max(1, Math.floor(w / CELL));
    cells = new Array(cols * ROWS);
    for (let c = 0; c < cols; c += 1) {
      for (let r = 0; r < ROWS; r += 1) {
        const i = c * ROWS + r;
        cells[i] = { h: hash(i), h2: hash(i + 977), b: 0 };
      }
    }
  }

  function frame(dt: number) {
    if (!ctx || cols === 0) return;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    const pos = Math.min(1, Math.max(0, options.getPosition()));
    const level = Math.min(3, Math.max(0, Math.round(options.getLevel())));
    // Energy steps per level rather than tracking the pointer continuously:
    // the field must never suggest a strength between two real levels.
    const energy = LEVEL_ENERGY[level as 0 | 1 | 2 | 3];
    const t = elapsed;

    // frame-rate independent decay: shorter memory keeps the trail readable
    // instead of accumulating into a solid slab
    const decay = Math.pow(0.86, dt * 60);

    const cellW = w / cols;
    const rowH = h / ROWS;

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    if (energy <= 0 || pos <= 0.001) {
      for (let i = 0; i < cells.length; i += 1) cells[i].b = 0;
      return;
    }

    for (let c = 0; c < cols; c += 1) {
      // cell centre in normalized track space
      const x = (c + 0.5) / cols;
      for (let r = 0; r < ROWS; r += 1) {
        const cell = cells[c * ROWS + r];
        const hh = cell.h;

        // --- ignition front creeping left from the nozzle ---
        const delay = hh * 0.55;
        const age = Math.max(0, t - delay);
        const speed = 0.85 + hh * 0.3;
        // ease-out-cubic: fast launch, settling reach
        const eased = 1 - Math.pow(1 - Math.min(1, age / 1.6), 3);
        // reach scales with energy: a short ember at 轻微整理, wall-to-wall at
        // 理清重点 (the >1 factor guarantees the top level clears the left edge
        // even for the slowest cells)
        const travel = eased * pos * speed * (0.45 + energy * 0.85);
        const front = Math.max(pos - travel - (hh - 0.5) * 0.04, 0.0);
        const tail = Math.max(pos - front, 0.001);

        let add = 0;
        // nothing is drawn ahead of the nozzle: the lit span itself is the
        // value readout, so the right side of the handle stays empty
        const behindNozzle = x <= pos + 0.002;
        const inZone = behindNozzle && x >= front - 0.004;
        if (inZone) {
          // normalized distance behind the nozzle
          const dn = Math.min(1, Math.max(0, (pos - x) / tail));

          // Coverage falls off leftward and is compared against this cell's
          // own hash: a cell is either part of the plume or blank, so the
          // plume DISSOLVES into sparser pixels toward the tail instead of
          // fading as a smooth slab. The floor grows with energy so the top
          // level still reaches the far wall.
          const floor = 0.34 * energy * energy;
          const cov = Math.pow(1 - dn, 0.9) * (1 - floor) + floor;
          if (cell.h2 >= cov * 1.08) continue;

          // vertical lens falloff, so rows form a plume not a rectangle
          const vy = Math.abs((r + 0.5) / ROWS - 0.5) * 2;
          const vf = Math.pow(Math.max(0, 1 - vy * vy * 0.45), 0.75);
          const bright = cov;

          // flicker: three sines at incommensurate spatial/temporal rates
          const f1 = Math.sin(x * 30 + t * 15 + hh * 6.28);
          const f2 = Math.sin(x * 17 + t * 8 + hh * 3.14);
          const f3 = Math.sin(x * 52 + t * 25 + hh * 10);
          const flame = smoothstep(0.08, 0.92, (f1 + f2 * 0.5 + f3 * 0.25) * 0.35 + 0.5);

          // rhythm: waves running toward the nozzle (note the minus on t)
          const r1 = Math.sin(dn * 16 - t * 5 + hh * 3);
          const r2 = Math.sin(dn * 8 - t * 2.5 + hh * 5);
          const rhythm = Math.pow(
            Math.max(0, smoothstep(-0.15, 0.55, r1) * (r2 * 0.5 + 0.5)),
            1.2,
          );

          // one-shot flash as the front sweeps past this cell
          const avgSpd = travel / Math.max(age, 0.001);
          const passed = Math.max(0, age - (pos - x) / Math.max(avgSpd, 0.001));
          const flash = Math.exp(-passed * 3.2);

          // sparks drifting down the tail
          const sp = frac(t * (0.38 + hh * 0.15) + hh * 7);
          const sX = pos - sp * tail;
          const sY = 0.5 + Math.sin(sp * 11 + hh * 6.28) * 0.28;
          const spark =
            smoothstep(0.014, 0, Math.abs(x - sX)) *
            smoothstep(0.2, 0, Math.abs((r + 0.5) / ROWS - sY)) *
            (1 - sp) * (1 - sp);

          add =
            bright * vf * (flame * 0.55 + rhythm * 0.45) +
            flash * bright * vf * 0.5 +
            spark * 0.6;
          // gain kept low so the steady state lands mid-range instead of
          // saturating every cell to the clamp (which flattens the gradient)
          add *= energy * 0.17;
        }

        if (behindNozzle) {
          // leading edge: a brighter shimmering line at the advancing front
          const edgeBase = Math.exp(-Math.pow((x - front) * 18, 2));
          if (edgeBase > 0.002) {
            const e1 = Math.sin(x * 45 + t * 20 + hh * 6.28) * 0.5 + 0.5;
            const e2 = Math.sin(x * 28 + t * 11 + hh * 3.14) * 0.5 + 0.5;
            add += edgeBase * (0.25 + e1 * e2 * 1.5) * 0.1 * energy;
          }
          // glow at the nozzle itself
          add += Math.exp(-Math.pow((x - pos) * 15, 2)) * 0.09 * energy;
        }

        // carry brightness across frames — this is the trail
        cell.b = cell.b * decay + add;
        if (cell.b < 0.004) cell.b = 0;
        if (cell.b > 1.4) cell.b = 1.4;

        if (cell.b <= 0.012) continue;

        const lum = 1 - Math.exp(-cell.b * 2.2);
        const temp = Math.min(1, cell.b * 1.05);
        const rgb = ramp(temp);
        ctx.fillStyle =
          "rgba(" +
          Math.round(rgb[0]) + "," +
          Math.round(rgb[1]) + "," +
          Math.round(rgb[2]) + "," +
          lum.toFixed(3) +
          ")";
        ctx.fillRect(
          c * cellW,
          r * rowH,
          Math.max(1, cellW - CELL_INSET),
          Math.max(1, rowH - CELL_INSET),
        );
      }
    }
  }

  function loop(now: number) {
    if (!running) return;
    const dt = last ? Math.min(0.05, (now - last) / 1000) : 0.016;
    last = now;
    elapsed += dt;
    frame(dt);
    raf = window.requestAnimationFrame(loop);
  }

  function drawOnce() {
    // A settled frame: let the simulation converge so the static render shows
    // the plume at rest rather than a single ignition frame.
    elapsed = 2.2;
    for (let i = 0; i < 30; i += 1) frame(1 / 60);
  }

  return {
    resize() {
      build();
      if (options.getStatic()) drawOnce();
    },
    drawOnce,
    start() {
      if (running) return;
      if (options.getStatic()) {
        drawOnce();
        return;
      }
      running = true;
      last = 0;
      raf = window.requestAnimationFrame(loop);
    },
    stop() {
      running = false;
      if (raf) window.cancelAnimationFrame(raf);
      raf = 0;
    },
    destroy() {
      running = false;
      if (raf) window.cancelAnimationFrame(raf);
      raf = 0;
      cells = [];
      cols = 0;
    },
  };
}

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0 || 1e-6)));
  return t * t * (3 - 2 * t);
}

function frac(x: number): number {
  return x - Math.floor(x);
}
