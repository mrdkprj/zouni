use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use windows::Win32::System::Com::CLSIDFromString;
use windows_core::PCWSTR;

use super::{shell, util::encode_wide};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub streams: Vec<HashMap<String, String>>,
    pub format: HashMap<String, String>,
}

pub fn get_media_metadata<P: AsRef<Path>>(file_path: P) -> Metadata {
    let all_props = shell::read_properties(file_path);
    to_ffmpeg_(all_props)
}

fn to_ffmpeg_(props: HashMap<String, String>) -> Metadata {
    let mut metadata = Metadata {
        streams: Vec::new(),
        format: HashMap::new(),
    };

    metadata.streams.push(HashMap::new());
    metadata.streams.push(HashMap::new());

    let aspect_ratio = format!("{}:{}", props.get("VideoHorizontalAspectRatio").unwrap_or(&String::new()), props.get("VideoVerticalAspectRatio").unwrap_or(&String::new()));
    metadata.streams[0].insert("sample_aspect_ratio".to_string(), aspect_ratio);

    for (key, value) in props {
        match key.as_str() {
            "AudioChannelCount" => metadata.streams[1].insert("channels".to_string(), value),
            "AudioEncodingBitrate" => metadata.streams[1].insert("bit_rate".to_string(), value),
            "AudioFormat" => metadata.streams[1].insert("codec_name".to_string(), get_audio_format(&value).to_string()),
            "AudioSampleRate" => metadata.streams[1].insert("sample_rate".to_string(), value),
            "MediaDuration" => metadata.format.insert("duration".to_string(), duration(&value)),
            "Size" => metadata.format.insert("size".to_string(), value),
            // "VideoCompression" => metadata.streams[0].insert("codec_name".to_string(), get_video_format(&value).to_string()),
            "VideoEncodingBitrate" => metadata.streams[0].insert("bit_rate".to_string(), value),
            "VideoFourCC" => metadata.streams[0].insert("codec_name".to_string(), fourcc_to_string(value.parse().unwrap())),
            "VideoFrameHeight" => metadata.streams[0].insert("height".to_string(), value),
            "VideoFrameRate" => metadata.streams[0].insert("r_frame_rate".to_string(), value),
            "VideoFrameWidth" => metadata.streams[0].insert("width".to_string(), value),
            "VideoOrientation" => metadata.streams[0].insert("rotation".to_string(), value),
            "VideoTotalBitrate" => metadata.format.insert("bit_rate".to_string(), value),
            _ => None,
        };
    }

    metadata
}

fn duration(duration_str: &str) -> String {
    let duration: f64 = duration_str.parse().unwrap();
    let duration_sec = duration / 10000000.0;
    duration_sec.to_string()
}

fn fourcc_to_string(fourcc: u32) -> String {
    let bytes = fourcc.to_le_bytes(); // Convert to little-endian bytes
    format!("{}{}{}{}", bytes[0] as char, bytes[1] as char, bytes[2] as char, bytes[3] as char)
}

