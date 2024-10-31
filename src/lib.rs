mod platform;
use platform::platform_impl;

pub fn reserve() -> u32 {
    platform_impl::reserve()
}

pub fn mv(source_file: String, dest_file: String, id: Option<u32>) -> Result<(), String> {
    platform_impl::mv(source_file, dest_file, id)
}

pub fn mv_with_progress(source_file: String, dest_file: String, handler: &mut dyn FnMut(i64, i64), id: Option<u32>) -> Result<(), String> {
    platform_impl::mv_with_progress(source_file, dest_file, handler, id)
}

pub fn mv_sync(source_file: String, dest_file: String) -> bool {
    platform_impl::mv_sync(source_file, dest_file).unwrap()
}

pub fn cancel(id: u32) -> bool {
    platform_impl::cancel(id)
}

pub fn trash(file: String) -> Result<(), String> {
    platform_impl::trash(file)
}
