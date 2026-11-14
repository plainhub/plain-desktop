pub(crate) const MEDIA_BACKEND: (&str, &str) = ("WEBKIT_GST_USE_PLAYBIN3", "1");

pub(crate) fn configure_media_backend() {
    // Application startup calls this before logging, the runtime or WebKit creates threads.
    unsafe { std::env::set_var(MEDIA_BACKEND.0, MEDIA_BACKEND.1) };
}
