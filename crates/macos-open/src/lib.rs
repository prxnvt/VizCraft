//! Documents Finder asks the app to open (double click, Open With, Dock drops): the `odoc`
//! Apple Event. Cascade 0 stub: a no-op API with the final shape; the macOS `imp` lands later.

/// Register the Open Documents handler. Call before the event loop starts.
pub fn install() {}

/// Called when documents arrive, so the UI can repaint.
pub fn set_waker(_wake: impl Fn() + Send + Sync + 'static) {}

/// Paths asked for since the last call.
pub fn take_pending() -> Vec<String> {
    Vec::new()
}
