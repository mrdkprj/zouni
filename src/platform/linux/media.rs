use ffmpeg_next::{
    format::{input, Pixel},
    media::Type,
    software::scaling::{context::Context, flag::Flags},
    util::frame::video::Video,
};
use image::RgbImage;
use std::{collections::HashMap, path::Path};

pub fn extract_video_thumbnail<P: AsRef<Path>>(file_path: P) -> Result<Vec<u8>, String> {
    ffmpeg_next::init().map_err(|e| e.to_string())?;

    get_video_thumbnail(file_path).map_err(|e| e.to_string())
}

pub fn extract_video_thumbnails<P: AsRef<Path>>(file_paths: &[P]) -> Result<HashMap<String, Vec<u8>>, String> {
    ffmpeg_next::init().map_err(|e| e.to_string())?;

    let mut result = HashMap::new();
    for file_path in file_paths {
        let thumbnail = get_video_thumbnail(file_path).map_err(|e| e.to_string())?;
        let _ = result.insert(file_path.as_ref().to_string_lossy().to_string(), thumbnail);
    }

    Ok(result)
}

fn get_video_thumbnail<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, ffmpeg_next::Error> {
    let mut result = Vec::new();

    if let Ok(mut ictx) = input(path.as_ref()) {
        let input = ictx.streams().best(Type::Video).ok_or(ffmpeg_next::Error::StreamNotFound)?;
        let stream_index = input.index();
        let context_decoder = ffmpeg_next::codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = context_decoder.decoder().video()?;

        let mut scaler = Context::get(decoder.format(), decoder.width(), decoder.height(), Pixel::RGB24, decoder.width(), decoder.height(), Flags::BILINEAR)?;

        for (stream, packet) in ictx.packets() {
            if stream.index() == stream_index {
                decoder.send_packet(&packet)?;

                let mut frame = Video::empty();
                decoder.receive_frame(&mut frame)?;

                let mut rgb_frame = Video::empty();
                scaler.run(&frame, &mut rgb_frame)?;

                result = into_buffer(&rgb_frame);

                break;
            }
        }
    }

    Ok(result)
}

fn into_buffer(rgb_frame: &Video) -> Vec<u8> {
    let mut buffer: RgbImage = image::ImageBuffer::new(rgb_frame.width(), rgb_frame.height());

    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let data = rgb_frame.data(0);
        let stride = rgb_frame.stride(0);
        let offset = y as usize * stride + x as usize * 3;
        *pixel = image::Rgb([data[offset], data[offset + 1], data[offset + 2]]);
    }

    let mut bytes: Vec<u8> = Vec::new();
    buffer.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
    bytes
}
