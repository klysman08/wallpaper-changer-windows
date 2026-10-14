/**
 * Wallpaper Studio — the hero's working editor.
 *
 * The layout, fit, effects and adjustments come from engine.js, a port of the app's
 * engine; this file only owns the page: the pictures, the controls, the preview and
 * the saved file. Nothing leaves the browser — pictures are decoded locally and the
 * wallpaper is handed back as a download.
 *
 * The studio composes for one screen. The app does every monitor; the engine port
 * still takes a list of them, so the page could offer more without touching it.
 */
import {
  RANGES,
  finish,
  fitRect,
  isIdentity,
  neutralAdjustments,
  planCollage,
  regionsFor,
} from './engine.js';

const SAMPLES = [
  'images/example 1 (1).jpg',
  'images/example 1 (2).jpg',
  'images/example 1 (3).jpg',
  'images/example 1 (4).jpg',
  'images/example 1 (5).jpg',
  'images/example 1 (1).jpeg',
  'images/example 1 (2).jpeg',
  'images/example 1 (3).jpeg',
  'images/example 1 (4).jpeg',
  'images/example 1 (5).jpeg',
];

const PRESETS = [
  { id: 'screen', label: 'This screen' },
  { id: '1920x1080', width: 1920, height: 1080, label: 'Full HD' },
  { id: '2560x1440', width: 2560, height: 1440, label: 'QHD' },
  { id: '3840x2160', width: 3840, height: 2160, label: '4K' },
  { id: '3440x1440', width: 3440, height: 1440, label: 'Ultrawide' },
  { id: '5120x1440', width: 5120, height: 1440, label: 'Super ultrawide' },
  { id: '1080x1920', width: 1080, height: 1920, label: 'Portrait' },
  { id: '1170x2532', width: 1170, height: 2532, label: 'Phone' },
  { id: 'custom', label: 'Custom…' },
];

const MIN_SIDE = 320;
const MAX_SIDE = 8192;
// The preview is composed at most this big; blur and vignette are sized relative to
// the screen, so it shows what the full-size file will.
const PREVIEW_MAX_W = 1600;
const PREVIEW_MAX_H = 1000;
// Larger sources are scaled down on load: a 4K cell never needs more.
const MAX_SOURCE_SIDE = 7680;
const MAX_POOL = 48;
// iOS Safari refuses canvases over 16.7 MP and draws nothing, without an error.
const IS_IOS = /iP(hone|ad|od)/.test(navigator.userAgent)
  || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
const MAX_EXPORT_PIXELS = IS_IOS ? 16_777_216 : 33_177_600;
// Formats a picture is plausibly stored in that browsers commonly cannot decode.
const UNREADABLE = ['heic', 'heif', 'tif', 'tiff', 'psd', 'cr2', 'cr3', 'nef', 'arw', 'dng', 'raf', 'orf', 'rw2', 'jxl'];

const $ = (id) => document.getElementById(id);
const sound = () => window.SoundEngine;
const clampSide = (v) => Math.min(MAX_SIDE, Math.max(MIN_SIDE, Math.round(Number(v) || 0)));
const nextFrame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));

/** The visitor's own screen in device pixels; Full HD when the browser will not say. */
function thisScreen() {
  const dpr = window.devicePixelRatio || 1;
  const width = Math.round(window.screen.width * dpr);
  const height = Math.round(window.screen.height * dpr);
  // An embedded or hidden view can report 0, or a tiny box.
  if (width < 640 || height < 480) return { width: 1920, height: 1080 };
  return { width: clampSide(width), height: clampSide(height) };
}

function presetSize(id) {
  if (id === 'screen') return thisScreen();
  const preset = PRESETS.find((p) => p.id === id);
  return preset && preset.width ? { width: preset.width, height: preset.height } : null;
}

const state = {
  screen: { preset: 'screen', ...thisScreen() },
  count: 4,
  fit: 'fill',
  effect: 'normal',
  adjust: neutralAdjustments(),
  format: 'png',
  pool: [], // { id, name, bitmap, thumb, sample }
  selection: [], // pool ids, one per slot
  nextId: 1,
};

/** The one screen the studio composes for, as the engine's monitor list. */
function monitors() {
  return [{ x: 0, y: 0, width: state.screen.width, height: state.screen.height }];
}

