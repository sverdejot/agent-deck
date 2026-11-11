// "Start screen" section of the flasher: load a picture or animation, lay it out with the deck's
// hint below it, preview it as the deck will show it and write it to the `splash` partition.
import {
  encode, W, H, TEXT_H, PARTITION_OFFSET, PARTITION_SIZE, WRITE_RATE, rgb565ToRgb,
} from "./splash.js";

const ESPTOOL = "https://unpkg.com/esptool-js@0.6.1/lib/index.js?module";
const MAX_FRAMES = 400;
/** Starting zoom: a little under the most the layout allows, so it breathes. */
const DEFAULT_ZOOM = 80;
/** Layout: picture, GAP, hint, as one block centred on the screen, at least MARGIN from the edges. */
const MARGIN = 24;
const GAP = 20;
const MAX_PIC_H = H - 2 * MARGIN - GAP - TEXT_H;
const CH340 = 0x1a86; // the Indicator's USB-serial chip
const $ = (id) => document.getElementById(id);

const ui = {
  file: $("ss-file"), drop: $("ss-drop"), preview: $("ss-preview"), controls: $("ss-controls"),
  zoom: $("ss-zoom"), zoomOut: $("ss-zoom-out"), bg: $("ss-bg"), colors: $("ss-colors"),
  center: $("ss-center"), stats: $("ss-stats"), write: $("ss-write"), download: $("ss-download"),
  reset: $("ss-reset"), progress: $("ss-progress"), status: $("ss-status"),
};
const ctx = ui.preview.getContext("2d");

/** Source frames as ImageBitmaps + delay (ms); the transform the user set; the last encoding. */
let source = [];
let view = { zoom: DEFAULT_ZOOM / 100, dx: 0, dy: 0, bg: "#000000" };
let result = null;
let busy = false;

// ---- loading ------------------------------------------------------------------------------

async function decodeFile(file) {
  const type = file.type || "image/gif";
  if ("ImageDecoder" in window && (await ImageDecoder.isTypeSupported(type))) {
    const dec = new ImageDecoder({ data: await file.arrayBuffer(), type });
    await dec.tracks.ready;
    await dec.completed; // the frame count is only final once everything is parsed
    const n = dec.tracks.selectedTrack?.frameCount ?? 1;
    if (n > MAX_FRAMES) throw new Error(`${n} frames: at most ${MAX_FRAMES}. Trim the animation first.`);
    const frames = [];
    for (let i = 0; i < n; i++) {
      const { image } = await dec.decode({ frameIndex: i });
      frames.push({ bitmap: await createImageBitmap(image), delay: (image.duration ?? 100_000) / 1000 });
      image.close();
    }
    dec.close();
    return frames;
  }
  return [{ bitmap: await createImageBitmap(file), delay: 0 }]; // no ImageDecoder: first frame only
}

async function load(file) {
  if (!file) return;
  status(`Reading ${file.name}…`);
  try {
    source = await decodeFile(file);
  } catch (e) {
    status(`Can't read ${file.name}: ${e.message}`, true);
    return;
  }
  view = { zoom: DEFAULT_ZOOM / 100, dx: 0, dy: 0, bg: cornerColor(source[0].bitmap) };
  ui.bg.value = view.bg;
  ui.zoom.value = DEFAULT_ZOOM;
  ui.zoomOut.textContent = `${DEFAULT_ZOOM}%`;
  ui.controls.hidden = false;
  ui.drop.hidden = true;
  status("");
  await refresh();
}

/** Top-left pixel: usually the background, so the margins blend in. */
function cornerColor(bitmap) {
  const c = new OffscreenCanvas(1, 1).getContext("2d");
  c.drawImage(bitmap, 0, 0, 1, 1, 0, 0, 1, 1);
  const [r, g, b] = c.getImageData(0, 0, 1, 1).data;
  return "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
}

// ---- fitting and encoding -----------------------------------------------------------------

/**
 * Where the picture goes ([x, y, w, h]) and where the deck centres its hint (textY): the picture
 * and the hint below it form one block, centred on the screen; dragging moves the block.
 */
function layout(bitmap) {
  const s = Math.min(W / bitmap.width, MAX_PIC_H / bitmap.height) * view.zoom;
  const w = bitmap.width * s, h = bitmap.height * s;
  const top = (H - (h + GAP + TEXT_H)) / 2 + view.dy;
  const textY = Math.round(Math.max(TEXT_H / 2, Math.min(H - TEXT_H / 2, top + h + GAP + TEXT_H / 2)));
  return { pic: [(W - w) / 2 + view.dx, top, w, h], textY };
}

