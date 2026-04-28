//! Windows-only: clip the widget `HWND` to a round-rect to match the web view
//! and avoid DWM / WebView2 leaving wrong pixels in the physical square corners.
// Must match `--widget-corners: 10px` when "round" (see widgetTheme + main).
const WIDGET_CORNER_RADIUS_CSS_PX: f64 = 10.0;

/// Returns the bottom of the work area (physical pixels from screen top) for the primary monitor.
/// This is the Y coordinate of the top edge of the taskbar (or full screen height if unavailable).
pub fn work_area_bottom_px() -> i32 {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut rect = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    rect.bottom
}

pub fn apply_win32_widget_region(w: &tauri::WebviewWindow, corner_style: &str) {
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};

    let Ok(hwnd) = w.hwnd() else {
        return;
    };

    let Ok(phys) = w.inner_size() else {
        return;
    };
    let width = phys.width as i32;
    let height = phys.height as i32;
    if width <= 0 || height <= 0 {
        return;
    }

    if corner_style != "round" {
        unsafe {
            // Restore default (rectangular) client region; system owns any previous region.
            let _ = SetWindowRgn(hwnd, None, true);
        }
        return;
    }

    let scale = w.scale_factor().unwrap_or(1.0);
    let r = ((WIDGET_CORNER_RADIUS_CSS_PX * scale).round() as i32).max(0);
    // CreateRoundRectRgn uses the ellipse width/height of each corner; for a circular corner
    // of radius r (CSS), use 2r × 2r (same as Win32 GDI+ / typical round-rect).
    let d = 2 * r;
    if d < 1 {
        unsafe {
            let _ = SetWindowRgn(hwnd, None, true);
        }
        return;
    }

    unsafe {
        let hrgn = CreateRoundRectRgn(0, 0, width, height, d, d);
        if hrgn.0.is_null() {
            return;
        }
        if SetWindowRgn(hwnd, Some(hrgn), true) == 0 {
            use windows::Win32::Graphics::Gdi::HGDIOBJ;
            let _ = DeleteObject(HGDIOBJ(hrgn.0));
        }
    }
}
