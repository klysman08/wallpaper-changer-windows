/**
 * The KokoroPaper engine's composition, effects and adjustments, ported to the
 * browser for the site's Wallpaper Studio.
 *
 * The source of truth is the Rust in desktop/src-tauri/crates/wallpaper-core:
 * collage.rs, effects.rs and adjust.rs. Every number here is copied from there,
 * integer truncation included, so a wallpaper saved from the site is the one the
 * app would compose. Two differences are accepted: resampling is the browser's
 * (the app uses SIMD Lanczos3), and 3x3 filters accumulate in f64 rather than f32.
 * When any of those files changes, change this one with it.
 *
 * Pure functions over RGBA byte arrays — no DOM — so the worker and node can load it.
 */

// ── collage.rs ───────────────────────────────────────────────────────────────

export const COUNT_MIN = 1;
export const COUNT_MAX = 8;
export const FIT_MODES = ['fill', 'fit', 'stretch', 'center'];

/** Columns per image count, then ceil(sqrt(n)). Hard-coded, as in `columns_for`. */
export function columnsFor(n) {
  switch (n) {
    case 1: return 1;
    case 2: return 2;
    case 3: return 2;
    case 4: return 2;
    case 5: return 3;
    case 6: return 3;
    case 7: return 4;
    case 8: return 4;
    case 9: return 3;
    default: return Math.max(1, Math.ceil(Math.sqrt(n)));
  }
}

/**
 * Split a w x h area into n cells. `h / rows` truncates, leaving a black strip at the
 * bottom, and a short last row is centred — both deliberate, as in `grid_layout`.
 */
export function gridLayout(n, w, h) {
  const cols = columnsFor(n);
  const rows = Math.ceil(n / cols);
  const cellH = Math.trunc(h / rows);
  const cells = [];
  let placed = 0;
  for (let r = 0; r < rows; r++) {
    const rowCols = Math.min(cols, n - placed);
    if (rowCols === 0) break;
    const cellW = Math.trunc(w / rowCols);
    const offsetX = Math.trunc((w - rowCols * cellW) / 2);
    for (let c = 0; c < rowCols; c++) {
      cells.push({ x: offsetX + c * cellW, y: r * cellH, width: cellW, height: cellH });
      placed++;
    }
  }
  return cells;
}