function poolItem(id) {
  return state.pool.find((p) => p.id === id);
}

/**
 * Keep one picture per slot, every one still in the pool. Slots past what the layout
 * shows are kept, so 8 → 4 → 8 gives the same pictures back.
 */
function ensureSelection() {
  const ids = state.pool.map((p) => p.id);
  if (!ids.length) {
    state.selection = [];
    return;
  }
  state.selection = state.selection.filter((id) => ids.includes(id));
  let cursor = 0;
  while (state.selection.length < state.count) {
    const unused = ids.filter((id) => !state.selection.includes(id));
    state.selection.push(unused.length ? unused[0] : ids[cursor++ % ids.length]);
  }
}

function shuffle() {
  const ids = state.pool.map((p) => p.id);
  if (!ids.length) return;
  const picked = [];
  while (picked.length < state.count) {
    const round = [...ids];
    for (let i = round.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [round[i], round[j]] = [round[j], round[i]];
    }
    picked.push(...round);
  }
  state.selection = picked.slice(0, state.count);
}

// ── decoding ─────────────────────────────────────────────────────────────────

function extensionOf(name) {
  const dot = name.lastIndexOf('.');
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
}

async function decode(blob) {
  let bitmap = await createImageBitmap(blob);
  const long = Math.max(bitmap.width, bitmap.height);
  if (long > MAX_SOURCE_SIDE) {
    const k = MAX_SOURCE_SIDE / long;
    const smaller = await createImageBitmap(bitmap, {
      resizeWidth: Math.max(1, Math.round(bitmap.width * k)),
      resizeHeight: Math.max(1, Math.round(bitmap.height * k)),
      resizeQuality: 'high',
    });
    bitmap.close();
    bitmap = smaller;
  }
  return bitmap;
}

function thumbnail(bitmap) {
  const h = 96;
  const w = Math.max(1, Math.round((bitmap.width / bitmap.height) * h));
  const c = document.createElement('canvas');
  c.width = Math.min(w, 192);
  c.height = h;
  const ctx = c.getContext('2d');
  const r = fitRect(bitmap.width, bitmap.height, c.width, c.height, 'fill');
  ctx.drawImage(bitmap, r.x, r.y, r.width, r.height);
  return c.toDataURL('image/jpeg', 0.8);
}

async function makeItem(source, name, sample) {
  const blob = typeof source === 'string' ? await (await fetch(source)).blob() : source;
  const bitmap = await decode(blob);
  return { id: state.nextId++, name, bitmap, thumb: thumbnail(bitmap), sample };
}

function removeFromPool(id) {
  const item = poolItem(id);
  if (!item) return;
  item.bitmap.close?.();
  state.pool = state.pool.filter((p) => p.id !== id);
}

async function loadSamples() {
  const results = await Promise.allSettled(
    SAMPLES.filter((src) => !state.pool.some((p) => p.sample && p.name === src))
      .map((src) => makeItem(encodeURI(src), src, true)),
  );
  // In list order, whichever finished decoding first.
  for (const r of results) if (r.status === 'fulfilled') state.pool.push(r.value);
  const failed = results.filter((r) => r.status === 'rejected').length;
  if (failed) showWarning([`${failed} sample picture(s) could not be loaded.`]);
}

/**
 * Add the user's files. The first ones replace the samples, so "my four pictures"
 * are what the collage shows; and every new picture goes to the front of the layout.
 * A file that cannot be read is named, never dropped in silence.
 */
async function addFiles(fileList, intoSlot = null) {
  const files = [...fileList];
  if (!files.length) return;
  setBusy(true, `Reading ${files.length === 1 ? 'picture' : `${files.length} pictures`}…`);
  const problems = [];
  const added = [];
  for (const file of files) {
    const ext = extensionOf(file.name);
    const looksLikeImage = file.type.startsWith('image/') || UNREADABLE.includes(ext);
    if (!looksLikeImage) {
      problems.push(`${file.name} (not a picture)`);
      continue;
    }
    if (state.pool.filter((p) => !p.sample).length >= MAX_POOL) {
      problems.push(`${file.name} (the studio holds ${MAX_POOL} pictures at a time)`);
      continue;
    }
    try {
      const item = await makeItem(file, file.name, false);
      state.pool.push(item);
      added.push(item);
    } catch {
      problems.push(UNREADABLE.includes(ext)
        ? `${file.name} (${ext.toUpperCase()} can't be read by this browser — save it as JPG or PNG first)`
        : `${file.name} (could not be read)`);
    }
  }

  if (added.length) {
    if (state.pool.every((p) => p.sample || added.includes(p))) {
      for (const p of state.pool.filter((q) => q.sample)) removeFromPool(p.id);
      state.selection = [];
    }
    const newIds = added.map((p) => p.id);
    if (intoSlot !== null) {
      state.selection[intoSlot] = newIds[0];
    } else {
      state.selection = [...newIds, ...state.selection.filter((id) => !newIds.includes(id))];
    }
    sound()?.playSuccess();
  }
  showWarning(problems.length ? [`Couldn't add ${problems.length === 1 ? 'one file' : `${problems.length} files`}: ${problems.join('; ')}.`] : []);
  setBusy(false);
  refresh();
}

