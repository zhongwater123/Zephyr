/**
 * Pixel energy field for the smart-polish track.
 *
 * This is a faithful canvas-2D port of the pipeline used by open-source
 * "effort slider" implementations of this effect (BSD-3-Clause), which run it
 * as a WebGL fragment shader. The constants and formulas below are the
 * reference ones; the only deliberate substitution is the palette, which uses
 * Zephyr's teal instead of the reference product's brand colour.
 *
 * The parts that actually make it read as a lit energy field, and that a
 * hand-rolled version tends to miss:
 *
 *   - energy drives ALPHA over a ramp that never darkens; the reference
 *     multiplies colour by brightness because it renders additively onto a
 *     dark bed, which on a light track turns dim cells into black squares
 *   - a soft blurred pass sits under the crisp cells as a halo
 *   - one non-linear `intensity` gain scales the whole field
 *   - the nozzle stays glued to the handle; the growth comes from the
 *     per-cell ignition creep, not from lagging the nozzle
 *   - the reveal (nothing right of the handle) is a CSS mask, not shader work
 *
 * The loop never touches component state, so it cannot trigger a re-render.
 */

/**
 * Coarser than the reference's 72x6: at this track width bigger cells read
 * better as pixels, and five rows keeps the plume from looking like a solid
 * band.
 */
/** How far Fast's rightward plume reaches, as a fraction of the track. */
const FAST_REACH = 0.33;

export const COLS = 50;
export const ROWS = 5;

type Cell = {
  /** stable per-cell randomness; drives delay, speed and every phase */
  h: number;
  /** second independent hash, for lead sparks */
  h2: number;
  /** current brightness, carried across frames — this is the trail */
  b: number;
};

