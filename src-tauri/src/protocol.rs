//! Protocole `lunas-pdf://` : bitmaps des pages (RGBA brut) et miniatures des récents.
//!
//! - `lunas-pdf://localhost/render/<doc>/<page>?w=&h=[&tx=&ty=&tw=&th=]&p=<priorité>&e=<époque>`
//!   → octets RGBA, en-têtes `x-w` / `x-h` ; 204 si la demande est périmée.
//! - `lunas-pdf://localhost/thumb/<id>.png`
//!
//! Sous Windows, l'URL est `http://lunas-pdf.localhost/...` (même chemin).

use std::collections::HashMap;

use lunas_pdf_core::{Engine, Error, RenderRequest, Tile};
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};

use crate::store::Store;

fn reply(status: StatusCode, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap()
}

pub fn handle<R: tauri::Runtime>(ctx: UriSchemeContext<'_, R>, req: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = req.uri().path().to_string();
    let query: HashMap<String, String> = req
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let num = |k: &str| query.get(k).and_then(|v| v.parse::<u32>().ok());
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    match parts.as_slice() {
        ["render", doc, page] => {
            let (Ok(doc), Ok(page), Some(width), Some(height)) = (doc.parse(), page.parse(), num("w"), num("h")) else {
                return responder.respond(reply(StatusCode::BAD_REQUEST, vec![]));
            };
            // Garde-fou mémoire : 64 Mpx par bitmap.
            if width == 0 || height == 0 || (width as u64 * height as u64) > 64_000_000 {
                return responder.respond(reply(StatusCode::BAD_REQUEST, vec![]));
            }
            let tile = match (num("tx"), num("ty"), num("tw"), num("th")) {
                (Some(x), Some(y), Some(w), Some(h)) => Some(Tile { x, y, w, h }),
                _ => None,
            };
            let rr = RenderRequest {
                doc,
                page,
                width,
                height,
                tile,
                priority: num("p").unwrap_or(5).min(255) as u8,
                epoch: num("e").unwrap_or(0),
            };
            let engine = app.state::<Engine>().inner().clone();
            std::thread::spawn(move || {
                let res = match engine.render(rr) {
                    Ok(b) => Response::builder()
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Access-Control-Expose-Headers", "x-w, x-h")
                        .header("Content-Type", "application/octet-stream")
                        .header("x-w", b.width.to_string())
                        .header("x-h", b.height.to_string())
                        .body(b.rgba)
                        .unwrap(),
                    Err(Error::Cancelled) => reply(StatusCode::NO_CONTENT, vec![]),
                    Err(Error::UnknownDocument | Error::PageOutOfRange) => reply(StatusCode::NOT_FOUND, vec![]),
                    Err(e) => reply(StatusCode::INTERNAL_SERVER_ERROR, e.to_string().into_bytes()),
                };
                responder.respond(res);
            });
        }
        ["thumb", file] if file.ends_with(".png") && file[..file.len() - 4].chars().all(|c| c.is_ascii_hexdigit()) => {
            let p = app.state::<Store>().thumbs_dir.join(file);
            match std::fs::read(p) {
                Ok(bytes) => responder.respond(
                    Response::builder()
                        .header("Content-Type", "image/png")
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Cache-Control", "no-cache")
                        .body(bytes)
                        .unwrap(),
                ),
                Err(_) => responder.respond(reply(StatusCode::NOT_FOUND, vec![])),
            }
        }
        _ => responder.respond(reply(StatusCode::NOT_FOUND, vec![])),
    }
}
