pub(crate) fn init() {
    if !gtk::is_initialized() {
        let _ = gtk::init();
    }
}
