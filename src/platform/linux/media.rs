use crate::Size;
#[cfg(feature = "ffmpeg")]
use ffmpeg_next::{
    format::{input, Pixel},
    media::Type,
    software::scaling::{context::Context, flag::Flags},
    util::frame::video::Video,
};
use gio::{traits::FileExt, Cancellable, FileQueryInfoFlags};
#[cfg(feature = "ffmpeg")]
use image::RgbImage;
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

    #[cfg(feature = "ffmpeg")]
    return create_video_thumbnail(path, size).map_err(|e| e.to_string());
    #[cfg(not(feature = "ffmpeg"))]
    return Err("No thumbnails available".to_string());
}

#[cfg(feature = "ffmpeg")]
fn create_video_thumbnail<P: AsRef<Path>>(path: P, size: Option<Size>) -> Result<Vec<u8>, ffmpeg_next::Error> {
    ffmpeg_next::init()?;

    let mut result = Vec::new();

    if let Ok(mut ictx) = input(path.as_ref()) {
        let input = ictx.streams().best(Type::Video).ok_or(ffmpeg_next::Error::StreamNotFound)?;
        let stream_index = input.index();
        let context_decoder = ffmpeg_next::codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = context_decoder.decoder().video()?;
        let mut rotation: i32 = 0;
        for data in input.side_data() {
            if data.kind() == ffmpeg_next::packet::side_data::Type::DisplayMatrix {
                rotation = parse_display_matrix(data.data());
            } else {
                rotation = input.metadata().get("rotate").unwrap_or("0").parse().unwrap();
            }
        }

        let (width, height) = if let Some(size) = size {
            let scale = f64::min(size.width as f64 / decoder.width() as f64, size.height as f64 / decoder.height() as f64);
            ((decoder.width() as f64 * scale) as u32, (decoder.height() as f64 * scale) as u32)
        } else {
            (decoder.width(), decoder.height())
        };

        let mut scaler = Context::get(decoder.format(), decoder.width(), decoder.height(), Pixel::RGB24, width, height, Flags::BILINEAR)?;

        for (stream, packet) in ictx.packets() {
            if stream.index() == stream_index {
                decoder.send_packet(&packet)?;

                let mut frame = Video::empty();
                decoder.receive_frame(&mut frame)?;

                let mut rgb_frame = Video::empty();
                scaler.run(&frame, &mut rgb_frame)?;

                result = into_buffer(&rgb_frame, rotation);

                break;
            }
        }
    }

    Ok(result)
}

#[cfg(feature = "ffmpeg")]
fn into_buffer(rgb_frame: &Video, rotation: i32) -> Vec<u8> {
    let mut buffer: RgbImage = image::ImageBuffer::new(rgb_frame.width(), rgb_frame.height());

    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let data = rgb_frame.data(0);
        let stride = rgb_frame.stride(0);
        let offset = y as usize * stride + x as usize * 3;
        *pixel = image::Rgb([data[offset], data[offset + 1], data[offset + 2]]);
    }

    let buffer = match rotation {
        90 => image::imageops::rotate90(&buffer),
        180 => image::imageops::rotate180(&buffer),
        270 => image::imageops::rotate270(&buffer),
        _ => buffer,
    };

    let mut bytes: Vec<u8> = Vec::new();
    buffer.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
    bytes
}

#[cfg(feature = "ffmpeg")]
fn parse_display_matrix(data: &[u8]) -> i32 {
    let matrix: [i32; 9] = unsafe { std::ptr::read(data.as_ptr() as *const [i32; 9]) };
    // let matrix_f: Vec<f64> = matrix.iter().map(|&v| v as f64 / 65536.0).collect();

    // Detect rotation
    match (matrix[0], matrix[1], matrix[3], matrix[4]) {
        (0, 65536, -65536, 0) => 90,
        (0, -65536, 65536, 0) => 270,
        (-65536, 0, 0, -65536) => 180,
        (65536, 0, 0, 65536) => 0,
        _ => -1,
    }
}