// ── composition ──────────────────────────────────────────────────────────────

/**
 * Draw the collage onto a canvas at scale s: a black screen, then each cell's
 * picture fitted and clipped to it — what `compose_collage` pastes.
 */
function compose(s) {
  const mons = monitors().map((m) => ({
    x: 0,
    y: 0,
    width: Math.max(1, Math.round(m.width * s)),
    height: Math.max(1, Math.round(m.height * s)),
  }));
  const canvas = document.createElement('canvas');
  canvas.width = mons[0].width;
  canvas.height = mons[0].height;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = 'high';

  const cells = planCollage(mons, state.count, false);
  if (state.selection.length) {
    for (const cell of cells) {
      const item = poolItem(state.selection[cell.imageIndex % state.selection.length]);
      if (!item) continue;
      const { bitmap } = item;
      const r = fitRect(bitmap.width * s, bitmap.height * s, cell.width, cell.height, state.fit);
      ctx.save();
      ctx.beginPath();
      ctx.rect(cell.x, cell.y, cell.width, cell.height);
      ctx.clip();
      ctx.drawImage(bitmap, cell.x + r.x, cell.y + r.y, r.width, r.height);
      ctx.restore();
    }
  }
  return { canvas, ctx, mons, cells };
}

// The effect and adjustments run in a worker; if the browser cannot start a module
// worker they run here instead.
let worker = null;
let jobId = 0;
const jobs = new Map();
try {
  worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
  worker.onmessage = (event) => {
    const job = jobs.get(event.data.id);
    if (!job) return;
    jobs.delete(event.data.id);
    if (event.data.error) job.reject(new Error(event.data.error));
    else job.resolve(new Uint8ClampedArray(event.data.buffer));
  };
  worker.onerror = () => {
    worker = null;
    for (const job of jobs.values()) job.retry();
    jobs.clear();
  };
} catch {
  worker = null;
}

function processPixels(imageData, regions) {
  const { width, height } = imageData;
  const effect = state.effect;
  const adjust = { ...state.adjust };
  if (effect === 'normal' && isIdentity(adjust)) return Promise.resolve(imageData.data);
  const local = (data) => {
    finish(data, width, height, effect, adjust, regions);
    return data;
  };
  if (!worker) return Promise.resolve(local(imageData.data));
  // Keep a copy only so a worker that dies mid-job can be retried here.
  const backup = new Uint8ClampedArray(imageData.data);
  return new Promise((resolve, reject) => {
    const id = ++jobId;
    jobs.set(id, { resolve, reject, retry: () => resolve(local(backup)) });
    const buffer = imageData.data.buffer;
    worker.postMessage({ id, buffer, width, height, effect, adjust, regions }, [buffer]);
  });
}

async function render(s) {
  const { canvas, ctx, mons, cells } = compose(s);
  const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const data = await processPixels(image, regionsFor(mons, canvas.width, canvas.height));
  ctx.putImageData(new ImageData(data, canvas.width, canvas.height), 0, 0);
  return { canvas, cells };
}

// ── preview ──────────────────────────────────────────────────────────────────

const els = {
  root: $('studio'),
  stage: $('studioStage'),
  box: $('studioCanvasBox'),
  canvas: $('studioCanvas'),
  hits: $('studioHits'),
  busy: $('studioBusy'),
  busyText: $('studioBusyText'),
  size: $('studioSize'),
  countBadge: $('studioCountBadge'),
  screen: $('studioScreen'),
  pool: $('studioPool'),
  poolLabel: $('studioPoolLabel'),
  warn: $('studioWarn'),
  save: $('studioSave'),
  saveHint: $('studioSaveHint'),
  files: $('studioFiles'),
};

