use std::time::{Duration, Instant};

use anyhow::Result;
use rusty_codecs::codec::H264VideoDecoder as RustH264Decoder;
use rusty_codecs::config::{VideoCodec, VideoConfig, H264};
use rusty_codecs::format::{DecodeConfig, MediaPacket};
use rusty_codecs::traits::VideoDecoder;

use crate::demux::{read_frame_data, Mp4Index, VideoInfo};

pub fn config_from_video_info(info: &VideoInfo) -> VideoConfig {
    VideoConfig {
        codec: VideoCodec::H264(H264 {
            inline: false,
            profile: 0x42,
            constraints: 0,
            level: 0x1E,
        }),
        description: None,
        coded_width: Some(info.width as u32),
        coded_height: Some(info.height as u32),
        display_ratio_width: None,
        display_ratio_height: None,
        bitrate: None,
        framerate: None,
        optimize_for_latency: None,
    }
}

pub fn nal_to_media_packet(data: &[u8], timestamp: Duration, is_keyframe: bool) -> MediaPacket {
    MediaPacket {
        timestamp,
        payload: data.to_vec().into(),
        is_keyframe,
    }
}

#[derive(Debug, Clone)]
pub struct DecodeStats {
    pub frames_decoded: u64,
    pub total_decode_time: Duration,
    pub avg_decode_time: Duration,
    pub max_decode_time: Duration,
    pub min_decode_time: Duration,
    pub last_fps: f64,
    fps_start: Instant,
    fps_frames: u64,
}

impl Default for DecodeStats {
    fn default() -> Self {
        Self {
            frames_decoded: 0,
            total_decode_time: Duration::ZERO,
            avg_decode_time: Duration::ZERO,
            max_decode_time: Duration::ZERO,
            min_decode_time: Duration::MAX,
            last_fps: 0.0,
            fps_start: Instant::now(),
            fps_frames: 0,
        }
    }
}

impl DecodeStats {
    pub fn record_decode(&mut self, elapsed: Duration) {
        self.frames_decoded += 1;
        self.total_decode_time += elapsed;
        self.max_decode_time = self.max_decode_time.max(elapsed);
        if elapsed < self.min_decode_time {
            self.min_decode_time = elapsed;
        }
        self.avg_decode_time = self.total_decode_time / self.frames_decoded as u32;

        self.fps_frames += 1;
        let elapsed_secs = self.fps_start.elapsed().as_secs_f64();
        if elapsed_secs >= 1.0 {
            self.last_fps = self.fps_frames as f64 / elapsed_secs;
            self.fps_frames = 0;
            self.fps_start = Instant::now();
        }
    }
}

pub struct DecodedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub timestamp: Duration,
}

