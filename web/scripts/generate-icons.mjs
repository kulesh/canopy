/**
 * Generates PWA icons as self-contained SVG files.
 * Canopy icon: stylized tree canopy on dark background.
 */

import { writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const publicDir = join(__dirname, "..", "public");

function canopyIconSvg(size) {
  const r = (n) => Math.round(n);
  const s = size;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${s} ${s}" width="${s}" height="${s}">
  <rect width="${s}" height="${s}" rx="${r(s * 0.15)}" fill="#0f0f1a"/>
  <g transform="translate(${r(s / 2)}, ${r(s * 0.52)})">
    <rect x="${r(-s * 0.03)}" y="${r(s * 0.05)}" width="${r(s * 0.06)}" height="${r(s * 0.28)}" rx="${r(s * 0.015)}" fill="#7c6f5b"/>
    <ellipse cx="0" cy="${r(-s * 0.04)}" rx="${r(s * 0.32)}" ry="${r(s * 0.22)}" fill="#2d7a4f"/>
    <ellipse cx="${r(-s * 0.1)}" cy="${r(-s * 0.12)}" rx="${r(s * 0.22)}" ry="${r(s * 0.18)}" fill="#38a169"/>
    <ellipse cx="${r(s * 0.08)}" cy="${r(-s * 0.1)}" rx="${r(s * 0.2)}" ry="${r(s * 0.16)}" fill="#48bb78"/>
  </g>
</svg>`;
}

for (const size of [192, 512]) {
  const path = join(publicDir, `icon-${size}.svg`);
  writeFileSync(path, canopyIconSvg(size));
  console.log(`wrote ${path}`);
}

writeFileSync(join(publicDir, "favicon.svg"), canopyIconSvg(32));
console.log(`wrote ${join(publicDir, "favicon.svg")}`);