let previewBusy = false;
let previewDirty = false;

function requestPreview() {
  previewDirty = true;
  if (!previewBusy) runPreview();
}

async function runPreview() {
  previewBusy = true;
  try {
    while (previewDirty) {
      previewDirty = false;
      const { width, height } = state.screen;
      const s = Math.min(1, PREVIEW_MAX_W / width, PREVIEW_MAX_H / height);
      const { canvas, cells } = await render(s);
      els.canvas.width = canvas.width;
      els.canvas.height = canvas.height;
      els.canvas.getContext('2d').drawImage(canvas, 0, 0);
      drawHits(cells, canvas.width, canvas.height);
      fitBox();
    }
  } catch (error) {
    console.error('Studio preview failed', error);
    showWarning([`The preview could not be drawn: ${error.message}`]);
  } finally {
    previewBusy = false;
  }
}

/** Fit the canvas box inside the stage at the screen's own proportions. */
function fitBox() {
  const ar = state.screen.width / state.screen.height;
  const style = getComputedStyle(els.stage);
  const sw = els.stage.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const sh = els.stage.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
  if (sw <= 0 || sh <= 0) return;
  let w = sw;
  let h = w / ar;
  if (h > sh) {
    h = sh;
    w = h * ar;
  }
  els.box.style.width = `${Math.floor(w)}px`;
  els.box.style.height = `${Math.floor(h)}px`;
}

new ResizeObserver(fitBox).observe(els.stage);

const pct = (v, total) => `${(v / total) * 100}%`;

/** A hit target over every picture, from the same plan the canvas was drawn from. */
function drawHits(cells, width, height) {
  els.hits.replaceChildren();
  if (!state.pool.length) return;
  for (const cell of cells) {
    const hit = document.createElement('button');
    hit.type = 'button';
    hit.className = 'studio-hit';
    hit.dataset.slot = String(cell.imageIndex);
    hit.title = `Picture ${cell.imageIndex + 1}: click to change, drag to swap`;
    hit.setAttribute('aria-label', `Picture ${cell.imageIndex + 1}`);
    Object.assign(hit.style, {
      left: pct(cell.x, width), top: pct(cell.y, height), width: pct(cell.width, width), height: pct(cell.height, height),
    });
    const badge = document.createElement('span');
    badge.className = 'studio-hit-badge';
    badge.textContent = String(cell.imageIndex + 1);
    hit.append(badge);
    els.hits.append(hit);
  }
}

function setBusy(on, text = 'Rendering…') {
  els.busy.hidden = !on;
  els.busyText.textContent = text;
}

function showWarning(lines) {
  els.warn.hidden = !lines.length;
  els.warn.textContent = lines.join(' ');
}

// ── controls ─────────────────────────────────────────────────────────────────

// The sidebar: one section at a time.
const tabs = [...els.root.querySelectorAll('[data-studio-tab]')];
const panels = [...els.root.querySelectorAll('[data-studio-panel]')];
function showSection(name) {
  for (const tab of tabs) {
    const on = tab.dataset.studioTab === name;
    tab.classList.toggle('active', on);
    tab.setAttribute('aria-selected', String(on));
    tab.tabIndex = on ? 0 : -1;
  }
  for (const panel of panels) {
    const on = panel.dataset.studioPanel === name;
    panel.hidden = !on;
    panel.classList.toggle('active', on);
  }
}
tabs.forEach((tab, i) => {
  tab.addEventListener('click', () => showSection(tab.dataset.studioTab));
  tab.addEventListener('keydown', (e) => {
    const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    const next = tabs[(i + step + tabs.length) % tabs.length];
    showSection(next.dataset.studioTab);
    next.focus();
  });
});
showSection('layout');

