// Accès typé au backend (types générés dans ../bindings.ts).
import { commands, type PdfError } from "../bindings";

export { commands };
export type { PdfError };

/** Hors de Tauri (tests e2e, aperçu navigateur), l'IPC est simulé. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window && !("__LUNAS_PDF_E2E__" in window);

export class BackendError extends Error {
  constructor(public readonly error: PdfError) {
    super("message" in error ? `${error.kind}: ${error.message}` : error.kind);
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: PdfError };

/** Déballe un résultat tauri-specta ; lève `BackendError` en cas d'erreur. */
export async function unwrap<T>(p: Promise<Res<T>>): Promise<T> {
  const r = await p;
  if (r.status === "error") throw new BackendError(r.error);
  return r.data;
}
