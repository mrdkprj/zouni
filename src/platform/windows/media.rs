#![allow(non_upper_case_globals)]
use super::util::encode_wide;
use crate::platform::windows::util::ComGuard;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use windows::{
    core::{Result, GUID, PCWSTR},
    Win32::{Media::MediaFoundation::*, System::Com::StructuredStorage::PropVariantClear},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub streams: Vec<HashMap<String, String>>,
    pub format: HashMap<String, String>,
}

pub fn get_media_metadata<P1: AsRef<Path>>(file_path: P1) -> Result<Metadata> {
    let mut metadata = Metadata::default();

    let _guard = ComGuard::new();

    unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }?;

    let source_reader = unsafe { MFCreateSourceReaderFromURL(PCWSTR::from_raw(encode_wide(file_path.as_ref()).as_ptr()), None) }?;

    if let Ok(video_type) = unsafe { source_reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as _) } {
        let mut streams0 = HashMap::new();

        if let Ok(value) = unsafe { video_type.GetGUID(&MF_MT_SUBTYPE) } {
            streams0.insert("codec_name".to_string(), get_video_format(value));
        }

        if let Ok(value) = unsafe { video_type.GetUINT64(&MF_MT_FRAME_SIZE) } {
            let (width, height) = unpack_uint32_pair(value);
            streams0.insert("width".to_string(), width.to_string());
            streams0.insert("height".to_string(), height.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_MPEG2_PROFILE) } {
            let value = eAVEncH264VProfile(value as _);
            let primaries = match value {
                eAVEncH264VProfile_422 => "422",
                eAVEncH264VProfile_444 => "444",
                eAVEncH264VProfile_ConstrainedBase => "ConstrainedBase",
                eAVEncH264VProfile_Extended => "Extended",
                eAVEncH264VProfile_High => "High",
                eAVEncH264VProfile_High10 => "High10",
                eAVEncH264VProfile_Main => "Main",
                eAVEncH264VProfile_MultiviewHigh => "MultiviewHigh",
                eAVEncH264VProfile_ScalableBase => "ScalableBase",
                eAVEncH264VProfile_ScalableHigh => "ScalableHigh",
                eAVEncH264VProfile_Simple => "Simple",
                eAVEncH264VProfile_StereoHigh => "StereoHigh",
                eAVEncH264VProfile_UCConstrainedHigh => "UCConstrainedHigh",
                eAVEncH264VProfile_UCScalableConstrainedBase => "UCScalableConstrainedBase",
                eAVEncH264VProfile_UCScalableConstrainedHigh => "UCScalableConstrainedHigh",
                eAVEncH264VProfile_unknown => "unknown",
                _ => "N/A",
            };
            streams0.insert("profile".to_string(), primaries.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_MPEG2_LEVEL) } {
            streams0.insert("level".to_string(), value.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_VIDEO_PRIMARIES) } {
            let value = MFVideoPrimaries(value as _);
            let primaries = match value {
                MFVideoPrimaries_Unknown => "Unknown",
                MFVideoPrimaries_reserved => "reserved",
                MFVideoPrimaries_BT709 => "bt709",
                MFVideoPrimaries_BT470_2_SysM => "bt470_2_sysm",
                MFVideoPrimaries_BT470_2_SysBG => "bt470_2_sysbg",
                MFVideoPrimaries_SMPTE170M => "smpte170m",
                MFVideoPrimaries_SMPTE240M => "smpte240m",
                MFVideoPrimaries_EBU3213 => "ebu3213",
                MFVideoPrimaries_SMPTE_C => "smpte_c",
                MFVideoPrimaries_BT2020 => "bt2020",
                MFVideoPrimaries_XYZ => "xyz",
                MFVideoPrimaries_DCI_P3 => "dci_p3",
                MFVideoPrimaries_ACES => "aces",
                _ => "N/A",
            };
            streams0.insert("color_primaries".to_string(), primaries.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_TRANSFER_FUNCTION) } {
            let value = MFVideoTransferFunction(value as _);
            let primaries = match value {
                MFVideoTransFunc_10 => "10",
                MFVideoTransFunc_10_rel => "10_rel",
                MFVideoTransFunc_18 => "18",
                MFVideoTransFunc_20 => "20",
                MFVideoTransFunc_2020 => "2020",
                MFVideoTransFunc_2020_const => "2020_const",
                MFVideoTransFunc_2084 => "2084",
                MFVideoTransFunc_22 => "22",
                MFVideoTransFunc_240M => "240m",
                MFVideoTransFunc_26 => "26",
                MFVideoTransFunc_28 => "28",
                MFVideoTransFunc_709 => "709",
                MFVideoTransFunc_709_sym => "709_sym",
                MFVideoTransFunc_HLG => "hlg",
                MFVideoTransFunc_Log_100 => "log_100",
                MFVideoTransFunc_Log_316 => "log_316",
                MFVideoTransFunc_Unknown => "unknown",
                MFVideoTransFunc_sRGB => "srgb",
                _ => "N/A",
            };
            streams0.insert("color_trc".to_string(), primaries.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_YUV_MATRIX) } {
            let value = MFVideoTransferMatrix(value as _);
            let primaries = match value {
                MFVideoTransferMatrix_BT2020_10 => "bt2020_10",
                MFVideoTransferMatrix_BT2020_12 => "bt2020_12",
                MFVideoTransferMatrix_BT601 => "bt601",
                MFVideoTransferMatrix_BT709 => "bt709",
                MFVideoTransferMatrix_SMPTE240M => "smpte240m",
                MFVideoTransferMatrix_Unknown => "unknown",
                _ => "N/A",
            };
            streams0.insert("colorspace".to_string(), primaries.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_INTERLACE_MODE) } {
            let value = MFVideoInterlaceMode(value as _);
            let primaries = match value {
                MFVideoInterlace_FieldInterleavedLowerFirst => "FieldInterleavedLowerFirst",
                MFVideoInterlace_FieldInterleavedUpperFirst => "FieldInterleavedUpperFirst",
                MFVideoInterlace_FieldSingleLower => "FieldSingleLower",
                MFVideoInterlace_FieldSingleUpper => "FieldSingleUpper",
                MFVideoInterlace_MixedInterlaceOrProgressive => "MixedInterlaceOrProgressive",
                MFVideoInterlace_Progressive => "Progressive",
                MFVideoInterlace_Unknown => "smpte240m",
                _ => "N/A",
            };
            streams0.insert("field_order".to_string(), primaries.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_AVG_BITRATE) } {
            streams0.insert("bit_rate".to_string(), value.to_string());
        }

        if let Ok(value) = unsafe { video_type.GetUINT64(&MF_MT_FRAME_RATE) } {
            let (a, b) = unpack_uint32_pair(value);
            streams0.insert("r_frame_rate".to_string(), format!("{:?}/{:?}", a, b));
        }

        if let Ok(value) = unsafe { video_type.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO) } {
            let (a, b) = unpack_uint32_pair(value);
            streams0.insert("sample_aspect_ratio".to_string(), format!("{:?}:{:?}", a, b));
        }

        if let Ok(value) = unsafe { video_type.GetUINT32(&MF_MT_VIDEO_ROTATION) } {
            streams0.insert("rotation".to_string(), value.to_string());
        }

        metadata.streams.push(streams0);
    }

    if let Ok(audio_type) = unsafe { source_reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as _) } {
        let mut streams1 = HashMap::new();

        if let Ok(value) = unsafe { audio_type.GetGUID(&MF_MT_SUBTYPE) } {
            streams1.insert("codec_name".to_string(), get_audio_format(value));
        }

        if let Ok(value) = unsafe { audio_type.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) } {
            streams1.insert("channels".to_string(), value.to_string());
        }

        if let Ok(value) = unsafe { audio_type.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE) } {
            streams1.insert("bits_per_sample".to_string(), value.to_string());
        }

        if let Ok(value) = unsafe { audio_type.GetUINT32(&MF_MT_AVG_BITRATE) } {
            streams1.insert("bit_rate".to_string(), value.to_string());
        }

        metadata.streams.push(streams1);
    }

    if let Ok(mut variant) = unsafe { source_reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as _, &MF_PD_DURATION) } {
        let duration_str = variant.to_string();
        let duration: f32 = duration_str.parse().unwrap();
        let duration_min = duration / 10000001.0;
        metadata.format.insert("duration".to_string(), duration_min.to_string());
        let _ = unsafe { PropVariantClear(&mut variant as _) };
    }

    if let Ok(mut variant) = unsafe { source_reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as _, &MF_PD_TOTAL_FILE_SIZE) } {
        metadata.format.insert("size".to_string(), variant.to_string());
        let _ = unsafe { PropVariantClear(&mut variant as _) };
    }

    unsafe { MFShutdown() }?;

    Ok(metadata)
}

