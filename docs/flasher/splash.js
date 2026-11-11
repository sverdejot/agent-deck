// Agent Deck start screen encoder: turns 480×480 RGBA frames into the deck's splash format
// (docs/splash.md), which the firmware decodes in crates/deck-ui/src/splash.rs. No DOM here, so
// it also runs under Node for tests.

export const W = 480;
export const H = 480;
/** Height of the deck's two-line hint (crates/deck-ui/src/ui/offline.rs: lines at centre ∓ 13). */
export const TEXT_H = 44;
/** Where the `splash` partition starts and how big it is (firmware/partitions.csv). */
export const PARTITION_OFFSET = 0x400000;
export const PARTITION_SIZE = 0x400000;
export const MIN_DELAY_MS = 20;
export const MAX_DELAY_MS = 65535;
/** Bytes per second the Indicator's CH340 takes at 115200 baud, before compression. */
export const WRITE_RATE = 11000;

const HEADER_LEN = 24;

/** RGBA → RGB565, rounded to the nearest level. */
function to565(r, g, b) {
  return (((r * 31 + 127) / 255) << 11) | (((g * 63 + 127) / 255) << 5) | ((b * 31 + 127) / 255);
}

export function rgb565ToRgb(c) {
  const r = (c >> 11) & 31, g = (c >> 5) & 63, b = c & 31;
  return [(r * 255 + 15) / 31 | 0, (g * 255 + 31) / 63 | 0, (b * 255 + 15) / 31 | 0];
}

/**
 * Picks at most `maxColors` colours for all frames together: the exact colours when they fit, otherwise
 * median cut over the RGB565 histogram. Returns the palette and a 565 → index lookup.
 */
function buildPalette(hist, maxColors) {
  const used = [];
  for (let c = 0; c < 65536; c++) if (hist[c]) used.push(c);
  const lut = new Uint8Array(65536);
  if (used.length <= maxColors) {
    used.forEach((c, i) => (lut[c] = i));
    return { palette: used, lut };
  }
  const chan = (c, k) => (k === 0 ? (c >> 11) << 1 : k === 1 ? (c >> 5) & 63 : (c & 31) << 1); // 6-bit scale
  let boxes = [used];
  while (boxes.length < maxColors) {
    // split the box with the widest channel, weighted by how many pixels it holds
    let best = -1, bestScore = 0, bestK = 0;
    boxes.forEach((box, i) => {
      if (box.length < 2) return;
      let pixels = 0;
      for (const c of box) pixels += hist[c];
      for (let k = 0; k < 3; k++) {
        let lo = 64, hi = -1;
        for (const c of box) {
          const v = chan(c, k);
          if (v < lo) lo = v;
          if (v > hi) hi = v;
        }
        const score = (hi - lo) * Math.sqrt(pixels);
        if (score > bestScore) [best, bestScore, bestK] = [i, score, k];
      }
    });
    if (best < 0) break;
    const box = boxes[best].sort((a, b) => chan(a, bestK) - chan(b, bestK));
    let total = 0;
    for (const c of box) total += hist[c];
    let acc = 0, cut = 1;
    for (; cut < box.length - 1; cut++) {
      acc += hist[box[cut - 1]];
      if (acc >= total / 2) break;
    }
    boxes.splice(best, 1, box.slice(0, cut), box.slice(cut));
  }
  const palette = boxes.map((box) => {
    let n = 0, r = 0, g = 0, b = 0;
    for (const c of box) {
      const [cr, cg, cb] = rgb565ToRgb(c);
      n += hist[c];
      r += cr * hist[c];
      g += cg * hist[c];
      b += cb * hist[c];
    }
    return to565(r / n, g / n, b / n);
  });
  const prgb = palette.map(rgb565ToRgb);
  for (const c of used) {
    const [r, g, b] = rgb565ToRgb(c);
    let bi = 0, bd = Infinity;
    prgb.forEach(([pr, pg, pb], i) => {
      const d = 2 * (r - pr) ** 2 + 4 * (g - pg) ** 2 + 3 * (b - pb) ** 2;
      if (d < bd) [bi, bd] = [i, d];
    });
    lut[c] = bi;
  }
  return { palette, lut };
}

/**
 * Packs the pixels of a rectangle: `cur` is what to show, `prev` what is on screen before it
 * (null for frame 0). Packets: a byte `kind << 6 | n` (n = 0: a u16 length follows); kind 0 = n
 * literal indices, 1 = one index repeated n times, 2 = skip n unchanged pixels.
 */
