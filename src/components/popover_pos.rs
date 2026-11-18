//! Positioning for the floating panels: event/day/slot details and the
//! "+N more" list (anchored at the pointer), and the month/year title menus
//! (anchored under their field).
//!
//! All of them are `position: fixed`, so they live in **viewport**
//! coordinates — which is exactly what `client_x`/`client_y` and
//! `getBoundingClientRect()` give us.
//!
//! **Never clamp against a guessed size.** The old helper took the panel's
//! width/height as arguments, and every call site passed a hardcoded guess
//! (e.g. 460 px for the day popup). When the real panel was shorter than the
//! guess it was pushed far from the cursor — the lower the anchor, the bigger
//! the error. Panels are now placed at the anchor first, then re-measured from
//! the DOM and flipped/clamped into the viewport.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

/// Gap between the anchor and the panel.
const GAP: f64 = 8.0;
/// Space kept to the viewport edges.
const MARGIN: f64 = 8.0;

/// What a panel hangs from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    /// A pointer position (`client_x`/`client_y`): open just below-right of it.
    Cursor(f64, f64),
    /// A field's rect `(left, right, bottom)`: open centred under it.
    Under(f64, f64, f64),
    /// No anchor (deep link / search result): centre horizontally near the top.
    Centered,
}

fn viewport() -> (f64, f64) {
    let win = web_sys::window();
    let w = win
        .as_ref()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(1200.0);
    let h = win
        .as_ref()
        .and_then(|w| w.inner_height().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    (w, h)
}

/// The rect of the element matching `selector`.
pub fn rect_of(selector: &str) -> Option<Anchor> {
    let doc = web_sys::window()?.document()?;
    let el = doc.query_selector(selector).ok()??;
    let el: HtmlElement = el.dyn_into().ok()?;
    let r = el.get_bounding_client_rect();
    Some(Anchor::Under(r.left(), r.right(), r.bottom()))
}

/// Place a `position: fixed` panel for `anchor`, using the panel's REAL size.
///
/// Call from an effect with the panel's `node_ref`: the element is laid out by
/// then (fonts/size known), so the measurement is exact. The panel is kept
/// below the anchor when it fits, flipped above it when it does not, and
/// clamped into the viewport only as a last resort.
pub fn place(node: &HtmlElement, anchor: Anchor) {
    let (vw, vh) = viewport();
    let r = node.get_bounding_client_rect();
    let w = (if r.width() > 1.0 { r.width() } else { node.offset_width() as f64 })
        .min(vw - 2.0 * MARGIN);
    let h = (if r.height() > 1.0 { r.height() } else { node.offset_height() as f64 })
        .min(vh - 2.0 * MARGIN);

    let left = match anchor {
        Anchor::Cursor(x, _) => x + GAP,
        Anchor::Under(l, right, _) => (l + right) / 2.0 - w / 2.0,
        Anchor::Centered => (vw - w) / 2.0,
    };
    let left = left.clamp(MARGIN, (vw - w - MARGIN).max(MARGIN));

    let top = match anchor {
        Anchor::Cursor(_, y) => {
            let below = y + GAP;
            if h <= vh - below - MARGIN {
                below
            } else if y - GAP - h >= MARGIN {
                y - GAP - h // flip above the cursor
            } else {
                (vh - h - MARGIN).max(MARGIN)
            }
        }
        Anchor::Under(_, _, bottom) => {
            let below = bottom + 6.0;
            if h <= vh - below - MARGIN {
                below
            } else {
                (vh - h - MARGIN).max(MARGIN)
            }
        }
        Anchor::Centered => (vh * 0.10).max(MARGIN),
    };

    let style = node.style();
    let _ = style.set_property("left", &format!("{left:.1}px"));
    let _ = style.set_property("top", &format!("{top:.1}px"));
    // the initial `left/top` in the markup are placeholders only
    let _ = style.set_property("visibility", "visible");
}

/// Position a panel from a `node_ref` as soon as it exists.
pub fn use_place(node: NodeRef<leptos::html::Div>, anchor: Anchor) {
    Effect::new(move |_| {
        if let Some(el) = node.get() {
            place(&el, anchor);
        }
    });
}
