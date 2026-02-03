use crate::Size;
use gio::{traits::FileExt, Cancellable, FileQueryInfoFlags};
use std::{collections::HashMap, path::Path};

pub fn extract_video_thumbnail<P: AsRef<Path>>(file_path: P, size: Option<Size>) -> Result<Vec<u8>, String> {
    get_video_thumbnail(file_path, size).map_err(|e| e.to_string())
}

pub fn extract_video_thumbnails<P: AsRef<Path>>(file_paths: &[P], size: Option<Size>) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut result = HashMap::new();
    for file_path in file_paths {
        let thumbnail = get_video_thumbnail(file_path, size.clone()).map_err(|e| e.to_string())?;
        let _ = result.insert(file_path.as_ref().to_string_lossy().to_string(), thumbnail);
    }

    Ok(result)
}

#[allow(unused_variables)]
fn get_video_thumbnail<P: AsRef<Path>>(path: P, size: Option<Size>) -> Result<Vec<u8>, String> {
    let attributes = "thumbnail::path-normal,thumbnail::path-large,thumbnail::path-xlarge";
    let file = gio::File::for_parse_name(path.as_ref().to_str().unwrap());
    let info = file.query_info(attributes, FileQueryInfoFlags::NONE, Cancellable::NONE).map_err(|e| e.message().to_string())?;
    for attribute in attributes.split(",") {
        if let Some(thumbnail) = info.attribute_byte_string(attribute) {
            return std::fs::read(thumbnail).map_err(|e| e.to_string());
        }
    }

    Err("No thumbnails available".to_string())
}
