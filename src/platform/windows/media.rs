use crate::{
    platform::windows::util::{encode_wide, ComGuard},
    shell::read_properties,
    Size,
};
use image::{ImageBuffer, ImageFormat, RgbImage};
use std::{collections::HashMap, io::Cursor, path::Path};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{DeleteObject, GetObjectW, BITMAP},
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY},
    },
};

pub fn extract_video_thumbnail<P: AsRef<Path>>(file_path: P, size: Option<Size>) -> Result<Vec<u8>, String> {
    let _guard = ComGuard::new();
    unsafe { get_video_thumbnail(file_path, size).map_err(|e| e.message()) }
}

pub fn extract_video_thumbnails<P: AsRef<Path>>(file_paths: &[P], size: Option<Size>) -> Result<HashMap<String, Vec<u8>>, String> {
    let _guard = ComGuard::new();

    let mut result = HashMap::new();
    for file_path in file_paths {
        let thumbnail = unsafe { get_video_thumbnail(file_path, size.clone()).map_err(|e| e.message()) }?;
        let _ = result.insert(file_path.as_ref().to_string_lossy().to_string(), thumbnail);
    }

    Ok(result)
}

unsafe fn get_video_thumbnail<P: AsRef<Path>>(path: P, size: Option<Size>) -> windows::core::Result<Vec<u8>> {
    let _guard = ComGuard::new();

    let wide = encode_wide(path.as_ref());
    let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;

    let (width, height) = if let Some(size) = size {
        (size.width, size.height)
    } else {
        let props = read_properties(path);
        (props.get("VideoFrameWidth").unwrap_or(&"100".to_string()).parse().unwrap(), props.get("VideoFrameHeight").unwrap_or(&"100".to_string()).parse().unwrap())
    };

    // Request image at desired size
    let size = SIZE {
        cx: width as i32,
        cy: height as i32,
    };

    // SIIGBF_THUMBNAILONLY: force thumbnail generation
    // SIIGBF_RESIZETOFIT: fit within requested size
    let hbitmap = factory.GetImage(size, SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT)?;

    // Convert HBITMAP → BGRA bytes
    let mut bmp: BITMAP = std::mem::zeroed();
    GetObjectW(hbitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as _));

    let width = bmp.bmWidth as usize;
    let height = bmp.bmHeight as usize;
    let stride = bmp.bmWidthBytes as usize;
    let bites_per_pixel = bmp.bmBitsPixel;
    let buf_size = stride * height;

    let mut buffer = vec![0u8; buf_size];
    std::ptr::copy_nonoverlapping(bmp.bmBits as *const u8, buffer.as_mut_ptr(), buf_size);

    let _ = DeleteObject(hbitmap.into());

    let bytes = into_buffer(&buffer, width as _, height as _, stride as _, bites_per_pixel);

    Ok(bytes)
}

fn into_buffer(data: &[u8], width: u32, height: u32, stride: usize, bits_per_pixel: u16) -> Vec<u8> {
    let mut bytes: Vec<u8> = Vec::new();

    let bytes_per_pixel = match bits_per_pixel {
        32 => 4,
        24 => 3,
        _ => 3,
    };

    let mut buffer: RgbImage = ImageBuffer::new(width, height);

    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let offset = y as usize * stride + x as usize * bytes_per_pixel;
        *pixel = image::Rgb([data[offset + 2], data[offset + 1], data[offset]]);
    }

    buffer.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg).unwrap();

    bytes
}
