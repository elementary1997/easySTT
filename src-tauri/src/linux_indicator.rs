use gtk::prelude::*;

/// Clip the native window as well as the CSS pill. X11/XWayland then keeps
/// desktop pixels visible at the corners even without an alpha compositor.
/// The window interior stays opaque to avoid partial WebKitGTK alpha painting.
pub fn apply(window: &tauri::WebviewWindow) {
    let _ = window.with_webview(|webview| {
        let Some(toplevel) = webview
            .inner()
            .toplevel()
            .and_then(|widget| widget.downcast::<gtk::Window>().ok())
        else {
            return;
        };
        let Some(surface) = toplevel.window() else {
            return;
        };
        let region = gtk::cairo::Region::create();
        for y in 0..44 {
            let dy = (y as f64 + 0.5 - 22.0).abs();
            let inset = (22.0 - (22.0_f64.powi(2) - dy.powi(2)).sqrt()).ceil() as i32;
            let _ = region.union_rectangle(&gtk::cairo::RectangleInt::new(
                inset,
                y,
                180 - 2 * inset,
                1,
            ));
        }
        surface.shape_combine_region(Some(&region), 0, 0);
        surface.input_shape_combine_region(&region, 0, 0);
    });
}