/** Draws a source frame on the background, where the layout puts it. */
function drawSource(c, bitmap) {
  c.fillStyle = view.bg;
  c.fillRect(0, 0, W, H);
  c.imageSmoothingQuality = "high";
  c.drawImage(bitmap, ...layout(bitmap).pic);
}

let pending = 0;
async function refresh() {
  if (!source.length) return;
  const ticket = ++pending;
  stopPreview();
  status("Converting…");
  await new Promise((r) => setTimeout(r, 0)); // let the status paint
  const work = new OffscreenCanvas(W, H).getContext("2d", { willReadFrequently: true });
  const frames = source.map(({ bitmap, delay }) => {
    drawSource(work, bitmap);
    return { rgba: work.getImageData(0, 0, W, H).data, delay };
  });
  const { textY } = layout(source[0].bitmap);
  const r = encode(frames, { maxColors: +ui.colors.value, bg: view.bg, textY });
  if (ticket !== pending) return; // a newer change is on its way
  result = { ...r, textY, images: r.frames.map((f) => toImageData(r.palette, f.indices)) };
  status("");
  await showStats();
  startPreview();
}

function toImageData(palette, indices) {
  const rgb = palette.map(rgb565ToRgb);
  const img = new ImageData(W, H);
  for (let i = 0, p = 0; i < indices.length; i++, p += 4) {
    const [r, g, b] = rgb[indices[i]];
    img.data[p] = r, img.data[p + 1] = g, img.data[p + 2] = b, img.data[p + 3] = 255;
  }
  return img;
}

async function deflatedSize(bytes) {
  if (!("CompressionStream" in window)) return bytes.length;
  const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream("deflate"));
  return (await new Response(stream).arrayBuffer()).byteLength;
}

async function showStats() {
  const { bytes, frames, colors } = result;
  const kb = (n) => `${Math.round(n / 1024).toLocaleString()} KB`;
  const fits = (result.fits = bytes.length <= PARTITION_SIZE);
  const secs = Math.round((await deflatedSize(bytes)) / WRITE_RATE) + 5;
  const time = secs < 90 ? `about ${secs} s` : `about ${Math.round(secs / 60)} min`;
  ui.stats.textContent = `${frames.length} frame${frames.length > 1 ? "s" : ""} · ${colors} colours · ` +
    `${kb(bytes.length)} of ${kb(PARTITION_SIZE)}` + (fits ? ` · ${time} to write` : "");
  ui.stats.classList.toggle("bad", !fits);
  ui.write.disabled = ui.download.disabled = !fits;
  if (!fits) status("Too big for the deck: use fewer colours or a shorter animation.", true);
}

// ---- preview ------------------------------------------------------------------------------

let timer = 0;
function startPreview() {
  let k = 0;
  const step = () => {
    paint(result.images[k]);
    if (result.frames.length > 1) {
      timer = setTimeout(step, result.frames[k].delay);
      k = (k + 1) % result.frames.length;
    }
  };
  step();
}
function stopPreview() {
  clearTimeout(timer);
}

/** The hint, centred on y, as the deck draws it (crates/deck-ui/src/ui/offline.rs). */
function paintHint(y) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(view.bg.slice(i, i + 2), 16));
  const dark = 299 * r + 587 * g + 114 * b < 140_000;
  ctx.font = '500 15px "JetBrains Mono", ui-monospace, monospace';
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = dark ? "rgb(245, 242, 236)" : "rgb(40, 38, 36)";
  ctx.fillText("Connect the deck to the computer", W / 2, y - 13);
  ctx.fillStyle = dark ? "rgb(160, 156, 148)" : "rgb(105, 100, 92)";
  ctx.fillText("and run agent-deck", W / 2, y + 13);
}

function paint(img) {
  ctx.putImageData(img, 0, 0);
  paintHint(result.textY);
}

/** Quick unconverted preview of the first frame, while dragging or zooming. */
function sketch() {
  drawSource(ctx, source[0].bitmap);
  paintHint(layout(source[0].bitmap).textY);
}

// dragging moves the picture; a quick unconverted preview follows the finger
let drag = null;
ui.preview.addEventListener("pointerdown", (e) => {
  if (!source.length || busy) return;
  ui.preview.setPointerCapture(e.pointerId);
  stopPreview();
  drag = { x: e.clientX, y: e.clientY, dx: view.dx, dy: view.dy };
});
ui.preview.addEventListener("pointermove", (e) => {
  if (!drag) return;
  const k = W / ui.preview.getBoundingClientRect().width; // CSS px → screen px
  view.dx = drag.dx + (e.clientX - drag.x) * k;
  view.dy = drag.dy + (e.clientY - drag.y) * k;
  sketch();
});
ui.preview.addEventListener("pointerup", () => {
  if (!drag) return;
  drag = null;
  refresh();
});

