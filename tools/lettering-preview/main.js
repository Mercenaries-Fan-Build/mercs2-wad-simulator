// Every letter SVG, read raw; Vite reloads the page when one changes on disk.
const files = import.meta.glob("../../crates/mercs2_quartermaster/data/lettering/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
});

// The menu's lettering: a 27-texel cap height, 1000 SVG units; every letter's SVG has 10 texels of room above
// the cap (accents) and 6 below (a cedilla).
const CAP_TEXELS = 27;
const UNITS_PER_TEXEL = 1000 / CAP_TEXELS;
const ABOVE = 10 * UNITS_PER_TEXEL;
const BELOW = 6 * UNITS_PER_TEXEL;

// Measured from retail shell_MAINMENU_text (qm::label_art): each state's fill and outline ring colours.
const STATES = {
  dark: { fill: "rgb(24,23,18)", ring: "rgb(196,194,163)" },
  bright: { fill: "rgb(226,224,214)", ring: "rgb(32,30,0)" },
};

const DEFAULTS = { text: "CONTINUE", zoom: 6, gap: 5, space: 22, ring: 4.5, retail: true };
const settings = { ...DEFAULTS, ...JSON.parse(localStorage.getItem("lettering-preview") ?? "{}") };

// char -> { width (units), body (the SVG's inner markup), file }
const glyphs = new Map();
for (const [path, text] of Object.entries(files)) {
  const m = path.match(/U\+([0-9A-Fa-f]{4,6})\.svg$/);
  if (!m) continue;
  const doc = new DOMParser().parseFromString(text, "image/svg+xml");
  const svg = doc.documentElement;
  const vb = (svg.getAttribute("viewBox") ?? "0 0 0 1000").split(/[\s,]+/).map(Number);
  glyphs.set(String.fromCodePoint(parseInt(m[1], 16)), { width: vb[2], body: svg.innerHTML, file: path.split("/").pop() });
}

const $ = (id) => document.getElementById(id);

// The label as one SVG string: letters left to right, `gap` texels apart, a space `space` texels wide; the
// ring is every letter's outline stroked round, under every fill.
function labelSvg(text, state, opts) {
  const ring = opts.ring * UNITS_PER_TEXEL;
  const pad = ring + 2 * UNITS_PER_TEXEL;
  let x = pad;
  const placed = [];
  const missing = new Set();
  for (const ch of text) {
    if (ch === " ") {
      x += opts.space * UNITS_PER_TEXEL;
      continue;
    }
    const g = glyphs.get(ch) ?? glyphs.get(ch.toUpperCase());
    if (!g) {
      missing.add(ch);
      continue;
    }
    placed.push({ g, x });
    x += g.width + opts.gap * UNITS_PER_TEXEL;
  }
  if (placed.length) x -= opts.gap * UNITS_PER_TEXEL;
  const width = Math.max(x + pad, 1);
  const top = pad + ABOVE;
  const height = top + 1000 + BELOW + pad;
  const colours = STATES[state];
  const layer = (kind) =>
    placed
      .map(({ g, x }) => {
        const paint =
          kind === "ring"
            ? `fill="${colours.ring}" stroke="${colours.ring}" stroke-width="${2 * ring}" stroke-linejoin="round"`
            : `fill="${colours.fill}"`;
        // Each letter's own paints are replaced by the layer's.
        const body = g.body.replace(/\s(fill|stroke|stroke-width|stroke-linejoin)="[^"]*"/g, "").replace(/<path/g, `<path ${paint}`);
        return `<g transform="translate(${x} ${top})">${body}</g>`;
      })
      .join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">${layer("ring")}${layer("fill")}</svg>`;
  return { svg, width, height, missing };
}

// `svg` drawn with its cap `CAP_TEXELS` pixels high, as a canvas magnified `zoom` times without smoothing.
async function pixels(svg, width, height, zoom) {
  const scale = CAP_TEXELS / 1000;
  const w = Math.ceil(width * scale);
  const h = Math.ceil(height * scale);
  const img = new Image();
  img.src = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  canvas.getContext("2d").drawImage(img, 0, 0, w, h);
  URL.revokeObjectURL(img.src);
  canvas.className = "pixels";
  canvas.style.width = `${w * zoom}px`;
  canvas.style.height = `${h * zoom}px`;
  return canvas;
}

