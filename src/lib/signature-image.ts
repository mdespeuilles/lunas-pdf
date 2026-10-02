// Images de signature : recadrage, fond blanc retiré, texte manuscrit, export PNG.

/** Taille de rendu : 4 px par point PDF pour une signature nette à l'impression. */
export const SIG_SCALE = 4;

/** Recadre sur les pixels non transparents (marge `pad` px) ; null si l'image est vide. */
export function trim(src: HTMLCanvasElement, pad = 8): HTMLCanvasElement | null {
  const ctx = src.getContext("2d")!;
  const { width: w, height: h } = src;
  const d = ctx.getImageData(0, 0, w, h).data;
  let x0 = w, y0 = h, x1 = -1, y1 = -1;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (d[(y * w + x) * 4 + 3] > 8) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        if (y > y1) y1 = y;
      }
    }
  }
  if (x1 < 0) return null;
  x0 = Math.max(0, x0 - pad);
  y0 = Math.max(0, y0 - pad);
  x1 = Math.min(w - 1, x1 + pad);
  y1 = Math.min(h - 1, y1 + pad);
  const out = document.createElement("canvas");
  out.width = x1 - x0 + 1;
  out.height = y1 - y0 + 1;
  out.getContext("2d")!.drawImage(src, x0, y0, out.width, out.height, 0, 0, out.width, out.height);
  return out;
}

/** Rend transparents les pixels proches du blanc (fond d'un scan), avec un fondu. */
export function removeWhite(c: HTMLCanvasElement, threshold = 225): void {
  const ctx = c.getContext("2d")!;
  const img = ctx.getImageData(0, 0, c.width, c.height);
  const d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const lum = 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
    if (lum >= 250) d[i + 3] = 0;
    else if (lum > threshold) d[i + 3] = Math.round((d[i + 3] * (250 - lum)) / (250 - threshold));
  }
  ctx.putImageData(img, 0, 0);
}

export async function toPng(c: HTMLCanvasElement): Promise<Uint8Array> {
  const blob = await new Promise<Blob | null>((r) => c.toBlob(r, "image/png"));
  if (!blob) throw new Error("PNG");
  return new Uint8Array(await blob.arrayBuffer());
}

/** Image décodée à partir d'octets (PNG, JPEG, SVG). */
export async function loadImage(bytes: Uint8Array, name: string): Promise<HTMLImageElement> {
  const type = /\.svg$/i.test(name) ? "image/svg+xml" : /\.jpe?g$/i.test(name) ? "image/jpeg" : "image/png";
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    return img;
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}

/** Image dans un canevas (bornée à `max` px de large). */
export function imageCanvas(img: HTMLImageElement, max = 1600): HTMLCanvasElement {
  const w = img.naturalWidth || 600;
  const h = img.naturalHeight || 200;
  const s = Math.min(1, max / w);
  const c = document.createElement("canvas");
  c.width = Math.round(w * s);
  c.height = Math.round(h * s);
  c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
  return c;
}

export const TYPED_FONTS = [
  { family: "Allura", size: 34 },
  { family: "Caveat", size: 28 },
  { family: "Homemade Apple", size: 20 },
] as const;

/** Nom écrit dans une police manuscrite, encre `color`, recadré. */
export async function typedCanvas(name: string, family: string, color: string): Promise<HTMLCanvasElement | null> {
  const size = 48 * SIG_SCALE / 2;
  await document.fonts.load(`${size}px "${family}"`);
  const c = document.createElement("canvas");
  const ctx = c.getContext("2d")!;
  ctx.font = `${size}px "${family}"`;
  const w = Math.ceil(ctx.measureText(name).width) + size;
  c.width = Math.max(1, w);
  c.height = Math.ceil(size * 2.2);
  ctx.font = `${size}px "${family}"`;
  ctx.fillStyle = color;
  ctx.textBaseline = "middle";
  ctx.fillText(name, size / 2, c.height / 2);
  return trim(c, 6);
}