pub fn unpack_uint32_pair(packed: u64) -> (u32, u32) {
    let high = (packed >> 32) as u32;
    let low = (packed & 0xFFFFFFFF) as u32;
    (high, low)
}

#[allow(dead_code)]
fn fourcc_to_string(fourcc: u32) -> String {
    let bytes = fourcc.to_le_bytes(); // Convert to little-endian bytes
    format!("{}{}{}{}", bytes[0] as char, bytes[1] as char, bytes[2] as char, bytes[3] as char)
}

fn get_video_format(guid: GUID) -> String {
    let format = match guid {
        MFVideoFormat_420O => "420O",
        MFVideoFormat_A16B16G16R16F => "A16B16G16R16F",
        MFVideoFormat_A2R10G10B10 => "A2R10G10B10",
        MFVideoFormat_AI44 => "AI44",
        MFVideoFormat_ARGB32 => "ARGB32",
        MFVideoFormat_AV1 => "AV1",
        MFVideoFormat_AYUV => "AYUV",
        MFVideoFormat_Base => "Base",
        MFVideoFormat_Base_HDCP => "Base_HDCP",
        MFVideoFormat_DV25 => "DV25",
        MFVideoFormat_DV50 => "DV50",
        MFVideoFormat_DVH1 => "DVH1",
        MFVideoFormat_DVHD => "DVHD",
        MFVideoFormat_DVSD => "DVSD",
        MFVideoFormat_DVSL => "DVSL",
        MFVideoFormat_H263 => "H263",
        MFVideoFormat_H264 => "H264",
        MFVideoFormat_H264_ES => "H264_ES",
        MFVideoFormat_H264_HDCP => "H264_HDCP",
        MFVideoFormat_H265 => "H265",
        MFVideoFormat_HEVC => "HEVC",
        MFVideoFormat_HEVC_ES => "HEVC_ES",
        MFVideoFormat_HEVC_HDCP => "HEVC_HDCP",
        MFVideoFormat_I420 => "I420",
        MFVideoFormat_IYUV => "IYUV",
        MFVideoFormat_L16 => "L16",
        MFVideoFormat_L8 => "L8",
        MFVideoFormat_M4S2 => "M4S2",
        MFVideoFormat_MJPG => "MJPG",
        MFVideoFormat_MP43 => "MP43",
        MFVideoFormat_MP4S => "MP4S",
        MFVideoFormat_MP4V => "MP4V",
        MFVideoFormat_MPEG2 => "MPEG2",
        MFVideoFormat_MPG1 => "MPG1",
        MFVideoFormat_MSS1 => "MSS1",
        MFVideoFormat_MSS2 => "MSS2",
        MFVideoFormat_NV11 => "NV11",
        MFVideoFormat_NV12 => "NV12",
        MFVideoFormat_NV21 => "NV21",
        MFVideoFormat_ORAW => "ORAW",
        MFVideoFormat_P010 => "P010",
        MFVideoFormat_P016 => "P016",
        MFVideoFormat_P210 => "P210",
        MFVideoFormat_P216 => "P216",
        MFVideoFormat_RGB24 => "RGB24",
        MFVideoFormat_RGB32 => "RGB32",
        MFVideoFormat_RGB555 => "RGB555",
        MFVideoFormat_RGB565 => "RGB565",
        MFVideoFormat_RGB8 => "RGB8",
        MFVideoFormat_Theora => "Theora",
        MFVideoFormat_UYVY => "UYVY",
        MFVideoFormat_VP10 => "VP10",
        MFVideoFormat_VP80 => "VP80",
        MFVideoFormat_VP90 => "VP90",
        MFVideoFormat_WMV1 => "WMV1",
        MFVideoFormat_WMV2 => "WMV2",
        MFVideoFormat_WMV3 => "WMV3",
        MFVideoFormat_WVC1 => "WVC1",
        MFVideoFormat_Y210 => "Y210",
        MFVideoFormat_Y216 => "Y216",
        MFVideoFormat_Y410 => "Y410",
        MFVideoFormat_Y416 => "Y416",
        MFVideoFormat_Y41P => "Y41P",
        MFVideoFormat_Y41T => "Y41T",
        MFVideoFormat_Y42T => "Y42T",
        MFVideoFormat_YUY2 => "YUY2",
        MFVideoFormat_YV12 => "YV12",
        MFVideoFormat_YVU9 => "YVU9",
        MFVideoFormat_YVYU => "YVYU",
        MFVideoFormat_v210 => "v210",
        MFVideoFormat_v216 => "v216",
        MFVideoFormat_v410 => "v410",
        MFAudioFormat_AAC => "AAC",
        MFAudioFormat_AAC_HDCP => "AAC_HDCP",
        MFAudioFormat_ADTS => "ADTS",
        MFAudioFormat_ADTS_HDCP => "ADTS_HDCP",
        MFAudioFormat_ALAC => "ALAC",
        MFAudioFormat_AMR_NB => "AMR_NB",
        MFAudioFormat_AMR_WB => "AMR_WB",
        MFAudioFormat_AMR_WP => "AMR_WP",
        MFAudioFormat_Base_HDCP => "Base_HDCP",
        MFAudioFormat_DRM => "DRM",
        MFAudioFormat_DTS => "DTS",
        MFAudioFormat_DTS_HD => "DTS_HD",
        MFAudioFormat_DTS_LBR => "DTS_LBR",
        MFAudioFormat_DTS_RAW => "DTS_RAW",
        MFAudioFormat_DTS_UHD => "DTS_UHD",
        MFAudioFormat_DTS_UHDY => "DTS_UHDY",
        MFAudioFormat_DTS_XLL => "DTS_XLL",
        MFAudioFormat_Dolby_AC3 => "Dolby_AC3",
        MFAudioFormat_Dolby_AC3_HDCP => "Dolby_AC3_HDCP",
        MFAudioFormat_Dolby_AC3_SPDIF => "Dolby_AC3_SPDIF",
        MFAudioFormat_Dolby_AC4 => "Dolby_AC4",
        MFAudioFormat_Dolby_AC4_V1 => "Dolby_AC4_V1",
        MFAudioFormat_Dolby_AC4_V1_ES => "Dolby_AC4_V1_ES",
        MFAudioFormat_Dolby_AC4_V2 => "Dolby_AC4_V2",
        MFAudioFormat_Dolby_AC4_V2_ES => "Dolby_AC4_V2_ES",
        MFAudioFormat_Dolby_DDPlus => "Dolby_DDPlus",
        MFAudioFormat_FLAC => "FLAC",
        MFAudioFormat_Float => "Float",
        MFAudioFormat_Float_SpatialObjects => "Float_SpatialObjects",
        MFAudioFormat_LPCM => "LPCM",
        MFAudioFormat_MP3 => "MP3",
        MFAudioFormat_MPEG => "MPEG",
        MFAudioFormat_MSP1 => "MSP1",
        MFAudioFormat_Opus => "Opus",
        MFAudioFormat_PCM => "PCM",
        MFAudioFormat_PCM_HDCP => "PCM_HDCP",
        MFAudioFormat_Vorbis => "Vorbis",
        MFAudioFormat_WMASPDIF => "WMASPDIF",
        MFAudioFormat_WMAudioV8 => "WMAudioV8",
        MFAudioFormat_WMAudioV9 => "WMAudioV9",
        MFAudioFormat_WMAudio_Lossless => "WMAudio_Lossless",
        _ => "",
    };

    format.to_string()
}

