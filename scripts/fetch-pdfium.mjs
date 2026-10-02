// Télécharge le binaire PDFium (bblanchon/pdfium-binaries) pour la plateforme courante
// dans src-tauri/pdfium/. La version est épinglée sur celle de l'API liée par pdfium-render
// (feature `pdfium_7881`) : voir docs/ADR-001-moteur-pdf.md.
//
// Usage : bun scripts/fetch-pdfium.mjs [plateforme]   (ex. linux-x64, mac-arm64, win-x64)
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const PDFIUM_BUILD = "7881";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dest = join(root, "src-tauri", "pdfium");

function platform() {
  const arch = { x64: "x64", arm64: "arm64" }[process.arch];
  const os = { linux: "linux", darwin: "mac", win32: "win" }[process.platform];
  if (!arch || !os) throw new Error(`plateforme non prise en charge : ${process.platform}-${process.arch}`);
  return `${os}-${arch}`;
}

const target = process.argv[2] ?? platform();
const stamp = join(dest, ".version");
if (existsSync(stamp) && readFileSync(stamp, "utf8").trim() === `${PDFIUM_BUILD} ${target}`) {
  console.log(`PDFium ${PDFIUM_BUILD} (${target}) déjà présent`);
  process.exit(0);
}

const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_BUILD}/pdfium-${target}.tgz`;
console.log(`Téléchargement de ${url}`);
const res = await fetch(url);
if (!res.ok) throw new Error(`échec du téléchargement : HTTP ${res.status}`);
rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });
const archive = join(dest, "pdfium.tgz");
writeFileSync(archive, Buffer.from(await res.arrayBuffer()));
execFileSync("tar", ["-xzf", archive, "-C", dest], { stdio: "inherit" });
rmSync(archive);
writeFileSync(stamp, `${PDFIUM_BUILD} ${target}\n`);
console.log(`PDFium installé dans ${dest}`);
