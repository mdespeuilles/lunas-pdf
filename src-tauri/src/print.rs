//! Impression native : dialogue d'impression du système, pages rendues par PDFium (état
//! courant du document, annotations et champs compris). Pas `webview.print()`, qui
//! imprimerait l'interface.

#[cfg(not(target_os = "macos"))]
use lunas_pdf_core::{DocId, Engine, Error, Result};

/// Résolution d'impression maximale (au-delà, le rendu coûte sans gain visible).
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_DPI: f64 = 300.0;

#[cfg(target_os = "linux")]
pub fn print(window: &tauri::WebviewWindow, engine: Engine, doc: DocId, name: String, pages: Vec<(f32, f32)>) -> Result<bool> {
    use gtk::prelude::*;
    use gtk::{PageOrientation, PrintOperation, PrintOperationAction, PrintOperationResult, cairo};
    use lunas_pdf_core::RenderRequest;

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

#[cfg(target_os = "macos")]
pub use mac::print;

/// macOS : `NSPrintOperation` sur une vue qui fournit sa propre pagination (une page
/// imprimée par page du document) et dessine chaque page rendue par PDFium.
#[cfg(target_os = "macos")]
mod mac {
    use std::cell::Cell;

    use lunas_pdf_core::{Bitmap, DocId, Engine, Error, RenderRequest, Result};
    use objc2::rc::Retained;
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSGraphicsContext, NSPaperOrientation, NSPrintInfo, NSPrintOperation, NSView};
    use objc2_core_foundation::{CFData, CFRetained, CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext, CGDataProvider, CGImage, CGImageAlphaInfo,
    };
    use objc2_foundation::{NSCopying, NSInteger, NSRange, NSString};

    use super::MAX_DPI;

    pub struct Ivars {
        engine: Engine,
        doc: DocId,
        pages: Vec<(f32, f32)>,
        /// Zone imprimable du papier choisi (taille d'une page dans la vue).
        cell: Cell<CGSize>,
    }

    define_class!(
        // SAFETY : NSView peut être sous-classée ; aucune méthode n'impose d'invariant de plus.
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[ivars = Ivars]
        struct PrintView;

        impl PrintView {
            #[unsafe(method(knowsPageRange:))]
            fn knows_page_range(&self, range: *mut NSRange) -> bool {
                self.layout();
                // SAFETY : AppKit passe un pointeur valide vers la plage à remplir.
                unsafe { *range = NSRange::new(1, self.ivars().pages.len()) };
                true
            }

            #[unsafe(method(rectForPage:))]
            fn rect_for_page(&self, page: NSInteger) -> CGRect {
                self.cell_rect((page.max(1) - 1) as usize)
            }

            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, dirty: CGRect) {
                self.draw(dirty);
            }
        }
    );

    impl PrintView {
        fn new(mtm: MainThreadMarker, engine: Engine, doc: DocId, pages: Vec<(f32, f32)>) -> Retained<Self> {
            let letter = CGSize::new(612.0, 792.0);
            let frame = CGRect::new(CGPoint::ZERO, CGSize::new(letter.width, letter.height * pages.len() as f64));
            let this = Self::alloc(mtm).set_ivars(Ivars {
                engine,
                doc,
                pages,
                cell: Cell::new(letter),
            });
            // SAFETY : initialiseur désigné de NSView.
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }

        /// Une cellule par page, de la taille de la zone imprimable ; marges = bords non
        /// imprimables. Recalculé à chaque changement de papier dans le dialogue.
        fn layout(&self) {
            let Some(op) = NSPrintOperation::currentOperation(self.mtm()) else {
                return;
            };
            let info = op.printInfo();
            let paper = info.paperSize();
            let b = info.imageablePageBounds();
            info.setLeftMargin(b.origin.x);
            info.setBottomMargin(b.origin.y);
            info.setRightMargin(paper.width - b.origin.x - b.size.width);
            info.setTopMargin(paper.height - b.origin.y - b.size.height);
            self.ivars().cell.set(b.size);
            self.setFrameSize(CGSize::new(b.size.width, b.size.height * self.ivars().pages.len() as f64));
        }

        /// Vue non retournée : la première page est en haut.
        fn cell_rect(&self, i: usize) -> CGRect {
            let c = self.ivars().cell.get();
            let n = self.ivars().pages.len();
            CGRect::new(CGPoint::new(0.0, c.height * n.saturating_sub(i + 1) as f64), c)
        }

        fn draw(&self, dirty: CGRect) {
            let Some(gc) = NSGraphicsContext::currentContext() else {
                return;
            };
            let cg = gc.CGContext();
            let iv = self.ivars();
            for (i, &(pw, ph)) in iv.pages.iter().enumerate() {
                let cell = self.cell_rect(i);
                if !intersects(cell, dirty) {
                    continue;
                }
                // Page ajustée à la zone imprimable, centrée, proportions conservées.
                let scale = (cell.size.width / pw as f64).min(cell.size.height / ph as f64);
                let (dw, dh) = (pw as f64 * scale, ph as f64 * scale);
                let dpi = MAX_DPI / 72.0 * scale;
                let Ok(bmp) = iv.engine.render(RenderRequest {
                    doc: iv.doc,
                    page: i as u32,
                    width: (pw as f64 * dpi).round().max(1.0) as u32,
                    height: (ph as f64 * dpi).round().max(1.0) as u32,
                    tile: None,
                    priority: 255,
                    epoch: u32::MAX,
                }) else {
                    continue;
                };
                let Some(img) = image(&bmp) else { continue };
                let at = CGPoint::new(
                    cell.origin.x + (cell.size.width - dw) / 2.0,
                    cell.origin.y + (cell.size.height - dh) / 2.0,
                );
                CGContext::draw_image(Some(&cg), CGRect::new(at, CGSize::new(dw, dh)), Some(&img));
            }
        }
    }

    fn intersects(a: CGRect, b: CGRect) -> bool {
        a.origin.x < b.origin.x + b.size.width
            && b.origin.x < a.origin.x + a.size.width
            && a.origin.y < b.origin.y + b.size.height
            && b.origin.y < a.origin.y + a.size.height
    }

    /// RGBA opaque (fond blanc) → CGImage.
    fn image(bmp: &Bitmap) -> Option<CFRetained<CGImage>> {
        let data = CFData::from_bytes(&bmp.rgba);
        let provider = CGDataProvider::with_cf_data(Some(&data))?;
        let space = CGColorSpace::new_device_rgb()?;
        let (w, h) = (bmp.width as usize, bmp.height as usize);
        // SAFETY : `rgba` contient w × h pixels de 4 octets, sans remplissage.
        unsafe {
            CGImage::new(
                w,
                h,
                8,
                32,
                w * 4,
                Some(&space),
                CGBitmapInfo(CGImageAlphaInfo::NoneSkipLast.0),
                Some(&provider),
                std::ptr::null(),
                true,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
    }

    pub fn print(
        _window: &tauri::WebviewWindow,
        engine: Engine,
        doc: DocId,
        name: String,
        pages: Vec<(f32, f32)>,
    ) -> Result<bool> {
        let mtm = MainThreadMarker::new().ok_or_else(|| Error::Engine("impression hors du fil principal".into()))?;
        let Some(&(w, h)) = pages.first() else { return Ok(false) };
        let info = NSPrintInfo::sharedPrintInfo().copy();
        info.setOrientation(if w > h {
            NSPaperOrientation::Landscape
        } else {
            NSPaperOrientation::Portrait
        });
        let view = PrintView::new(mtm, engine, doc, pages);
        let op = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
        op.setJobTitle(Some(&NSString::from_str(&name)));
        op.setShowsPrintPanel(true);
        op.setShowsProgressPanel(true);
        Ok(op.runOperation())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn print(
    _window: &tauri::WebviewWindow,
    _engine: Engine,
    _doc: DocId,
    _name: String,
    _pages: Vec<(f32, f32)>,
) -> Result<bool> {
    Err(Error::Engine("impression native non disponible sur ce système".into()))
}
