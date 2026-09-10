use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::Result;
use rusty_codecs::codec::H264VideoDecoder as RustH264Decoder;
use rusty_codecs::config::{VideoCodec, VideoConfig, H264};
use rusty_codecs::format::{DecodeConfig, MediaPacket};
use rusty_codecs::traits::VideoDecoder;

use crate::demux::{read_frame_data, seek_to_keyframe, Mp4Index, VideoInfo};

pub fn config_from_video_info(info: &VideoInfo) -> VideoConfig {
    VideoConfig {
        codec: VideoCodec::H264(H264 {
            inline: false,
            profile: info.h264_profile,
            constraints: info.h264_constraints,
            level: info.h264_level,
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
    pub total_input_bytes: u64,
    pub total_decoded_pixels: u64,
    pub display_latency: Duration,
    pub display_latency_recorded: bool,
    pub network_latency: Duration,
    pub network_latency_recorded: bool,
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
            total_input_bytes: 0,
            total_decoded_pixels: 0,
            display_latency: Duration::ZERO,
            display_latency_recorded: false,
            network_latency: Duration::ZERO,
            network_latency_recorded: false,
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

    pub fn record_bytes(&mut self, input_bytes: u64, decoded_pixels: u64) {
        self.total_input_bytes += input_bytes;
        self.total_decoded_pixels += decoded_pixels;
    }

    pub fn record_display_latency(&mut self, elapsed: Duration) {
        self.display_latency = elapsed;
        self.display_latency_recorded = true;
    }

    pub fn record_network_latency(&mut self, elapsed: Duration) {
        self.network_latency = elapsed;
        self.network_latency_recorded = true;
    }

    pub fn input_throughput_mbps(&self) -> f64 {
        if self.total_decode_time.is_zero() {
            return 0.0;
        }
        let bits = self.total_input_bytes as f64 * 8.0;
        let secs = self.total_decode_time.as_secs_f64();
        bits / secs / 1_000_000.0
    }

    pub fn output_throughput_mbps(&self) -> f64 {
        if self.total_decode_time.is_zero() {
            return 0.0;
        }
        let bits = self.total_decoded_pixels as f64 * 4.0 * 8.0;
        let secs = self.total_decode_time.as_secs_f64();
        bits / secs / 1_000_000.0
    }

}

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp: Duration,
}

pub struct StreamingDecoder {
    index: Mp4Index,
    config: VideoConfig,
    decode_config: DecodeConfig,
    decoder: RustH264Decoder,
    pushed_to: Option<usize>,
    cache: VecDeque<(usize, DecodedFrame)>,
    stats: DecodeStats,
}

impl StreamingDecoder {
    pub fn new(index: Mp4Index, config: &VideoConfig) -> Result<Self> {
        let decode_config = DecodeConfig::default();
        let decoder = RustH264Decoder::new(config, &decode_config)?;
        Ok(Self {
            index,
            config: config.clone(),
            decode_config,
            decoder,
            pushed_to: None,
            cache: VecDeque::new(),
            stats: DecodeStats::default(),
        })
    }

    pub fn frame_count(&self) -> usize {
        self.index.frames.len()
    }

    fn index_for_timestamp(&self, ts: Duration) -> Option<usize> {
        self.index.frames.iter().position(|f| f.timestamp == ts)
    }

    fn recreate_at_keyframe(&mut self, target: usize) -> Result<()> {
        let kf = if self.index.frames.is_empty() {
            0
        } else {
            seek_to_keyframe(&self.index.frames, self.index.frames[target].timestamp)
        };
        self.decoder = RustH264Decoder::new(&self.config, &self.decode_config)?;
        self.pushed_to = Some(kf);
        self.cache.clear();
        Ok(())
    }

    pub fn decode_frame(&mut self, target: usize) -> Result<DecodedFrame> {
        if target >= self.index.frames.len() {
            anyhow::bail!(
                "frame index {target} out of bounds ({} frames)",
                self.index.frames.len()
            );
        }

        // Warm cache hit.
        if let Some(pos) = self.cache.iter().position(|(idx, _)| *idx == target) {
            return Ok(self.cache[pos].1.clone());
        }

        // If we've already pushed past the target without caching it, seek
        // back to the nearest keyframe and re-decode.
        if self.pushed_to.map_or(false, |p| p > target) && self.cache.is_empty() {
            self.recreate_at_keyframe(target)?;
        }

        if self.pushed_to.is_none() {
            self.pushed_to = Some(0);
        }

        // Keep pushing frames and caching decoder output until we hit the
        // target.  H.264 decoders have a look-ahead delay, so we may need to
        // push extra frames before output appears.
        while let Some(p) = self.pushed_to {
            if p >= self.index.frames.len() {
                break;
            }

            let i = p;
            let entry = &self.index.frames[i];
            let data = read_frame_data(&self.index, i)?;
            let input_bytes = data.len() as u64;
            let pkt = nal_to_media_packet(&data, entry.timestamp, entry.is_keyframe);

            let start = Instant::now();
            self.decoder.push_packet(pkt)?;
            self.stats.record_decode(start.elapsed());

            let push_idx = i;
            self.pushed_to = Some(push_idx + 1);

            // Drain all output frames from the decoder into our cache.
            let mut found_target = false;
            while let Some(vf) = self.decoder.pop_frame()? {
                if let Some(out_idx) = self.index_for_timestamp(vf.timestamp) {
                    let rgba = vf.rgba_image();
                    let pixels = (rgba.width() * rgba.height()) as u64;
                    self.stats.record_bytes(input_bytes, pixels);
                    let frame = DecodedFrame {
                        data: rgba.as_raw().to_vec(),
                        width: rgba.width(),
                        height: rgba.height(),
                        timestamp: vf.timestamp,
                    };
                    if out_idx == target {
                        found_target = true;
                    }
                    if !self.cache.iter().any(|(idx, _)| *idx == out_idx) {
                        self.cache.push_back((out_idx, frame));
                    }
                }
            }

            // If we just found the target in the drain, return from cache.
            if found_target {
                let pos = self
                    .cache
                    .iter()
                    .position(|(idx, _)| *idx == target)
                    .expect("just inserted");
                return Ok(self.cache[pos].1.clone());
            }
        }

        // Flush remaining buffered frames from the decoder.
        while let Some(vf) = self.decoder.pop_frame()? {
            if let Some(out_idx) = self.index_for_timestamp(vf.timestamp) {
                let rgba = vf.rgba_image();
                let pixels = (rgba.width() * rgba.height()) as u64;
                self.stats.record_bytes(0, pixels);
                let frame = DecodedFrame {
                    data: rgba.as_raw().to_vec(),
                    width: rgba.width(),
                    height: rgba.height(),
                    timestamp: vf.timestamp,
                };
                if out_idx == target {
                    return Ok(frame);
                }
                if !self.cache.iter().any(|(idx, _)| *idx == out_idx) {
                    self.cache.push_back((out_idx, frame));
                }
            }
        }

        anyhow::bail!("no decoded frame produced for index {target}")
    }

    pub fn stats(&self) -> &DecodeStats {
        &self.stats
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demux::FrameEntry;

    #[test]
    fn test_config_from_video_info() {
        let info = VideoInfo {
            width: 640,
            height: 480,
            duration: Duration::ZERO,
            frame_count: 0,
            track_count: 0,
            h264_profile: 0x42,
            h264_constraints: 0,
            h264_level: 0x1E,
        };
        let config = config_from_video_info(&info);
        assert_eq!(config.coded_width, Some(640));
        assert_eq!(config.coded_height, Some(480));
        assert!(matches!(config.codec, VideoCodec::H264(_)));
        if let VideoCodec::H264(h264) = config.codec {
            assert_eq!(h264.profile, 0x42);
            assert_eq!(h264.constraints, 0);
            assert_eq!(h264.level, 0x1E);
        }
    }

    #[test]
    fn test_config_from_video_info_uses_profile_from_info() {
        let info = VideoInfo {
            width: 320,
            height: 240,
            duration: Duration::ZERO,
            frame_count: 0,
            track_count: 1,
            h264_profile: 0x64,
            h264_constraints: 0x80,
            h264_level: 0x28,
        };
        let config = config_from_video_info(&info);
        if let VideoCodec::H264(h264) = config.codec {
            assert_eq!(h264.profile, 0x64);
            assert_eq!(h264.constraints, 0x80);
            assert_eq!(h264.level, 0x28);
        }
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
        assert_eq!(stats.total_input_bytes, 0);
        assert_eq!(stats.total_decoded_pixels, 0);
        assert!(!stats.display_latency_recorded);
        assert!(!stats.network_latency_recorded);
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

    #[test]
    fn test_decode_stats_record_bytes() {
        let mut stats = DecodeStats::default();
        stats.record_bytes(1000, 320 * 240);
        assert_eq!(stats.total_input_bytes, 1000);
        assert_eq!(stats.total_decoded_pixels, 320 * 240);

        stats.record_bytes(500, 640 * 480);
        assert_eq!(stats.total_input_bytes, 1500);
        assert_eq!(stats.total_decoded_pixels, 320 * 240 + 640 * 480);
    }

    #[test]
    fn test_decode_stats_latency() {
        let mut stats = DecodeStats::default();
        stats.record_display_latency(Duration::from_millis(16));
        assert!(stats.display_latency_recorded);
        assert_eq!(stats.display_latency, Duration::from_millis(16));

        stats.record_network_latency(Duration::from_millis(50));
        assert!(stats.network_latency_recorded);
        assert_eq!(stats.network_latency, Duration::from_millis(50));
    }

    #[test]
    fn test_throughput_calculation() {
        let mut stats = DecodeStats::default();
        stats.record_decode(Duration::from_secs(1));
        stats.record_bytes(125_000, 1920 * 1080);
        let input_mbps = stats.input_throughput_mbps();
        let output_mbps = stats.output_throughput_mbps();
        assert!((input_mbps - 1.0).abs() < 0.01);
        assert!(output_mbps > 0.0);
    }

    #[test]
    fn test_throughput_zero_when_no_decode() {
        let stats = DecodeStats::default();
        assert_eq!(stats.input_throughput_mbps(), 0.0);
        assert_eq!(stats.output_throughput_mbps(), 0.0);
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

    fn make_mock_index(encoded: &[Vec<u8>], keyframes: &[bool]) -> (Mp4Index, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.mp4");
        let mut data = Vec::new();
        let mut offset = 0u64;
        let mut frames = Vec::new();

        for (i, frame_data) in encoded.iter().enumerate() {
            let size = frame_data.len() as u32;
            data.extend_from_slice(frame_data);
            frames.push(FrameEntry {
                offset,
                size,
                timestamp: Duration::from_secs_f64(i as f64 / 30.0),
                is_keyframe: keyframes[i],
                track_id: 1,
            });
            offset += size as u64;
        }

        std::fs::write(&path, &data).unwrap();

        let info = VideoInfo {
            width: 320,
            height: 240,
            duration: Duration::from_secs_f64(0.0),
            frame_count: frames.len(),
            track_count: 1,
            h264_profile: 0x42,
            h264_constraints: 0,
            h264_level: 0x1E,
        };

        (Mp4Index { info, frames, path }, tmp)
    }

    #[test]
    fn test_streaming_decoder_new() {
        let (index, _tmp) = make_mock_index(&[], &[]);
        let config = config_from_video_info(&index.info);
        let decoder = StreamingDecoder::new(index, &config);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_streaming_decoder_sequential() {
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

        let keyframes: Vec<_> = encoded.iter().map(|p| p.is_keyframe).collect();
        let raw_data: Vec<_> = encoded.iter().map(|p| p.payload.to_vec()).collect();
        let (index, _tmp) = make_mock_index(&raw_data, &keyframes);

        let config = config_from_video_info(&index.info);
        let mut decoder = StreamingDecoder::new(index, &config).unwrap();

        let frame0 = decoder.decode_frame(0).unwrap();
        assert_eq!(frame0.width, w);
        assert_eq!(frame0.height, h);

        let frame1 = decoder.decode_frame(1).unwrap();
        assert_eq!(frame1.width, w);

        assert!(decoder.stats().frames_decoded >= 2);
    }

    #[test]
    fn test_streaming_decoder_seek_forward() {
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

        let keyframes: Vec<_> = encoded.iter().map(|p| p.is_keyframe).collect();
        let raw_data: Vec<_> = encoded.iter().map(|p| p.payload.to_vec()).collect();
        let (index, _tmp) = make_mock_index(&raw_data, &keyframes);

        let config = config_from_video_info(&index.info);
        let mut decoder = StreamingDecoder::new(index, &config).unwrap();

        let frame0 = decoder.decode_frame(0).unwrap();
        assert_eq!(frame0.width, w);

        let frame4 = decoder.decode_frame(4).unwrap();
        assert_eq!(frame4.width, w);
    }

    #[test]
    fn test_streaming_decoder_seek_backward() {
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

        let keyframes: Vec<_> = encoded.iter().map(|p| p.is_keyframe).collect();
        let raw_data: Vec<_> = encoded.iter().map(|p| p.payload.to_vec()).collect();
        let (index, _tmp) = make_mock_index(&raw_data, &keyframes);

        let config = config_from_video_info(&index.info);
        let mut decoder = StreamingDecoder::new(index, &config).unwrap();

        let frame4 = decoder.decode_frame(4).unwrap();
        assert_eq!(frame4.width, w);

        let frame0 = decoder.decode_frame(0).unwrap();
        assert_eq!(frame0.width, w);
    }

    #[test]
    fn test_streaming_decoder_out_of_bounds() {
        let (index, _tmp) = make_mock_index(&[], &[]);
        let config = config_from_video_info(&index.info);
        let mut decoder = StreamingDecoder::new(index, &config).unwrap();
        let result = decoder.decode_frame(0);
        assert!(result.is_err());
    }
}