pub fn decode_all_frames(
    index: &Mp4Index,
    config: &VideoConfig,
) -> Result<(Vec<DecodedFrame>, DecodeStats)> {
    let decode_config = DecodeConfig::default();
    let mut decoder = RustH264Decoder::new(config, &decode_config)?;
    let mut decoded = Vec::new();
    let mut stats = DecodeStats::default();

    for i in 0..index.frames.len() {
        let entry = &index.frames[i];
        let data = read_frame_data(index, i)?;
        let pkt = nal_to_media_packet(&data, entry.timestamp, entry.is_keyframe);

        let start = Instant::now();
        decoder.push_packet(pkt)?;

        while let Some(vf) = decoder.pop_frame()? {
            let rgba = vf.rgba_image();
            decoded.push(DecodedFrame {
                data: rgba.as_raw().to_vec(),
                width: rgba.width(),
                height: rgba.height(),
                timestamp: vf.timestamp,
            });
        }
        stats.record_decode(start.elapsed());
    }

    while let Some(vf) = decoder.pop_frame()? {
        let rgba = vf.rgba_image();
        decoded.push(DecodedFrame {
            data: rgba.as_raw().to_vec(),
            width: rgba.width(),
            height: rgba.height(),
            timestamp: vf.timestamp,
        });
    }

    Ok((decoded, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_video_info() {
        let info = VideoInfo {
            width: 640,
            height: 480,
            duration: Duration::ZERO,
            frame_count: 0,
            track_count: 0,
        };
        let config = config_from_video_info(&info);
        assert_eq!(config.coded_width, Some(640));
        assert_eq!(config.coded_height, Some(480));
        assert!(matches!(config.codec, VideoCodec::H264(_)));
    }

    #[test]
    fn test_nal_to_media_packet() {
        let data = vec![0u8, 0u8, 0u8, 1u8, 0x67u8, 0x42u8];
        let pkt = nal_to_media_packet(&data, Duration::from_millis(100), true);
        assert!(pkt.is_keyframe);
        assert_eq!(pkt.timestamp, Duration::from_millis(100));
        use bytes::Buf;
        assert!(pkt.payload.has_remaining());
    }

    #[test]
    fn test_decode_stats_default() {
        let stats = DecodeStats::default();
        assert_eq!(stats.frames_decoded, 0);
        assert_eq!(stats.min_decode_time, Duration::MAX);
    }

    #[test]
    fn test_decode_stats_record() {
        let mut stats = DecodeStats::default();
        stats.record_decode(Duration::from_millis(10));
        assert_eq!(stats.frames_decoded, 1);
        assert_eq!(stats.total_decode_time, Duration::from_millis(10));
        assert_eq!(stats.max_decode_time, Duration::from_millis(10));
        assert_eq!(stats.min_decode_time, Duration::from_millis(10));
    }

    #[test]
    fn test_decode_stats_record_multiple() {
        let mut stats = DecodeStats::default();
        stats.record_decode(Duration::from_millis(10));
        stats.record_decode(Duration::from_millis(5));
        assert_eq!(stats.frames_decoded, 2);
        assert_eq!(stats.total_decode_time, Duration::from_millis(15));
        assert_eq!(stats.avg_decode_time, Duration::from_nanos(7_500_000));
        assert_eq!(stats.max_decode_time, Duration::from_millis(10));
        assert_eq!(stats.min_decode_time, Duration::from_millis(5));
    }

    fn make_solid_frame(w: u32, h: u32, r: u8, g: u8, b: u8) -> rusty_codecs::format::VideoFrame {
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                data[i] = r;
                data[i + 1] = g;
                data[i + 2] = b;
                data[i + 3] = 255;
            }
        }
        rusty_codecs::format::VideoFrame::new_rgba(data.into(), w, h, Duration::ZERO)
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        use rusty_codecs::codec::h264::H264Encoder;
        use rusty_codecs::format::VideoPreset;
        use rusty_codecs::traits::{VideoEncoder, VideoEncoderFactory};

        let w = 320u32;
        let h = 180u32;
        let mut encoder = H264Encoder::with_preset(VideoPreset::P180).unwrap();

        let input_frames: Vec<_> = (0..5)
            .map(|i| make_solid_frame(w, h, (i * 50) as u8, 100, 200))
            .collect();

        let mut encoded = Vec::new();
        for frame in &input_frames {
            encoder.push_frame(frame.clone()).unwrap();
            while let Some(pkt) = encoder.pop_packet().unwrap() {
                encoded.push(pkt);
            }
        }

        let config = encoder.config();
        let decode_config = DecodeConfig::default();
        let mut decoder = RustH264Decoder::new(&config, &decode_config).unwrap();

        let mut decoded_count = 0;
        for pkt in &encoded {
            let mp = MediaPacket {
                timestamp: pkt.timestamp,
                payload: pkt.payload.clone().into(),
                is_keyframe: pkt.is_keyframe,
            };
            decoder.push_packet(mp).unwrap();
            while let Some(vf) = decoder.pop_frame().unwrap() {
                let img = vf.rgba_image();
                assert_eq!(img.width(), w);
                assert_eq!(img.height(), h);
                decoded_count += 1;
            }
        }
        while let Some(vf) = decoder.pop_frame().unwrap() {
            let _ = vf.rgba_image();
            decoded_count += 1;
        }

        assert!(
            decoded_count >= 3,
            "expected >= 3 decoded frames, got {decoded_count}"
        );
    }

    #[test]
    fn test_decode_all_frames_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.mp4");
        let index = Mp4Index {
            info: VideoInfo {
                width: 320,
                height: 240,
                duration: Duration::ZERO,
                frame_count: 0,
                track_count: 1,
            },
            frames: vec![],
            path,
        };
        let config = config_from_video_info(&index.info);
        let result = decode_all_frames(&index, &config);
        assert!(result.is_err() || result.unwrap().0.is_empty());
    }

    #[test]
    fn test_decode_nal_errors_on_garbage() {
        let config = VideoConfig {
            codec: VideoCodec::H264(H264 {
                inline: false,
                profile: 0x42,
                constraints: 0,
                level: 0x1E,
            }),
            description: None,
            coded_width: Some(320),
            coded_height: Some(240),
            display_ratio_width: None,
            display_ratio_height: None,
            bitrate: None,
            framerate: None,
            optimize_for_latency: None,
        };
        let decode_config = DecodeConfig::default();
        let mut decoder = RustH264Decoder::new(&config, &decode_config).unwrap();

        let garbage = vec![0xffu8; 1024];
        let pkt = nal_to_media_packet(&garbage, Duration::ZERO, true);
        let result = decoder.push_packet(pkt);
        assert!(result.is_err());
    }
}