function hash(n: number): number {
  const x = Math.sin(n * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
}

/**
 * Three temperature stops, in 0-255 RGB, for a LIGHT track.
 *
 * Heat is carried by SATURATION, with lightness held near the middle. The
 * reference's own gradient works the same way: its hot end is a vivid
 * mid-tone, not white and not dark. Both extremes are wrong on a light bed —
 * a dark hot end looks dirty, a near-white one washes into a pale band. The
 * tail sits close to the track's own grey so it vanishes into it, and heat
 * shows up as rising chroma (plus rising density, handled at the dissolve).
 *
 * Each stop has a calm and a vivid variant; the level interpolates between
 * them, so a higher level is genuinely more colourful at the same brightness.
 */
const COOL_CALM: [number, number, number] = [210, 214, 210];
const COOL_VIVID: [number, number, number] = [204, 218, 215];
const MID_CALM: [number, number, number] = [150, 186, 184];
const MID_VIVID: [number, number, number] = [116, 212, 208];
const HOT_CALM: [number, number, number] = [104, 156, 158];
const HOT_VIVID: [number, number, number] = [28, 188, 186];

/*
 * Fast's own ramp: a cool steel blue rather than the teal of the LLM levels.
 * Same light-bed discipline — the cool end sits near the track's grey and heat
 * shows up as chroma, never as darkness — but a distinctly different hue,
 * because Fast is a different code path, not a weaker setting.
 */
const FAST_COOL: [number, number, number] = [208, 214, 222];
const FAST_MID: [number, number, number] = [126, 162, 214];
const FAST_HOT: [number, number, number] = [56, 118, 208];

function fastRamp(temp: number): [number, number, number] {
  const t = Math.min(1, Math.max(0, temp));
  return t < 0.28
    ? mix3(FAST_COOL, FAST_MID, t / 0.28)
    : mix3(FAST_MID, FAST_HOT, (t - 0.28) / 0.72);
}

/*
 * Per-cell colour used to be built from scratch every frame: three array
 * allocations inside ramp(), a toFixed() string, a concatenation, and a CSS
 * colour parse — ~250 times a frame. That was the bulk of the main-thread
 * cost, and starving pointermove is what made the handle feel behind the
 * pointer.
 *
 * Colour is a pure function of (temp, alpha, level), and level only ever takes
 * four values, so the whole space is quantised into a table of ready-made
 * strings built once per level. Reusing the same string instances also lets
 * the engine reuse its parsed colour.
 */
const TEMP_STEPS = 32;
const ALPHA_STEPS = 24;
const colourTables = new Map<number, string[]>();

function colourTable(level: number): string[] {
  const cached = colourTables.get(level);
  if (cached) return cached;
  const energy = level / 3;
  const table = new Array<string>(TEMP_STEPS * ALPHA_STEPS);
  for (let t = 0; t < TEMP_STEPS; t += 1) {
    const temp = t / (TEMP_STEPS - 1);
    const rgb = level === 0 ? fastRamp(temp) : ramp(temp, energy);
    const r = Math.round(rgb[0]);
    const g = Math.round(rgb[1]);
    const b = Math.round(rgb[2]);
    for (let a = 0; a < ALPHA_STEPS; a += 1) {
      table[t * ALPHA_STEPS + a] =
        "rgba(" + r + "," + g + "," + b + "," + (a / (ALPHA_STEPS - 1)).toFixed(3) + ")";
    }
  }
  colourTables.set(level, table);
  return table;
}

function ramp(temp: number, energy: number): [number, number, number] {
  const t = Math.min(1, Math.max(0, temp));
  const cool = mix3(COOL_CALM, COOL_VIVID, energy);
  const mid = mix3(MID_CALM, MID_VIVID, energy);
  const hot = mix3(HOT_CALM, HOT_VIVID, energy);
  // Chroma arrives early: holding the cool end until past halfway leaves the
  // middle of the track looking like a washed-out dead zone.
  const base = t < 0.28 ? mix3(cool, mid, t / 0.28) : mix3(mid, hot, (t - 0.28) / 0.72);
  return base;
}

function mix3(
  a: [number, number, number],
  b: [number, number, number],
  k: number,
): [number, number, number] {
  return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
}

export type FieldOptions = {
  /** Continuous handle position, 0..1. Read every frame; may change mid-drag. */
  getPosition: () => number;
  /** True while the user is dragging; the halo pass is skipped to stay smooth. */
  getBusy?: () => boolean;
  /** Committed level 0..3. */
  getLevel: () => number;
  /** When true, draw one settled frame and stop animating. */
  getStatic: () => boolean;
};

export type FieldHandle = {
  resize: () => void;
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
  // Offscreen scene buffer: the bloom pass redraws the same pixels blurred
  // and additive, which needs a source image.
  let scene: HTMLCanvasElement | null = null;
  let sceneCtx: CanvasRenderingContext2D | null = null;

  let cells: Cell[] = [];
  let dpr = 1;
  let raf = 0;
  let running = false;
  let last = 0;
  let elapsed = 0;
  let lastLevel = -1;

  function build() {
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (!ctx || w <= 0 || h <= 0) {
      cells = [];
      return;
    }
    dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1));
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);

    if (!scene) {
      scene = document.createElement("canvas");
      sceneCtx = scene.getContext("2d");
    }
    if (scene) {
      scene.width = canvas.width;
      scene.height = canvas.height;
    }

    cells = new Array(COLS * ROWS);
    for (let c = 0; c < COLS; c += 1) {
      for (let r = 0; r < ROWS; r += 1) {
        const i = c * ROWS + r;
        cells[i] = {
          h: hash(i),
          h2: hash(i + 977),
          b: 0,
        };
      }
    }
  }

  function frame(dt: number) {
    if (!ctx || cells.length === 0 || !sceneCtx || !scene) return;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    /*
     * The nozzle is glued to the handle. The reference springs this value,
     * but it also DRAWS its handle at the sprung value; here the handle is
     * positioned by CSS from the raw value, so springing only the canvas
     * desynced the two — it looked like the pixels were chasing the handle
     * and only igniting once they caught up. The sense of the plume growing
     * comes from the ignition creep below, not from lagging the nozzle.
     */
    const slider = Math.min(1, Math.max(0, options.getPosition()));

    // Everything that expresses "how strong is this level" is driven by the
    // DISCRETE level, so it steps at the stops instead of tracking the pointer.
    const level = Math.min(3, Math.max(0, Math.round(options.getLevel())));
    const levelEnergy = level / 3;

    /*
     * Fast is not "level 0 of a continuum" — the Dossier is explicit that it is
     * a different MODE (ASR straight through, no LLM). So it gets its own
     * visual language: the plume fires RIGHTWARD out of the handle in a cool
     * blue, short and quick, expressing speed rather than strength. Keeping it
     * brief matters: the lowest level must never read as the one with the most
     * lit track.
     *
     * The simulation already works in `dn` space (normalised distance from the
     * nozzle), so direction is just a sign plus which wall bounds the travel.
     */
    const rightward = level === 0;
    const dirSign = rightward ? 1 : -1;
    const span = rightward ? 1 - slider : slider;
    const reach = rightward ? Math.min(span, FAST_REACH) : span;
    const haloK = rightward ? 11 : 3.5;

    /*
     * Raising the level re-ignites: the front collapses back to the handle and
     * sweeps out again, so the plume visibly fires FROM the handle rather than
     * just rescaling a plume that was already settled.
     *
     * The old plume's per-cell brightness has to be collapsed with it. Left
     * alone it kept decaying in place (0.9^frame, ~330ms from full), so the
     * previous level's tail lingered exactly where it was while the new front
     * swept out — two separate things moving at once, which is precisely what
     * pulls the eye in two directions. Damping hard instead of zeroing avoids
     * a blank frame: the residue is gone within a few frames, under the new
     * front rather than beside it.
     */
    if (level > lastLevel) {
      elapsed = 0;
      if (lastLevel >= 0) {
        for (let i = 0; i < cells.length; i += 1) cells[i].b *= 0.18;
      }
    }
    lastLevel = level;

    // Ramp-in envelopes, so the field grows into place instead of popping.
    const es = 0.15 + (0.5 - 0.15) * Math.min(elapsed / 1, 1);
    const rampIn = 0.85 + (1 - 0.85) * Math.min(elapsed / 1.5, 1);
    // Animation rate rides the level. Floors are deliberately generous: a
    // level that is BOTH sparse and slow reads as broken rather than calm,
    // and 自然表达 (the default) sits in the middle, so the middle has to look
    // finished. Separation lives mostly in speed and chroma; density and
    // intensity keep a high floor.
    const ts = rampIn * (rightward ? 2.6 : 0.95 + levelEnergy * 1.15);
    /*
     * Single gain for the whole field. The exponent is well above the
     * reference's 0.55 on purpose: with only four stops a gentle curve leaves
     * the levels looking nearly identical once alpha saturation is applied.
     */
    const intensity = rightward
      ? // the standard gain multiplies by smoothstep(0, 0.2, slider), which is
        // zero when the handle sits at the far left — Fast needs a fixed, and
        // deliberately modest, gain of its own. Kept low on purpose: Fast must
        // stay visibly LIGHTER than 轻微整理, or the lowest level reads as the
        // heaviest one.
        0.4
      : smoothstep(0, 0.2, slider) * (0.42 + 0.58 * Math.pow(levelEnergy, 1.1));
    // Fraction of cells that light up at all, so a higher level is visibly
    // MORE pixels and not just slightly brighter ones.
    const coverage = rightward ? 0.78 : 0.55 + 0.45 * levelEnergy;
    /*
     * How fast the plume fills out after it ignites. The reference hard-codes
     * a 2.5s sweep and a `h * 1.2` per-cell stagger; both are made
     * level-driven here, because "how quickly the pixels appear" is one of
     * the main things that should separate the levels.
     */
    /*
     * Both of these are derived from levelEnergy, which is 0 for Fast — so Fast
     * was getting the slowest sweep (1.15s) and the largest per-cell stagger
     * (0.75) of any level, i.e. the exact opposite of what it should feel like.
     * Fast gets its own pair: a near-instant sweep and almost no stagger, so
     * the jet appears all at once instead of crawling outward.
     */
    const spread = rightward ? 0.26 : 1.15 - levelEnergy * 0.6;
    const delayScale = rightward ? 0.14 : 0.75 - levelEnergy * 0.45;
    const t = elapsed;


    /*
     * The halo needs a source image to blur, so it needs the offscreen buffer.
     * Without it the offscreen is pure overhead — an extra full-canvas
     * drawImage every frame — so cells go straight to the visible canvas
     * whenever the halo is skipped (which is exactly when the frame budget
     * matters: during a gesture).
     */
    const wantHalo = typeof ctx.filter === "string" && !options.getBusy?.();
    const table = colourTable(level);

    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    // Clip to the handle: cells are gated by their CENTRE, so the last lit
    // cell's right edge, and the halo's spread, can bleed a few px past it.
    ctx.save();
    ctx.beginPath();
    if (rightward) {
      ctx.rect(slider * w, 0, Math.max(0, w - slider * w), h);
    } else {
      ctx.rect(0, 0, Math.max(0, slider * w), h);
    }
    ctx.clip();

    if (wantHalo) {
      sceneCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
      sceneCtx.clearRect(0, 0, w, h);
    }
    const target = wantHalo ? sceneCtx : ctx;

    if ((!rightward && slider <= 0.001) || intensity <= 0.0001 || reach <= 0.001) {
      for (let i = 0; i < cells.length; i += 1) cells[i].b = 0;
      ctx.restore();
      return;
    }

    const cellW = w / COLS;
    const rowH = h / ROWS;
    const gap = Math.min(1, Math.max(0.4, cellW * 0.18));

    for (let c = 0; c < COLS; c += 1) {
      const x = (c + 0.5) / COLS;
      /*
       * Per-frame decay. The reference kills the leftmost stretch hard
       * (`prev * 0.90 * smoothstep(0, 0.4, x)`), which on this track leaves
       * the left 40% with no trail at all — only raw per-frame flicker, and a
       * visible seam where the mask reaches 1. Keep the idea, lose the cliff:
       * the far left still fades faster, but never below a usable trail.
       */
      const fadeMask = 0.94 + 0.06 * smoothstep(0, 0.22, rightward ? 1 - x : x);
      const decay = Math.pow(0.9 * fadeMask, dt * 60);

      for (let r = 0; r < ROWS; r += 1) {
        const cell = cells[c * ROWS + r];
        const hh = cell.h;
        const y = (r + 0.5) / ROWS;

        const cellDelay = hh * delayScale;
        const cellAge = Math.max(0, t - cellDelay);
        const ignited = cellAge > 0.001 ? 1 : 0;
        const cellSpd = 0.85 + hh * 0.3;
        const eased = 1 - Math.pow(1 - Math.min(1, cellAge / spread), 3);
        const dist = eased * reach * cellSpd * ignited;
        const cellOff = (hh - 0.5) * 0.05;
        // `front` is the far edge of the plume, `tail` its length; `behind` is
        // this cell's distance from the nozzle measured along the direction of
        // travel, so everything downstream stays sign-agnostic.
        const front = rightward
          ? Math.min(slider + dist + cellOff, 0.98)
          : Math.max(slider - dist - cellOff, 0.02);
        const tail = Math.max(Math.abs(front - slider), 0.001);
        const behind = (x - slider) * dirSign;
        const dn = Math.min(1, Math.max(0, behind / tail));

        /*
         * Per-cell dissolve, with density RISING toward the nozzle and
         * FLOWING leftward over time.
         *
         * The spatial part alone (hash vs. a position-only threshold) fixes
         * which cells are lit forever, so the field freezes into a still
         * pattern — that is what made it look static. Feeding a travelling
         * wave into the threshold makes cells wink in and out in bands that
         * march away from the nozzle, which is where the sense of flow comes
         * from. The hash stays stable, so it is the WAVE that moves, not a
         * per-frame reshuffle.
         */
        const wave = 0.5 + 0.5 * Math.sin(dn * 13 - t * 6.8 * ts + hh * 6.28);
        // Sharpened, so cells pop on and off rather than easing through the
        // threshold — a soft wave only makes them breathe.
        const flow = smoothstep(0.28, 0.72, wave);
        // Modulation depth rides the level: the top level swings far enough
        // below the threshold to switch cells fully off, which is what makes
        // the blinking read as blinking rather than as a brightness ripple.
        const depth = 0.65 + 0.75 * levelEnergy;
        const density =
          coverage *
          (0.42 + 0.58 * (1 - dn)) *
          (Math.max(0, 1 - depth * 0.6) + depth * flow);
        const inZ =
          behind >= -0.003 && behind <= tail + 0.003 && cell.h2 < density ? 1 : 0;

        // No brightness floor: the reference keeps a 0.04 ember alive all the
        // way down the tail, which on this track leaves the far left
        // permanently lit and twinkling. Here the tail genuinely reaches zero.
        let bright = Math.pow(1 - dn, 0.4) * inZ;
        bright *= 1 - smoothstep(0.9, 1.02, dn);

        const vy = Math.abs(y - 0.5) * 2;
        const vf = Math.pow(Math.max(0, 1 - vy * vy * 0.45), 0.75);

        const f1 = Math.sin(x * 30 + t * 15 * ts + hh * 6.28);
        const f2 = Math.sin(x * 17 + t * 8 * ts + hh * 3.14);
        const f3 = Math.sin(x * 52 + t * 25 * ts + hh * 10);
        const flame = smoothstep(0.08, 0.92, (f1 + f2 * 0.5 + f3 * 0.25) * 0.35 + 0.5);

        const r1 = Math.sin(dn * 16 - t * 6.2 * ts + hh * 3);
        const r2 = Math.sin(dn * 8 - t * 3.2 * ts + hh * 5);
        const rhythm = Math.pow(
          Math.max(0, smoothstep(-0.15, 0.55, r1) * (r2 * 0.5 + 0.5)),
          1.2,
        );

        const avgSpd = dist / Math.max(cellAge, 0.001);
        const passedAge = Math.max(
          0,
          cellAge - Math.max(behind, 0) / Math.max(avgSpd, 0.001),
        );
        const flash = Math.exp(-passedAge * 3.2);

        const sp = frac(t * (0.38 + hh * 0.15) + hh * 7);
        const sX = slider + dirSign * sp * tail;
        const sY = 0.5 + Math.sin(sp * 11 + hh * 6.28) * 0.28;
        const spark =
          smoothstep(0.014, 0, Math.abs(x - sX)) *
          smoothstep(0.18, 0, Math.abs(y - sY)) *
          (1 - sp) * (1 - sp) * es;

        /*
         * Shimmer runs across the WHOLE plume — that liveliness is the point.
         * It is safe to leave it unbounded now because `bright` genuinely
         * decays to zero at the tail, so the modulation fades out with it
         * rather than animating a permanently-lit floor. The small constant
         * keeps cells from blinking fully out between beats.
         */
        /*
         * Green ejecta: hot cells launched at the handle that travel LEFT and
         * burn out well before the plume's own tail ends. Each cell runs its
         * own 0..1 lifecycle, so they read as discrete pixels being thrown
         * out rather than a static gradient near the nozzle.
         */
        let cellEnergy =
          bright * vf * (0.22 + flame * 0.42 + rhythm * 0.38) +
          flash * bright * vf * 0.55 +
          spark * 0.7 * inZ;
        cellEnergy *= es * intensity;

        /*
         * The leading edge and the sparks ahead of it belong to a front that
         * is still ADVANCING. Once `eased` reaches 1 the front parks at the
         * left wall, and leaving these on would pin a permanently flickering
         * line there. Gate both on how much travel is left.
         */
        const advancing = Math.max(0, 1 - eased);

        const edgeBase = Math.exp(-Math.pow((x - front) * 18, 2));
        const ef1 = Math.sin(x * 45 + t * 20 * ts + hh * 6.28) * 0.5 + 0.5;
        const ef2 = Math.sin(x * 28 + t * 11 * ts + hh * 3.14) * 0.5 + 0.5;
        const edge =
          edgeBase * (0.25 + ef1 * ef2 * 1.5) * 1.6 * intensity * es * advancing;

        const leadD = (front - x) * dirSign;
        const leadZone = smoothstep(0.07, 0, leadD) * (leadD >= 0 ? 1 : 0) * vf;
        const leadF = Math.sin(leadD * 100 + t * 20 * ts + cell.h2 * 6.28) * 0.5 + 0.5;
        const leadSpark =
          leadZone * (cell.h2 >= 0.6 ? 1 : 0) * leadF * intensity * es * 0.5 * advancing;

        const total = cellEnergy + edge + leadSpark;

        // core glow at the handle, with a slow breathing pulse
        const pulse = Math.sin(t * 2.8) * 0.15 + 1;
        const core = Math.exp(-Math.pow((x - slider) * 16, 2));
        // The nozzle halo has a fixed width, which for a SHORT plume becomes
        // the whole thing and stretches it well past its reach — so it is
        // tightened when the plume is short.
        const halo = Math.exp(-Math.pow((x - slider) * haloK, 2));
        const lit =
          total + core * 0.9 * pulse * intensity * es + halo * 0.12 * intensity * es;

        // frame feedback on the ENERGY scalar — this is the trail
        const next = Math.min(1.5, cell.b * decay + lit);
        cell.b = next < 0.004 ? 0 : next;
        if (cell.b <= 0.012) continue;

        /*
         * Energy drives ALPHA, never a multiply on the colour. The reference
         * multiplies colour by brightness because it renders additively onto
         * black; doing that on a light track turns every low-energy cell into
         * a literal black square. Here a dim cell is simply a faint one, so
         * the ramp can stay light at the tail and only gain chroma as it
         * heats up.
         */
        const temp = Math.min(1, Math.max(0, 1 - dn));
        const heat = Math.min(1, cell.b * 0.9);
        /*
         * The accent is a function of this cell's OWN accumulated brightness,
         * so it inherits the plume's motion and decay for free: it lights up
         * where the plume is hottest, travels with the front, and dies as the
         * trail fades. No separate particle lifecycle, which is what made the
         * previous version read as pasted-on.
         */
        // Gentler curve than before: at 2.4 the exponential saturated so early
        // that a level-1 cell and a level-3 cell looked the same.
        const alpha = Math.min(0.97, 1 - Math.exp(-cell.b * 1.5));
        const tIdx = Math.min(
          TEMP_STEPS - 1,
          (Math.max(temp, heat * 0.65) * (TEMP_STEPS - 1)) | 0,
        );
        const aIdx = Math.min(ALPHA_STEPS - 1, (alpha * (ALPHA_STEPS - 1)) | 0);

        target.fillStyle = table[tIdx * ALPHA_STEPS + aIdx];
        target.fillRect(
          c * cellW,
          r * rowH,
          Math.max(1, cellW - gap),
          Math.max(1, rowH - gap),
        );
      }
    }

    // Composite: a soft blurred halo UNDER the crisp cells. Additive `lighter`
    // is wrong on a light bed — it only washes the field toward white — so the
    // glow is a plain low-opacity blurred pass instead.
    // `filter: blur()` is a software blur, so the halo only runs when the
    // frame budget is not being spent on a gesture.
    if (wantHalo) {
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.globalAlpha = 0.4;
      ctx.filter = "blur(" + (2.4 * dpr).toFixed(2) + "px)";
      ctx.drawImage(scene, 0, 0);
      ctx.filter = "none";
      ctx.globalAlpha = 1;
      ctx.drawImage(scene, 0, 0);
      ctx.restore();
    }
    ctx.restore();
  }

  function loop(now: number) {
    if (!running) return;
    /*
     * Runs at full rate even during a gesture. Halving it here made the plume
     * visibly trail the handle by up to a frame, and with the per-cell colour
     * table and the offscreen bypass the frame is cheap enough not to need it.
     */
    const dt = last ? Math.min(0.05, (now - last) / 1000) : 0.016;
    last = now;
    elapsed += dt;
    frame(dt);
    raf = window.requestAnimationFrame(loop);
  }

  function drawOnce() {
    elapsed = 3;
    for (let i = 0; i < 90; i += 1) frame(1 / 60);
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
      scene = null;
      sceneCtx = null;
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
