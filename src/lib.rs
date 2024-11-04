mod platform;
use platform::platform_impl;

pub fn reserve_cancellable() -> u32 {
    platform_impl::reserve_cancellable()
}

pub fn mv(source_file: String, dest_file: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancellable: Option<u32>) -> Result<(), String> {
    platform_impl::mv(source_file, dest_file, callback, cancellable)
}

pub fn mv_bulk(source_files: Vec<String>, dest_dir: String, callback: Option<&mut dyn FnMut(i64, i64)>, cancellable: Option<u32>) -> Result<(), String> {
    platform_impl::mv_bulk(source_files, dest_dir, callback, cancellable)
}

pub fn cancel(id: u32) -> bool {
    platform_impl::cancel(id)
}

pub fn trash(file: String) -> Result<(), String> {
    platform_impl::trash(file)
}