export function packPixels(cur, prev) {
  const out = [];
  const head = (kind, n) => (n < 64 ? out.push((kind << 6) | n) : out.push(kind << 6, n & 0xff, n >> 8));
  const same = (i) => prev !== null && prev[i] === cur[i];
  const MAX = 65535;
  let lit = [];
  const flush = () => {
    for (let i = 0; i < lit.length; i += MAX) {
      const chunk = lit.slice(i, i + MAX);
      head(0, chunk.length);
      for (const c of chunk) out.push(c);
    }
    lit = [];
  };
  for (let i = 0; i < cur.length; ) {
    let n = 1;
    if (same(i)) {
      while (n < MAX && i + n < cur.length && same(i + n)) n++;
      // a lone unchanged pixel inside changed ones is cheaper as a literal
      if (n >= 2 || lit.length === 0) {
        flush();
        head(2, n);
        i += n;
        continue;
      }
    }
    n = 1;
    while (n < MAX && i + n < cur.length && cur[i + n] === cur[i]) n++;
    if (n >= 3) {
      flush();
      head(1, n);
      out.push(cur[i]);
      i += n;
    } else {
      lit.push(cur[i++]);
    }
  }
  flush();
  return out;
}

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

export function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/**
 * Encodes frames ({ rgba: Uint8ClampedArray of W*H*4, delay: ms }) into a splash file, with at
 * most `maxColors` colours (2-256): fewer colours, smaller file. `bg` ("#rrggbb") is the colour
 * the hint sits on (light or dark text) and `textY` the hint's vertical centre, in pixels.
 * Returns { bytes, palette, frames: [{ indices, delay }], colors } — `frames` is what the deck
 * will show (identical frames merged), for the preview.
 */
export function encode(input, { maxColors = 256, bg = "#000000", textY = H - 48 } = {}) {
  if (!input.length) throw new Error("no frames");
  const hist = new Uint32Array(65536);
  const c565 = input.map(({ rgba }) => {
    const out = new Uint16Array(W * H);
    for (let i = 0, p = 0; i < out.length; i++, p += 4) {
      const c = to565(rgba[p], rgba[p + 1], rgba[p + 2]);
      out[i] = c;
      hist[c]++;
    }
    return out;
  });
  const { palette, lut } = buildPalette(hist, Math.max(2, Math.min(256, maxColors)));

  // index every frame, merging runs of identical frames into one longer frame
  const frames = [];
  input.forEach(({ delay }, k) => {
    const indices = new Uint8Array(W * H);
    const src = c565[k];
    for (let i = 0; i < indices.length; i++) indices[i] = lut[src[i]];
    const prev = frames[frames.length - 1];
    const d = Math.round(Math.min(MAX_DELAY_MS, Math.max(MIN_DELAY_MS, delay || 100)));
    if (prev && sameAs(prev.indices, indices)) prev.delay = Math.min(MAX_DELAY_MS, prev.delay + d);
    else frames.push({ indices, delay: d });
  });

  const chunks = [];
  const u16 = (v) => chunks.push(v & 0xff, (v >> 8) & 0xff);
  const u32 = (v) => chunks.push(v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff);
  for (const c of palette) u16(c);
  frames.forEach(({ indices, delay }, k) => {
    const [x, y, w, h] = k === 0 ? [0, 0, W, H] : changed(frames[k - 1].indices, indices);
    const crop = (src) => {
      const px = new Uint8Array(w * h);
      for (let r = 0; r < h; r++) px.set(src.subarray((y + r) * W + x, (y + r) * W + x + w), r * w);
      return px;
    };
    const data = packPixels(crop(indices), k === 0 ? null : crop(frames[k - 1].indices));
    u16(delay), u16(x), u16(y), u16(w), u16(h), u32(data.length);
    for (const b of data) chunks.push(b);
  });
  const body = Uint8Array.from(chunks);

  const bytes = new Uint8Array(HEADER_LEN + body.length);
  const v = new DataView(bytes.buffer);
  const half = Math.max(1, Math.min(255, Math.round(textY / 2))); // byte 5: hint centre / 2
  bytes.set([0x44, 0x4b, 0x53, 0x50, 1, half]); // "DKSP", version 1
  v.setUint16(6, W, true);
  v.setUint16(8, H, true);
  v.setUint16(10, frames.length, true);
  v.setUint16(12, palette.length, true);
  const [br, bgr, bb] = [1, 3, 5].map((i) => parseInt(bg.slice(i, i + 2), 16));
  v.setUint16(14, to565(br, bgr, bb), true);
  v.setUint32(16, body.length, true);
  v.setUint32(20, crc32(body), true);
  bytes.set(body, HEADER_LEN);
  return { bytes, palette, frames, colors: palette.length };
}

function sameAs(a, b) {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** Bounding box [x, y, w, h] of the pixels that differ (1×1 if none). */
function changed(a, b) {
  let x0 = W, y0 = H, x1 = -1, y1 = -1;
  for (let y = 0; y < H; y++) {
    for (let x = 0, i = y * W; x < W; x++, i++) {
      if (a[i] !== b[i]) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        y1 = y;
      }
    }
  }
  return x1 < 0 ? [0, 0, 1, 1] : [x0, y0, x1 - x0 + 1, y1 - y0 + 1];
}