fn get_audio_format(guid: GUID) -> String {
    let format = match guid {
        MFAudioFormat_AAC => "AAC",
        MFAudioFormat_AAC_HDCP => "AAC_HDCP",
        MFAudioFormat_ADTS => "ADTS",
        MFAudioFormat_ADTS_HDCP => "ADTS_HDCP",
        MFAudioFormat_ALAC => "ALAC",
        MFAudioFormat_AMR_NB => "AMR_NB",
        MFAudioFormat_AMR_WB => "AMR_WB",
        MFAudioFormat_AMR_WP => "AMR_WP",
        MFAudioFormat_Base => "Base",
        MFAudioFormat_Base_HDCP => "Base_HDCP",
        MFAudioFormat_DRM => "DRM",
        MFAudioFormat_DTS => "DTS",
        MFAudioFormat_DTS_HD => "DTS_HD",
        MFAudioFormat_DTS_LBR => "DTS_LBR",
        MFAudioFormat_DTS_RAW => "DTS_RAW",
        MFAudioFormat_DTS_UHD => "DTS_UHD",
        MFAudioFormat_DTS_UHDY => "DTS_UHDY",
        MFAudioFormat_DTS_XLL => "DTS_XLL",
        MFAudioFormat_Dolby_AC3 => "Dolby_AC3",
        MFAudioFormat_Dolby_AC3_HDCP => "Dolby_AC3_HDCP",
        MFAudioFormat_Dolby_AC3_SPDIF => "Dolby_AC3_SPDIF",
        MFAudioFormat_Dolby_AC4 => "Dolby_AC4",
        MFAudioFormat_Dolby_AC4_V1 => "Dolby_AC4_V1",
        MFAudioFormat_Dolby_AC4_V1_ES => "Dolby_AC4_V1_ES",
        MFAudioFormat_Dolby_AC4_V2 => "Dolby_AC4_V2",
        MFAudioFormat_Dolby_AC4_V2_ES => "Dolby_AC4_V2_ES",
        MFAudioFormat_Dolby_DDPlus => "Dolby_DDPlus",
        MFAudioFormat_FLAC => "FLAC",
        MFAudioFormat_Float => "Float",
        MFAudioFormat_Float_SpatialObjects => "Float_SpatialObjects",
        MFAudioFormat_LPCM => "LPCM",
        MFAudioFormat_MP3 => "MP3",
        MFAudioFormat_MPEG => "MPEG",
        MFAudioFormat_MSP1 => "MSP1",
        MFAudioFormat_Opus => "Opus",
        MFAudioFormat_PCM => "PCM",
        MFAudioFormat_PCM_HDCP => "PCM_HDCP",
        MFAudioFormat_Vorbis => "Vorbis",
        MFAudioFormat_WMASPDIF => "WMASPDIF",
        MFAudioFormat_WMAudioV8 => "WMAudioV8",
        MFAudioFormat_WMAudioV9 => "WMAudioV9",
        MFAudioFormat_WMAudio_Lossless => "WMAudio_Lossless",
        _ => "",
    };

    format.to_string()
}
