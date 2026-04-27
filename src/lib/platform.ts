/** True when running on a Linux desktop (WebKitGTK). Reliable inside Tauri. */
export const IS_LINUX =
  /Linux/.test(navigator.userAgent) && !/Android/.test(navigator.userAgent);