// ---- controls -----------------------------------------------------------------------------

let debounce = 0;
const later = () => {
  clearTimeout(debounce);
  debounce = setTimeout(refresh, 200);
};
ui.file.addEventListener("change", () => load(ui.file.files[0]));
for (const ev of ["dragover", "dragenter"]) {
  document.getElementById("ss-card").addEventListener(ev, (e) => {
    e.preventDefault();
    ui.drop.classList.add("over");
  });
}
document.getElementById("ss-card").addEventListener("dragleave", () => ui.drop.classList.remove("over"));
document.getElementById("ss-card").addEventListener("drop", (e) => {
  e.preventDefault();
  ui.drop.classList.remove("over");
  load(e.dataTransfer.files[0]);
});
ui.zoom.addEventListener("input", () => {
  view.zoom = ui.zoom.value / 100;
  ui.zoomOut.textContent = `${ui.zoom.value}%`;
  if (source.length) sketch();
  later();
});
ui.bg.addEventListener("input", () => {
  view.bg = ui.bg.value;
  later();
});
ui.colors.addEventListener("change", refresh);
ui.center.addEventListener("click", () => {
  view.dx = view.dy = 0;
  refresh();
});
ui.download.addEventListener("click", () => {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([result.bytes], { type: "application/octet-stream" }));
  a.download = "agent-deck-start-screen.bin";
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
});
ui.write.addEventListener("click", () => flash(result.bytes, "Start screen written. The deck restarts with it."));
ui.reset.addEventListener("click", () =>
  flash(new Uint8Array(4096).fill(0xff), "Default start screen restored. The deck restarts with it."));

// ---- writing to the deck --------------------------------------------------------------------

/** Reads the partition table and checks the deck has the `splash` partition where we write. */
async function checkPartition(loader) {
  const table = await loader.readFlash(0x8000, 0xc00);
  const v = new DataView(table.buffer, table.byteOffset);
  for (let at = 0; at + 32 <= table.length && v.getUint16(at, true) === 0x50aa; at += 32) {
    const label = new TextDecoder().decode(table.subarray(at + 12, at + 28)).replace(/\0.*$/s, "");
    if (label === "splash") {
      if (v.getUint32(at + 4, true) === PARTITION_OFFSET) return;
      break;
    }
  }
  throw new Error("this deck's firmware has no room for a start screen. Install the latest firmware above first.");
}

async function flash(bytes, done) {
  if (!("serial" in navigator)) {
    status("This browser can't talk to the deck. Use Chrome or Edge on desktop.", true);
    return;
  }
  let port;
  try {
    port = await navigator.serial.requestPort({ filters: [{ usbVendorId: CH340 }] });
  } catch {
    return; // picker closed
  }
  setBusy(true);
  let transport;
  try {
    status("Connecting to the deck…");
    const { ESPLoader, Transport } = await import(ESPTOOL);
    transport = new Transport(port, false);
    const quiet = { clean() {}, write() {}, writeLine() {} };
    // 115200: the Indicator's CH340 drops data at higher rates
    const loader = new ESPLoader({ transport, baudrate: 115200, terminal: quiet });
    await loader.main();
    status("Checking the firmware…");
    await checkPartition(loader);
    status("Writing…");
    ui.progress.hidden = false;
    await loader.writeFlash({
      fileArray: [{ data: bytes, address: PARTITION_OFFSET }],
      flashMode: "keep", flashFreq: "keep", flashSize: "keep",
      eraseAll: false, compress: true,
      reportProgress: (_, written, total) => (ui.progress.value = written / total),
    });
    await loader.after("hard_reset");
    status(done);
  } catch (e) {
    const busyPort = /open|busy|access/i.test(e.message);
    status(busyPort ? "Can't open the port: quit agent-deck on your Mac (it holds the port) and try again."
      : `Couldn't write: ${e.message}`, true);
  } finally {
    ui.progress.hidden = true;
    ui.progress.value = 0;
    try {
      await transport?.disconnect();
    } catch {}
    setBusy(false);
  }
}

function setBusy(b) {
  busy = b;
  for (const el of [ui.write, ui.download, ui.reset, ui.file, ui.colors, ui.zoom, ui.bg, ui.center]) {
    el.disabled = b || ((el === ui.write || el === ui.download) && !result?.fits);
  }
}

function status(msg, bad = false) {
  ui.status.textContent = msg;
  ui.status.classList.toggle("bad", bad);
}
