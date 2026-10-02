//! Impression native : dialogue d'impression du système, pages rendues par PDFium (état
//! courant du document, annotations et champs compris). Pas `webview.print()`, qui
//! imprimerait l'interface.

use feuillet_core::{DocId, Engine, Error, RenderRequest, Result};

/// Résolution d'impression maximale (au-delà, le rendu coûte sans gain visible).
#[cfg(target_os = "linux")]
const MAX_DPI: f64 = 300.0;

#[cfg(target_os = "linux")]
pub fn print(window: &tauri::WebviewWindow, engine: Engine, doc: DocId, name: String, pages: Vec<(f32, f32)>) -> Result<bool> {
    use gtk::prelude::*;
    use gtk::{PageOrientation, PrintOperation, PrintOperationAction, PrintOperationResult, cairo};

    let parent = window.gtk_window().map_err(|e| Error::Engine(e.to_string()))?;
    let op = PrintOperation::new();
    op.set_job_name(&name);
    op.set_n_pages(pages.len() as i32);
    op.set_embed_page_setup(true);
    let sizes = pages.clone();
    op.connect_request_page_setup(move |_, _, page, setup| {
        if let Some((w, h)) = sizes.get(page as usize) {
            setup.set_orientation(if w > h {
                PageOrientation::Landscape
            } else {
                PageOrientation::Portrait
            });
        }
    });
    op.connect_draw_page(move |_, ctx, page| {
        let Some(&(pw, ph)) = pages.get(page as usize) else { return };
        let (aw, ah) = (ctx.width(), ctx.height());
        // Page ajustée à la zone imprimable, centrée, proportions conservées.
        let scale = (aw / pw as f64).min(ah / ph as f64);
        let dpi = (ctx.dpi_x().min(MAX_DPI) / 72.0) * scale;
        let (bw, bh) = (
            (pw as f64 * dpi).round().max(1.0) as u32,
            (ph as f64 * dpi).round().max(1.0) as u32,
        );
        let Ok(bmp) = engine.render(RenderRequest {
            doc,
            page: page as u32,
            width: bw,
            height: bh,
            tile: None,
            priority: 255,
            epoch: u32::MAX,
        }) else {
            return;
        };
        // RGBA → BGRA prémultiplié (fond blanc opaque : pas de prémultiplication à faire).
        let mut data = bmp.rgba;
        for px in data.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let stride = cairo::Format::ARgb32
            .stride_for_width(bmp.width)
            .unwrap_or(bmp.width as i32 * 4);
        let Ok(surface) =
            cairo::ImageSurface::create_for_data(data, cairo::Format::ARgb32, bmp.width as i32, bmp.height as i32, stride)
        else {
            return;
        };
        let Some(cr) = ctx.cairo_context() else { return };
        let (dw, dh) = (pw as f64 * scale, ph as f64 * scale);
        cr.translate((aw - dw) / 2.0, (ah - dh) / 2.0);
        cr.scale(dw / bmp.width as f64, dh / bmp.height as f64);
        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
        let _ = cr.paint();
    });
    match op.run(PrintOperationAction::PrintDialog, Some(&parent)) {
        Ok(PrintOperationResult::Apply) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(Error::Engine(e.to_string())),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn print(
    _window: &tauri::WebviewWindow,
    _engine: Engine,
    _doc: DocId,
    _name: String,
    _pages: Vec<(f32, f32)>,
) -> Result<bool> {
    Err(Error::Engine("impression native non disponible sur ce système".into()))
}
