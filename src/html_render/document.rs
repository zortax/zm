use std::sync::Arc;

use blitz_dom::BaseDocument;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};

use super::net::BlockingNetProvider;
use super::preprocess::preprocess_email_html;

pub struct PreparedDocument {
    pub doc: BaseDocument,
    pub content_height: f32,
    pub viewport_width: u32,
    pub scale: f64,
}

/// Parse HTML and resolve styles + layout. Intended to run on a background thread.
pub fn prepare_document(html: &str, viewport_width: u32, scale: f64) -> PreparedDocument {
    let viewport = Viewport::new(viewport_width, 4096, scale as f32, ColorScheme::Light);

    // Convert legacy HTML email attributes (bgcolor, width, <font>, etc.)
    // to inline CSS so blitz can render them.
    let html = preprocess_email_html(html);

    let mut doc = HtmlDocument::from_html(
        &html,
        blitz_dom::DocumentConfig {
            viewport: Some(viewport),
            net_provider: Some(Arc::new(BlockingNetProvider)),
            ..Default::default()
        },
    );

    // Initial resolve triggers image/resource fetches via the net provider.
    // Since BlockingNetProvider fetches synchronously, results are already
    // in the channel by the time resolve() returns.
    doc.resolve(0.0);

    // Process fetched resources (images, stylesheets)
    doc.handle_messages();

    // Re-resolve to layout with loaded images
    doc.resolve(0.0);

    let content_height = doc.root_element().final_layout.size.height;

    PreparedDocument {
        doc: doc.into_inner(),
        content_height,
        viewport_width,
        scale,
    }
}