function renderScreenRow() {
  const s = state.screen;
  els.screen.replaceChildren();

  const select = document.createElement('select');
  select.className = 'studio-select';
  select.setAttribute('aria-label', 'Screen resolution');
  for (const preset of PRESETS) {
    const option = document.createElement('option');
    option.value = preset.id;
    const size = presetSize(preset.id);
    option.textContent = size ? `${preset.label} · ${size.width}×${size.height}` : preset.label;
    select.append(option);
  }
  select.value = s.preset;
  select.addEventListener('change', () => {
    s.preset = select.value;
    const size = presetSize(select.value);
    if (size) Object.assign(s, size);
    renderScreenRow();
    refresh();
  });

  const custom = document.createElement('div');
  custom.className = 'studio-custom';
  custom.hidden = s.preset !== 'custom';
  const dim = (key) => {
    const input = document.createElement('input');
    input.type = 'number';
    input.className = 'studio-number';
    input.min = String(MIN_SIDE);
    input.max = String(MAX_SIDE);
    input.step = '1';
    input.value = String(s[key]);
    input.setAttribute('aria-label', key === 'width' ? 'Width in pixels' : 'Height in pixels');
    input.addEventListener('change', () => {
      s[key] = clampSide(input.value);
      input.value = String(s[key]);
      refresh();
    });
    return input;
  };
  const times = document.createElement('span');
  times.textContent = '×';
  custom.append(dim('width'), times, dim('height'));

  els.screen.append(select, custom);
}

function renderPool() {
  els.pool.replaceChildren();
  const used = new Set(state.selection.slice(0, state.count));
  els.poolLabel.textContent = state.pool.length
    ? `Pictures (${state.pool.length})${state.pool.every((p) => p.sample) ? ' · samples' : ''}`
    : 'No pictures yet';
  for (const item of state.pool) {
    const tile = document.createElement('div');
    tile.className = `studio-pool-item${used.has(item.id) ? ' in-use' : ''}`;
    tile.dataset.poolId = String(item.id);
    tile.title = `${item.name.split('/').pop()} — drag onto the preview to place it`;
    const img = document.createElement('img');
    img.src = item.thumb;
    img.alt = item.name.split('/').pop();
    img.draggable = false;
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'studio-pool-remove';
    remove.setAttribute('aria-label', `Remove ${img.alt}`);
    remove.textContent = '×';
    remove.addEventListener('click', (e) => {
      e.stopPropagation();
      sound()?.playClick();
      removeFromPool(item.id);
      refresh();
    });
    tile.append(img, remove);
    els.pool.append(tile);
  }
  $('studioSamples').disabled = SAMPLES.every((src) => state.pool.some((p) => p.sample && p.name === src));
}

function updateSaveHint() {
  const { width, height } = state.screen;
  els.saveHint.textContent = `${width}×${height} ${state.format.toUpperCase()}, composed again at full size in your browser. Nothing is uploaded.`;
}

/** Everything that follows a change to the screen, the layout or the pictures. */
function refresh() {
  ensureSelection();
  els.size.textContent = `${state.screen.width}×${state.screen.height}`;
  els.countBadge.textContent = String(state.count);
  els.root.classList.toggle('is-empty', !state.pool.length);
  renderPool();
  updateSaveHint();
  requestPreview();
}

function bindChips(attr, apply) {
  const buttons = els.root.querySelectorAll(`[data-${attr}]`);
  buttons.forEach((btn) => {
    btn.addEventListener('click', () => {
      buttons.forEach((b) => b.classList.toggle('active', b === btn));
      apply(btn.dataset[attr]);
    });
  });
}

bindChips('count', (v) => {
  state.count = Number(v);
  refresh();
});
bindChips('fit', (v) => {
  state.fit = v;
  requestPreview();
});
bindChips('effect', (v) => {
  state.effect = v;
  requestPreview();
});
bindChips('format', (v) => {
  state.format = v;
  updateSaveHint();
});

// Adjustments: the six sliders, double-click a label to reset it.
const sliders = [...els.root.querySelectorAll('[data-studio-adjust]')];
function showSlider(input) {
  const out = input.closest('.mockup-adjust-row')?.querySelector('output');
  const v = Number(input.value);
  if (out) out.textContent = v > 0 && Number(input.min) < 0 ? `+${v}` : String(v);
}
for (const input of sliders) {
  const key = input.dataset.studioAdjust;
  const range = RANGES.find(([k]) => k === key);
  input.addEventListener('input', () => {
    state.adjust[key] = Math.min(range[2], Math.max(range[1], Number(input.value) || 0));
    showSlider(input);
    requestPreview();
  });
  input.closest('.mockup-adjust-row')?.querySelector('.mockup-adjust-head')
    ?.addEventListener('dblclick', (e) => {
      e.preventDefault();
      input.value = '0';
      state.adjust[key] = 0;
      showSlider(input);
      sound()?.playClick();
      requestPreview();
    });
}
$('studioAdjustReset').addEventListener('click', () => {
  state.adjust = neutralAdjustments();
  for (const input of sliders) {
    input.value = '0';
    showSlider(input);
  }
  requestPreview();
});