/** The bounding box of every monitor: `monitor::virtual_desktop`. */
export function virtualDesktop(monitors) {
  if (!monitors.length) return { x: 0, y: 0, width: 0, height: 0 };
  const minX = Math.min(...monitors.map((m) => m.x));
  const minY = Math.min(...monitors.map((m) => m.y));
  const maxX = Math.max(...monitors.map((m) => m.x + m.width));
  const maxY = Math.max(...monitors.map((m) => m.y + m.height));
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

/**
 * Where each image lands, in composite pixels: `plan_collage`. With sameForAll every
 * monitor repeats the same short list.
 */
export function planCollage(monitors, count, sameForAll) {
  const desk = virtualDesktop(monitors);
  const cells = [];
  let imageIndex = 0;
  monitors.forEach((monitor, m) => {
    gridLayout(count, monitor.width, monitor.height).forEach((cell, j) => {
      cells.push({
        monitor: m,
        imageIndex: sameForAll ? j : imageIndex,
        x: monitor.x - desk.x + cell.x,
        y: monitor.y - desk.y + cell.y,
        width: cell.width,
        height: cell.height,
      });
      if (!sameForAll) imageIndex++;
    });
  });
  return cells;
}

/** How many pictures a collage asks for. */
export function slotsNeeded(monitorCount, count, sameForAll) {
  return sameForAll ? count : count * monitorCount;
}

/**
 * Where a sw x sh picture goes inside a tw x th cell: `fit_image`, as a destination
 * rectangle relative to the cell. The caller clips to the cell on a black ground,
 * which is what the app's crop and paste do.
 */
export function fitRect(sw, sh, tw, th, mode) {
  if (mode === 'span') mode = 'fill';
  tw = Math.max(1, tw);
  th = Math.max(1, th);
  if (mode === 'stretch') return { x: 0, y: 0, width: tw, height: th };
  if (mode === 'center') {
    return { x: Math.trunc((tw - sw) / 2), y: Math.trunc((th - sh) / 2), width: sw, height: sh };
  }
  const ratioW = tw / sw;
  const ratioH = th / sh;
  const ratio = mode === 'fill' ? Math.max(ratioW, ratioH) : Math.min(ratioW, ratioH);
  const newW = Math.max(1, Math.trunc(sw * ratio));
  const newH = Math.max(1, Math.trunc(sh * ratio));
  if (mode === 'fill') {
    const left = Math.trunc((newW - tw) / 2);
    const top = Math.trunc((newH - th) / 2);
    return { x: 0 - left || 0, y: 0 - top || 0, width: newW, height: newH };
  }
  return { x: Math.trunc((tw - newW) / 2), y: Math.trunc((th - newH) / 2), width: newW, height: newH };
}

// ── effects.rs ───────────────────────────────────────────────────────────────

export const EFFECTS = ['normal', 'bw', 'vintage', 'hdr'];

const VINTAGE_BLACK = [0x3b, 0x2a, 0x1a];
const VINTAGE_WHITE = [0xd8, 0xc3, 0xa5];
const SMOOTH = [[1, 1, 1, 1, 5, 1, 1, 1, 1], 13];
const DETAIL = [[0, -1, 0, -1, 10, -1, 0, -1, 0], 6];

/** ITU-R 601-2 luma in fixed point — Pillow's convert("L"). */
export function luma(r, g, b) {
  return (r * 19595 + g * 38470 + b * 7471 + 32768) >>> 16;
}

/** The mean luminance, rounded as int(mean + 0.5). */
export function meanLuma(data) {
  let total = 0;
  for (let i = 0; i < data.length; i += 4) total += luma(data[i], data[i + 1], data[i + 2]);
  const count = Math.max(1, data.length / 4);
  return Math.min(255, Math.trunc(total / count + 0.5));
}

/**
 * a + f*(b - a), clamped then truncated. Uint8ClampedArray would round on assignment,
 * so every write goes through this.
 */
function lerp(a, b, f) {
  const v = a + f * (b - a);
  return v <= 0 ? 0 : v >= 255 ? 255 : v | 0;
}

function toGrey(data) {
  for (let i = 0; i < data.length; i += 4) {
    const l = luma(data[i], data[i + 1], data[i + 2]);
    data[i] = l;
    data[i + 1] = l;
    data[i + 2] = l;
  }
}

/** `Image.blend(a, b, alpha)` into a. */
function blendInto(a, b, alpha) {
  for (let i = 0; i < a.length; i += 4) {
    a[i] = lerp(a[i], b[i], alpha);
    a[i + 1] = lerp(a[i + 1], b[i + 1], alpha);
    a[i + 2] = lerp(a[i + 2], b[i + 2], alpha);
  }
}

/** `ImageOps.colorize` — the ramp covers 0..=254 with floor division; 255 is white. */
function colorize(data, black, white) {
  const lut = [0, 1, 2].map((c) => {
    const table = new Uint8Array(256);
    const lo = black[c];
    const hi = white[c];
    for (let i = 0; i < 255; i++) table[i] = lo + Math.floor((i * (hi - lo)) / 255);
    table[255] = white[c];
    return table;
  });
  for (let i = 0; i < data.length; i += 4) {
    const level = data[i];
    data[i] = lut[0][level];
    data[i + 1] = lut[1][level];
    data[i + 2] = lut[2][level];
  }
}

/** A 3x3 convolution with Pillow's arithmetic; the border is copied through. */
function filter3x3(data, width, height, kernel, scale) {
  const out = new Uint8ClampedArray(data);
  if (width < 3 || height < 3) return out;
  const k = kernel.map((w) => w / scale);
  const stride = width * 4;
  for (let y = 1; y < height - 1; y++) {
    for (let x = 1; x < width - 1; x++) {
      const p = y * stride + x * 4;
      for (let c = 0; c < 3; c++) {
        const i = p + c;
        let sum = data[i - stride - 4] * k[0] + data[i - stride] * k[1] + data[i - stride + 4] * k[2];
        sum += data[i - 4] * k[3] + data[i] * k[4] + data[i + 4] * k[5];
        sum += data[i + stride - 4] * k[6] + data[i + stride] * k[7] + data[i + stride + 4] * k[8];
        out[i] = sum <= 0 ? 0 : sum >= 255 ? 255 : (sum + 0.5) | 0;
      }
    }
  }
  return out;
}

/** One of the four effects, on the finished canvas, in place. */
export function applyEffect(data, width, height, effect) {
  switch (effect) {
    case 'normal':
      return;
    case 'bw':
      toGrey(data);
      return;
    case 'vintage': {
      toGrey(data);
      colorize(data, VINTAGE_BLACK, VINTAGE_WHITE);
      // ImageEnhance.Color(sepia).enhance(0.9): blend from its own grey.
      const grey = new Uint8ClampedArray(data);
      toGrey(grey);
      blendInto(grey, data, 0.9);
      data.set(grey);
      return;
    }
    case 'hdr': {
      // Contrast 1.35 against the mean luminance.
      const mean = meanLuma(data);
      for (let i = 0; i < data.length; i += 4) {
        data[i] = lerp(mean, data[i], 1.35);
        data[i + 1] = lerp(mean, data[i + 1], 1.35);
        data[i + 2] = lerp(mean, data[i + 2], 1.35);
      }
      // Sharpness 1.45: extrapolate away from a SMOOTH-filtered copy.
      const smooth = filter3x3(data, width, height, SMOOTH[0], SMOOTH[1]);
      blendInto(smooth, data, 1.45);
      data.set(filter3x3(smooth, width, height, DETAIL[0], DETAIL[1]));
      return;
    }
    default:
      throw new Error(`Unknown effect: ${effect}`);
  }
}

// ── adjust.rs ────────────────────────────────────────────────────────────────

/** Every adjustment: key, min, max. 0 is neutral for all of them. */
export const RANGES = [
  ['brightness', -100, 100],
  ['contrast', -100, 100],
  ['saturation', -100, 100],
  ['warmth', -100, 100],
  ['blur', 0, 100],
  ['vignette', 0, 100],
];

const WARMTH_SPAN = 0.12;
const BLUR_SPAN = 0.02;
const VIGNETTE_STRENGTH = 0.6;
const VIGNETTE_START = 0.35;

export function neutralAdjustments() {
  return Object.fromEntries(RANGES.map(([key]) => [key, 0]));
}

export function isIdentity(adjust) {
  return RANGES.every(([key]) => !adjust[key]);
}

/** Each monitor's rectangle in canvas coordinates, clipped: `adjust::regions`. */
export function regionsFor(monitors, width, height) {
  const desk = virtualDesktop(monitors);
  const clamp = (v, hi) => Math.min(Math.max(v, 0), hi);
  const found = monitors
    .map((m) => {
      const x0 = clamp(m.x - desk.x, width);
      const y0 = clamp(m.y - desk.y, height);
      const x1 = clamp(m.x - desk.x + m.width, width);
      const y1 = clamp(m.y - desk.y + m.height, height);
      return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
    })
    .filter((r) => r.width > 0 && r.height > 0);
  return found.length ? found : [{ x: 0, y: 0, width, height }];
}

/**
 * The six sliders on the finished canvas, in place: blur, then tone, then vignette.
 * All-zero changes nothing.
 */
export function applyAdjustments(data, width, height, adjust, regions) {
  if (isIdentity(adjust)) return;
  if (adjust.blur > 0) {
    for (const region of regions) {
      const short = Math.min(region.width, region.height);
      const radius = Math.max(1, Math.round((adjust.blur / 100) * BLUR_SPAN * short));
      blurRegion(data, width, region, radius);
    }
  }
  if (adjust.brightness || adjust.contrast || adjust.saturation || adjust.warmth) {
    tone(data, adjust);
  }
  if (adjust.vignette > 0) {
    for (const region of regions) vignetteRegion(data, width, region, adjust.vignette);
  }
}

const factor = (value) => 1 + value / 100;

function tone(data, adjust) {
  if (adjust.brightness) {
    const f = factor(adjust.brightness);
    const lut = new Uint8Array(256);
    for (let v = 0; v < 256; v++) lut[v] = lerp(0, v, f);
    for (let i = 0; i < data.length; i += 4) {
      data[i] = lut[data[i]];
      data[i + 1] = lut[data[i + 1]];
      data[i + 2] = lut[data[i + 2]];
    }
  }

  // Measured after brightness, since that is the picture contrast is applied to.
  let contrast = null;
  if (adjust.contrast) {
    const mean = meanLuma(data);
    const f = factor(adjust.contrast);
    contrast = new Uint8Array(256);
    for (let v = 0; v < 256; v++) contrast[v] = lerp(mean, v, f);
  }
  const saturation = adjust.saturation ? factor(adjust.saturation) : null;
  let red = null;
  let blue = null;
  if (adjust.warmth) {
    const shift = (adjust.warmth / 100) * WARMTH_SPAN;
    const scale = (gain) => {
      const lut = new Uint8Array(256);
      for (let v = 0; v < 256; v++) lut[v] = Math.min(255, Math.max(0, Math.round(v * gain)));
      return lut;
    };
    red = scale(1 + shift);
    blue = scale(1 - shift);
  }
  if (!contrast && saturation === null && !red) return;

  for (let i = 0; i < data.length; i += 4) {
    if (contrast) {
      data[i] = contrast[data[i]];
      data[i + 1] = contrast[data[i + 1]];
      data[i + 2] = contrast[data[i + 2]];
    }
    if (saturation !== null) {
      const grey = luma(data[i], data[i + 1], data[i + 2]);
      data[i] = lerp(grey, data[i], saturation);
      data[i + 1] = lerp(grey, data[i + 1], saturation);
      data[i + 2] = lerp(grey, data[i + 2], saturation);
    }
    if (red) {
      data[i] = red[data[i]];
      data[i + 2] = blue[data[i + 2]];
    }
  }
}

/** Three box blurs each way, edges clamped: close to a Gaussian, linear in the radius. */
function blurRegion(data, canvasWidth, region, radius) {
  const { width: w, height: h } = region;
  const stride = w * 4;
  const canvasStride = canvasWidth * 4;
  const buf = new Uint8Array(stride * h);
  for (let y = 0; y < h; y++) {
    const start = (region.y + y) * canvasStride + region.x * 4;
    buf.set(data.subarray(start, start + stride), y * stride);
  }
  const scratch = new Uint8Array(buf.length);
  for (let pass = 0; pass < 3; pass++) {
    for (let y = 0; y < h; y++) blurRow(buf, scratch, y * stride, w, radius);
    blurColumns(scratch, buf, stride, w, h, radius);
  }
  for (let y = 0; y < h; y++) {
    const start = (region.y + y) * canvasStride + region.x * 4;
    data.set(buf.subarray(y * stride, (y + 1) * stride), start);
  }
}

function blurRow(src, out, offset, w, radius) {
  const span = 2 * radius + 1;
  const half = (span - 1) / 2;
  const last = w - 1;
  let s0 = 0;
  let s1 = 0;
  let s2 = 0;
  for (let dx = -radius; dx <= radius; dx++) {
    const p = offset + Math.min(Math.max(dx, 0), last) * 4;
    s0 += src[p];
    s1 += src[p + 1];
    s2 += src[p + 2];
  }
  for (let x = 0; x < w; x++) {
    const o = offset + x * 4;
    out[o] = Math.floor((s0 + half) / span);
    out[o + 1] = Math.floor((s1 + half) / span);
    out[o + 2] = Math.floor((s2 + half) / span);
    out[o + 3] = src[o + 3];
    const add = offset + Math.min(x + radius + 1, last) * 4;
    const sub = offset + Math.max(x - radius, 0) * 4;
    s0 += src[add] - src[sub];
    s1 += src[add + 1] - src[sub + 1];
    s2 += src[add + 2] - src[sub + 2];
  }
}

function blurColumns(src, out, stride, w, h, radius) {
  const span = 2 * radius + 1;
  const half = (span - 1) / 2;
  const row = (y) => Math.min(Math.max(y, 0), h - 1) * stride;
  const sums = new Uint32Array(stride);
  for (let dy = -radius; dy <= radius; dy++) {
    const r = row(dy);
    for (let i = 0; i < stride; i++) sums[i] += src[r + i];
  }
  for (let y = 0; y < h; y++) {
    const o = y * stride;
    for (let i = 0; i < stride; i++) out[o + i] = Math.floor((sums[i] + half) / span);
    const add = row(y + radius + 1);
    const sub = row(y - radius);
    for (let i = 0; i < stride; i++) sums[i] = sums[i] + src[add + i] - src[sub + i];
  }
}

/** Darken toward the corners of one monitor, on its own proportions. */
function vignetteRegion(data, canvasWidth, region, vignette) {
  const strength = (vignette / 100) * VIGNETTE_STRENGTH;
  const halfW = region.width / 2;
  const halfH = region.height / 2;
  const canvasStride = canvasWidth * 4;
  for (let y = 0; y < region.height; y++) {
    const dy = (y + 0.5 - halfH) / halfH;
    const start = (region.y + y) * canvasStride + region.x * 4;
    for (let x = 0; x < region.width; x++) {
      const dx = (x + 0.5 - halfW) / halfW;
      const d = Math.sqrt((dx * dx + dy * dy) / 2);
      const t = Math.min(Math.max((d - VIGNETTE_START) / (1 - VIGNETTE_START), 0), 1);
      if (t <= 0) continue;
      const keep = 1 - strength * t * t * (3 - 2 * t);
      const p = start + x * 4;
      data[p] = (data[p] * keep) | 0;
      data[p + 1] = (data[p + 1] * keep) | 0;
      data[p + 2] = (data[p + 2] * keep) | 0;
    }
  }
}

/** The effect, then the adjustments — the order `compose_collage` runs them in. */
export function finish(data, width, height, effect, adjust, regions) {
  applyEffect(data, width, height, effect);
  applyAdjustments(data, width, height, adjust, regions);
}