#[allow(dead_code)]
fn get_video_format(guid_str: &str) -> &str {
    let guid = unsafe { CLSIDFromString(PCWSTR::from_raw(encode_wide(guid_str).as_ptr())) }.unwrap();
    match guid {
        windows::Win32::Media::MediaFoundation::MFVideoFormat_420O => "420O",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_A16B16G16R16F => "A16B16G16R16F",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_A2R10G10B10 => "A2R10G10B10",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_AI44 => "AI44",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_ARGB32 => "ARGB32",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_AV1 => "AV1",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_AYUV => "AYUV",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Base => "Base",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Base_HDCP => "Base_HDCP",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_DV25 => "DV25",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_DV50 => "DV50",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_DVH1 => "DVH1",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_DVHD => "DVHD",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_DVSD => "DVSD",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_DVSL => "DVSL",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_H263 => "H263",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_H264 => "H264",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_H264_ES => "H264_ES",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_H264_HDCP => "H264_HDCP",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_H265 => "H265",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_HEVC => "HEVC",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_HEVC_ES => "HEVC_ES",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_HEVC_HDCP => "HEVC_HDCP",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_I420 => "I420",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_IYUV => "IYUV",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_L16 => "L16",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_L8 => "L8",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_M4S2 => "M4S2",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MJPG => "MJPG",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MP43 => "MP43",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MP4S => "MP4S",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MP4V => "MP4V",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MPEG2 => "MPEG2",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MPG1 => "MPG1",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MSS1 => "MSS1",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_MSS2 => "MSS2",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_NV11 => "NV11",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_NV12 => "NV12",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_NV21 => "NV21",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_ORAW => "ORAW",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_P010 => "P010",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_P016 => "P016",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_P210 => "P210",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_P216 => "P216",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_RGB24 => "RGB24",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_RGB32 => "RGB32",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_RGB555 => "RGB555",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_RGB565 => "RGB565",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_RGB8 => "RGB8",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Theora => "Theora",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_UYVY => "UYVY",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_VP10 => "VP10",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_VP80 => "VP80",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_VP90 => "VP90",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_WMV1 => "WMV1",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_WMV2 => "WMV2",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_WMV3 => "WMV3",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_WVC1 => "WVC1",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y210 => "Y210",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y216 => "Y216",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y410 => "Y410",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y416 => "Y416",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y41P => "Y41P",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y41T => "Y41T",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_Y42T => "Y42T",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_YUY2 => "YUY2",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_YV12 => "YV12",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_YVU9 => "YVU9",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_YVYU => "YVYU",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_v210 => "v210",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_v216 => "v216",
        windows::Win32::Media::MediaFoundation::MFVideoFormat_v410 => "v410",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AAC => "AAC",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AAC_HDCP => "AAC_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_ADTS => "ADTS",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_ADTS_HDCP => "ADTS_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_ALAC => "ALAC",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AMR_NB => "AMR_NB",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AMR_WB => "AMR_WB",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AMR_WP => "AMR_WP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Base_HDCP => "Base_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DRM => "DRM",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS => "DTS",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_HD => "DTS_HD",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_LBR => "DTS_LBR",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_RAW => "DTS_RAW",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_UHD => "DTS_UHD",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_UHDY => "DTS_UHDY",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_XLL => "DTS_XLL",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC3 => "Dolby_AC3",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC3_HDCP => "Dolby_AC3_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC3_SPDIF => "Dolby_AC3_SPDIF",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4 => "Dolby_AC4",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V1 => "Dolby_AC4_V1",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V1_ES => "Dolby_AC4_V1_ES",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V2 => "Dolby_AC4_V2",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V2_ES => "Dolby_AC4_V2_ES",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_DDPlus => "Dolby_DDPlus",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_FLAC => "FLAC",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Float => "Float",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Float_SpatialObjects => "Float_SpatialObjects",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_LPCM => "LPCM",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_MP3 => "MP3",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_MPEG => "MPEG",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_MSP1 => "MSP1",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Opus => "Opus",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_PCM => "PCM",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_PCM_HDCP => "PCM_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Vorbis => "Vorbis",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMASPDIF => "WMASPDIF",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMAudioV8 => "WMAudioV8",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMAudioV9 => "WMAudioV9",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMAudio_Lossless => "WMAudio_Lossless",
        _ => "",
    }
}

fn get_audio_format(guid_str: &str) -> &str {
    let guid = unsafe { CLSIDFromString(PCWSTR::from_raw(encode_wide(guid_str).as_ptr())) }.unwrap();

    match guid {
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AAC => "AAC",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AAC_HDCP => "AAC_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_ADTS => "ADTS",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_ADTS_HDCP => "ADTS_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_ALAC => "ALAC",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AMR_NB => "AMR_NB",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AMR_WB => "AMR_WB",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_AMR_WP => "AMR_WP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Base => "Base",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Base_HDCP => "Base_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DRM => "DRM",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS => "DTS",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_HD => "DTS_HD",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_LBR => "DTS_LBR",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_RAW => "DTS_RAW",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_UHD => "DTS_UHD",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_UHDY => "DTS_UHDY",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_DTS_XLL => "DTS_XLL",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC3 => "Dolby_AC3",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC3_HDCP => "Dolby_AC3_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC3_SPDIF => "Dolby_AC3_SPDIF",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4 => "Dolby_AC4",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V1 => "Dolby_AC4_V1",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V1_ES => "Dolby_AC4_V1_ES",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V2 => "Dolby_AC4_V2",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_AC4_V2_ES => "Dolby_AC4_V2_ES",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Dolby_DDPlus => "Dolby_DDPlus",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_FLAC => "FLAC",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Float => "Float",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Float_SpatialObjects => "Float_SpatialObjects",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_LPCM => "LPCM",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_MP3 => "MP3",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_MPEG => "MPEG",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_MSP1 => "MSP1",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Opus => "Opus",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_PCM => "PCM",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_PCM_HDCP => "PCM_HDCP",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_Vorbis => "Vorbis",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMASPDIF => "WMASPDIF",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMAudioV8 => "WMAudioV8",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMAudioV9 => "WMAudioV9",
        windows::Win32::Media::MediaFoundation::MFAudioFormat_WMAudio_Lossless => "WMAudio_Lossless",
        _ => "",
    }
}