$('studioShuffle').addEventListener('click', () => {
  shuffle();
  refresh();
});
$('studioSamples').addEventListener('click', async () => {
  setBusy(true, 'Loading samples…');
  await loadSamples();
  setBusy(false);
  // Deal the layout again, so the samples are on screen rather than only in the tray.
  state.selection = [];
  refresh();
});

// Files: the button, a drop anywhere on the studio, and replacing one picture.
let replaceSlot = null;
$('studioAdd').addEventListener('click', () => {
  replaceSlot = null;
  els.files.click();
});
els.files.addEventListener('change', () => {
  const slot = replaceSlot;
  replaceSlot = null;
  addFiles(els.files.files, slot);
  els.files.value = '';
});

let dragDepth = 0;
const hasFiles = (e) => [...(e.dataTransfer?.types || [])].includes('Files');
els.root.addEventListener('dragenter', (e) => {
  if (!hasFiles(e)) return;
  e.preventDefault();
  dragDepth++;
  els.root.classList.add('is-dropping');
});
els.root.addEventListener('dragover', (e) => {
  if (!hasFiles(e)) return;
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
});
els.root.addEventListener('dragleave', (e) => {
  if (!hasFiles(e)) return;
  dragDepth = Math.max(0, dragDepth - 1);
  if (!dragDepth) els.root.classList.remove('is-dropping');
});
els.root.addEventListener('drop', (e) => {
  if (!hasFiles(e)) return;
  e.preventDefault();
  dragDepth = 0;
  els.root.classList.remove('is-dropping');
  addFiles(e.dataTransfer.files);
});

// ── picking and swapping ─────────────────────────────────────────────────────

let picker = null;

function openPicker(slot) {
  if (!picker) {
    picker = document.createElement('dialog');
    picker.className = 'studio-picker';
    picker.innerHTML = `
      <div class="studio-picker-head">
        <strong class="studio-picker-title"></strong>
        <button type="button" class="btn btn-ghost btn-xs studio-picker-close" aria-label="Close">✕</button>
      </div>
      <div class="studio-picker-grid"></div>
      <div class="studio-picker-foot">
        <button type="button" class="btn btn-outline btn-xs studio-picker-upload">Upload a picture…</button>
      </div>`;
    document.body.append(picker);
    picker.querySelector('.studio-picker-close').addEventListener('click', () => picker.close());
    picker.addEventListener('click', (e) => {
      if (e.target === picker) picker.close();
    });
    picker.querySelector('.studio-picker-upload').addEventListener('click', () => {
      replaceSlot = Number(picker.dataset.slot);
      picker.close();
      els.files.click();
    });
  }
  picker.dataset.slot = String(slot);
  picker.querySelector('.studio-picker-title').textContent = `Choose picture ${slot + 1}`;
  const grid = picker.querySelector('.studio-picker-grid');
  grid.replaceChildren();
  for (const item of state.pool) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `studio-picker-item${state.selection[slot] === item.id ? ' active' : ''}`;
    const img = document.createElement('img');
    img.src = item.thumb;
    img.alt = item.name.split('/').pop();
    btn.append(img);
    btn.addEventListener('click', () => {
      sound()?.playClick();
      state.selection[slot] = item.id;
      picker.close();
      refresh();
    });
    grid.append(btn);
  }
  picker.showModal();
}

// A drag starts on a picture in the preview (swap two slots) or in the tray (place
// a picture into a slot). Pointer events rather than HTML drag and drop, so it works
// with a finger too; the ghost lives on <body> so no transformed card clips it.
let drag = null;
let suppressClick = false;

function startPointer(e, el, source) {
  if (e.button !== 0) return;
  drag = { source, x: e.clientX, y: e.clientY, active: false, ghost: null, target: null, el };
}

function slotUnder(x, y) {
  const el = document.elementFromPoint(x, y);
  return el ? el.closest('.studio-hit') : null;
}