// A retail reference image if `mercs2_probe label-crops` wrote one into public/retail, else null.
async function retail(name, zoom) {
  const res = await fetch(`/retail/${name}`, { method: "HEAD" });
  if (!res.ok || !(res.headers.get("content-type") ?? "").startsWith("image/")) return null;
  const img = new Image();
  img.src = `/retail/${name}`;
  await img.decode();
  img.className = "pixels";
  img.style.width = `${img.naturalWidth * zoom}px`;
  return img;
}

function card(child, caption) {
  const div = document.createElement("div");
  div.className = "card";
  if (typeof child === "string") div.innerHTML = child;
  else div.append(child);
  const c = document.createElement("div");
  c.className = "caption";
  c.textContent = caption;
  div.append(c);
  return div;
}

let generation = 0;
async function render() {
  const run = ++generation;
  localStorage.setItem("lettering-preview", JSON.stringify(settings));
  $("zoom-v").textContent = `${settings.zoom}x`;
  const text = settings.text;
  const vector = $("vector");
  const px = $("pixels");
  const letters = $("letters");
  const vectorCards = [];
  const pixelCards = [];
  let missing = new Set();
  for (const state of Object.keys(STATES)) {
    const label = labelSvg(text, state, settings);
    label.missing.forEach((c) => missing.add(c));
    const scaled = label.svg.replace(/width="[^"]*" height="[^"]*"/, `height="${(label.height * 160) / 1000}"`);
    vectorCards.push(card(scaled, `${state}`));
    pixelCards.push(card(await pixels(label.svg, label.width, label.height, settings.zoom), `${state}, ours`));
    if (settings.retail) {
      const slug = text.replace(/[^\p{L}\p{N}]/gu, "_");
      const ref = await retail(`label-${slug}.${state}.png`, settings.zoom);
      if (ref) pixelCards.push(card(ref, `${state}, retail`));
    }
  }
  const letterCards = [];
  const chars = [...glyphs.keys()].sort((a, b) => a.codePointAt(0) - b.codePointAt(0));
  for (const ch of chars) {
    const g = glyphs.get(ch);
    const box = document.createElement("div");
    const big = labelSvg(ch, "dark", settings);
    box.innerHTML = big.svg.replace(/width="[^"]*" height="[^"]*"/, `height="120"`);
    const row = document.createElement("div");
    row.className = "row";
    for (const state of Object.keys(STATES)) {
      const l = labelSvg(ch, state, settings);
      row.append(await pixels(l.svg, l.width, l.height, 4));
      if (settings.retail) {
        const ref = await retail(`U+${ch.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}.${state}.png`, 4);
        if (ref) row.append(ref);
      }
    }
    box.append(row);
    letterCards.push(card(box, `${ch}  ${g.file}  ${(g.width / UNITS_PER_TEXEL).toFixed(1)} tx`));
  }
  if (run !== generation) return;
  vector.replaceChildren(...vectorCards);
  if (missing.size) {
    const m = document.createElement("div");
    m.className = "missing";
    m.textContent = `no letter for: ${[...missing].join(" ")}`;
    vector.append(m);
  }
  px.replaceChildren(...pixelCards);
  letters.replaceChildren(...letterCards);
}

for (const [id, key, read] of [
  ["text", "text", (e) => e.value],
  ["zoom", "zoom", (e) => Number(e.value)],
  ["gap", "gap", (e) => Number(e.value)],
  ["space", "space", (e) => Number(e.value)],
  ["ring", "ring", (e) => Number(e.value)],
  ["retail", "retail", (e) => e.checked],
]) {
  const el = $(id);
  if (el.type === "checkbox") el.checked = settings[key];
  else el.value = settings[key];
  el.addEventListener("input", () => {
    settings[key] = read(el);
    render();
  });
}

// A changed letter reloads the page (Vite's default for a module that does not accept updates); the
// settings above survive it in localStorage.
render();