window.addEventListener('pointermove', (e) => {
  if (!drag) return;
  if (!drag.active) {
    if (Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 6) return;
    drag.active = true;
    const id = drag.source.kind === 'slot' ? state.selection[drag.source.slot] : drag.source.id;
    const item = poolItem(id);
    drag.ghost = document.createElement('img');
    drag.ghost.className = 'studio-ghost';
    drag.ghost.alt = '';
    if (item) drag.ghost.src = item.thumb;
    document.body.append(drag.ghost);
    drag.el.classList.add('is-source');
    els.root.classList.add('is-dragging');
  }
  drag.ghost.style.transform = `translate(${e.clientX}px, ${e.clientY}px)`;
  const target = slotUnder(e.clientX, e.clientY);
  if (target !== drag.target) {
    drag.target?.classList.remove('is-target');
    drag.target = target && target !== drag.el ? target : null;
    drag.target?.classList.add('is-target');
  }
});

function endPointer(commit) {
  if (!drag) return;
  const { active, target, source, ghost, el } = drag;
  drag = null;
  ghost?.remove();
  el.classList.remove('is-source');
  target?.classList.remove('is-target');
  els.root.classList.remove('is-dragging');
  if (!active) return;
  suppressClick = true;
  setTimeout(() => { suppressClick = false; }, 0);
  if (!commit || !target) return;
  const to = Number(target.dataset.slot);
  if (source.kind === 'slot') {
    const from = source.slot;
    [state.selection[from], state.selection[to]] = [state.selection[to], state.selection[from]];
  } else {
    state.selection[to] = source.id;
  }
  sound()?.playClick();
  refresh();
}

window.addEventListener('pointerup', () => endPointer(true));
window.addEventListener('pointercancel', () => endPointer(false));

els.hits.addEventListener('pointerdown', (e) => {
  const hit = e.target.closest('.studio-hit');
  if (hit) startPointer(e, hit, { kind: 'slot', slot: Number(hit.dataset.slot) });
});
els.hits.addEventListener('click', (e) => {
  const hit = e.target.closest('.studio-hit');
  if (!hit || suppressClick) return;
  sound()?.playClick();
  openPicker(Number(hit.dataset.slot));
});
els.pool.addEventListener('pointerdown', (e) => {
  const tile = e.target.closest('.studio-pool-item');
  if (!tile || e.target.closest('.studio-pool-remove')) return;
  startPointer(e, tile, { kind: 'pool', id: Number(tile.dataset.poolId) });
});

// ── saving ───────────────────────────────────────────────────────────────────

let saving = false;

async function save() {
  if (saving) return;
  const { width, height } = state.screen;
  if (width * height > MAX_EXPORT_PIXELS) {
    showWarning([`${width}×${height} is more than this browser can draw in one canvas. Choose a smaller resolution.`]);
    return;
  }
  if (!state.pool.length) {
    showWarning(['Add a picture first.']);
    return;
  }
  saving = true;
  const label = els.save.querySelector('span');
  els.save.disabled = true;
  $('studioQuickSave').disabled = true;
  label.textContent = 'Rendering…';
  setBusy(true, `Rendering ${width}×${height}…`);
  await nextFrame();
  try {
    const { canvas } = await render(1);
    const type = state.format === 'jpg' ? 'image/jpeg' : 'image/png';
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, type, 0.92));
    if (!blob) throw new Error('the browser could not encode the image');
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `kokoropaper-${canvas.width}x${canvas.height}.${state.format}`;
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 30_000);
    sound()?.playSuccess();
    label.textContent = 'Saved!';
    showWarning([]);
  } catch (error) {
    console.error('Studio export failed', error);
    showWarning([`The wallpaper could not be saved: ${error.message}`]);
    label.textContent = 'Save wallpaper';
  } finally {
    setBusy(false);
    saving = false;
    els.save.disabled = false;
    $('studioQuickSave').disabled = false;
    setTimeout(() => { label.textContent = 'Save wallpaper'; }, 1600);
  }
}

els.save.addEventListener('click', save);
$('studioQuickSave').addEventListener('click', save);

// ── start ────────────────────────────────────────────────────────────────────

renderScreenRow();
refresh();
setBusy(true, 'Loading samples…');
loadSamples().then(() => {
  setBusy(false);
  refresh();
});
