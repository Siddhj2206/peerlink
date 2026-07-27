use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use iroh_live::media::config::{H264, VideoCodec, VideoConfig};
use iroh_live::media::format::{EncodedFrame, MediaPacket};
use iroh_live::media::publish::{LocalBroadcast, VideoInput};
use iroh_live::media::subscribe::RemoteBroadcast;
use iroh_live::media::traits::PreEncodedVideoSource;
use iroh_live::media::transport::PacketSource;

use crate::demux::{read_frame_data, Mp4Index, VideoInfo};

pub fn config_from_index(info: &VideoInfo) -> VideoConfig {
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

pub struct Mp4PreEncodedSource {
    index: Arc<Mp4Index>,
    cursor: usize,
}

impl Mp4PreEncodedSource {
    pub fn new(index: Arc<Mp4Index>) -> Self {
        Self { index, cursor: 0 }
    }
}

impl PreEncodedVideoSource for Mp4PreEncodedSource {
    fn name(&self) -> &str {
        "mp4-demux"
    }

    fn config(&self) -> VideoConfig {
        config_from_index(&self.index.info)
    }

    fn start(&mut self) -> Result<()> {
        self.cursor = 0;
        Ok(())
    }

    fn pop_packet(&mut self) -> Result<Option<EncodedFrame>> {
        if self.cursor >= self.index.frames.len() {
            return Ok(None);
        }

        let entry = &self.index.frames[self.cursor];
        let data = read_frame_data(&self.index, self.cursor)?;
        self.cursor += 1;

        Ok(Some(EncodedFrame {
            is_keyframe: entry.is_keyframe,
            timestamp: entry.timestamp,
            payload: Bytes::from(data),
        }))
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct Mp4Subscriber {
    packets: Vec<MediaPacket>,
    cursor: usize,
}

impl Mp4Subscriber {
    pub async fn from_source(source: &mut impl PacketSource, count: usize) -> Result<Self> {
        let mut packets = Vec::with_capacity(count);
        for _ in 0..count {
            match source.read().await {
                Ok(Some(pkt)) => packets.push(pkt),
                Ok(None) => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Self { packets, cursor: 0 })
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    pub fn frames(&self) -> &[MediaPacket] {
        &self.packets
    }

    pub fn next_frame(&mut self) -> Option<MediaPacket> {
        if self.cursor >= self.packets.len() {
            return None;
        }
        let frame = self.packets[self.cursor].clone();
        self.cursor += 1;
        Some(frame)
    }
}

#[allow(dead_code)]
pub async fn start_publisher(
    index: Arc<Mp4Index>,
) -> Result<LocalBroadcast> {
    let broadcast = LocalBroadcast::new();

    let config = config_from_index(&index.info);
    let factory = move || -> Result<Box<dyn PreEncodedVideoSource>> {
        Ok(Box::new(Mp4PreEncodedSource::new(index.clone())))
    };

    broadcast
        .video()
        .set(VideoInput::pre_encoded("video/h264-mp4", config, factory))
        .context("failed to set video input")?;

    Ok(broadcast)
}

#[allow(dead_code)]
pub async fn start_subscriber(broadcast: RemoteBroadcast, frame_count: usize) -> Result<Mp4Subscriber> {
    let track_name = broadcast
        .catalog()
        .select_video_rendition(iroh_live::media::format::Quality::Highest)
        .context("no video renditions in catalog")?;

    let (mut packet_source, _config) = broadcast
        .raw_video_track(&track_name)
        .context("failed to get raw video track")?;

    Mp4Subscriber::from_source(&mut packet_source, frame_count).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demux::{FrameEntry, VideoInfo};
    use std::sync::Arc;
    use std::time::Duration;

    fn create_test_index() -> (Mp4Index, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.mp4");
        let frame_size = 512u32;

        let frame_bytes: Vec<u8> = (0..frame_size).map(|i| (i % 256) as u8).collect();

        let frames = vec![
            FrameEntry {
                offset: 0,
                size: frame_size,
                timestamp: Duration::from_secs_f64(0.0),
                is_keyframe: true,
                track_id: 1,
            },
            FrameEntry {
                offset: frame_size as u64,
                size: frame_size,
                timestamp: Duration::from_secs_f64(0.033),
                is_keyframe: false,
                track_id: 1,
            },
            FrameEntry {
                offset: 2 * frame_size as u64,
                size: frame_size,
                timestamp: Duration::from_secs_f64(0.066),
                is_keyframe: false,
                track_id: 1,
            },
            FrameEntry {
                offset: 3 * frame_size as u64,
                size: frame_size,
                timestamp: Duration::from_secs_f64(0.099),
                is_keyframe: true,
                track_id: 1,
            },
        ];

        let total = frames.len() as u64 * frame_size as u64;
        let mut data = Vec::with_capacity(total as usize);
        for _ in 0..frames.len() {
            data.extend_from_slice(&frame_bytes);
        }
        std::fs::write(&path, &data).unwrap();

        let info = VideoInfo {
            width: 320,
            height: 240,
            duration: Duration::from_secs_f64(0.099),
            frame_count: frames.len(),
            track_count: 1,
        };

        (Mp4Index { info, frames, path }, tmp)
    }

    #[test]
    fn test_mp4_pre_encoded_source_creates() {
        let (index, _tmp) = create_test_index();
        let source = Mp4PreEncodedSource::new(Arc::new(index));
        assert_eq!(source.name(), "mp4-demux");
    }

    #[test]
    fn test_mp4_pre_encoded_source_config() {
        let (index, _tmp) = create_test_index();
        let source = Mp4PreEncodedSource::new(Arc::new(index));
        let config = source.config();
        assert_eq!(config.coded_width, Some(320));
        assert_eq!(config.coded_height, Some(240));
    }

    #[test]
    fn test_mp4_pre_encoded_source_start_stop() {
        let (index, _tmp) = create_test_index();
        let mut source = Mp4PreEncodedSource::new(Arc::new(index));
        assert!(source.start().is_ok());
        assert!(source.stop().is_ok());
    }

    #[test]
    fn test_mp4_pre_encoded_source_pop_packets() {
        let (index, _tmp) = create_test_index();
        let mut source = Mp4PreEncodedSource::new(Arc::new(index));
        source.start().unwrap();

        let pkt = source.pop_packet().unwrap().unwrap();
        assert!(pkt.is_keyframe);
        assert!(!pkt.payload.is_empty());
        assert_eq!(pkt.payload.len(), 512);
        assert_eq!(pkt.timestamp, Duration::from_secs_f64(0.0));

        let pkt = source.pop_packet().unwrap().unwrap();
        assert!(!pkt.is_keyframe);
        assert_eq!(pkt.timestamp, Duration::from_secs_f64(0.033));

        let pkt = source.pop_packet().unwrap().unwrap();
        assert!(!pkt.is_keyframe);

        let pkt = source.pop_packet().unwrap().unwrap();
        assert!(pkt.is_keyframe);
        assert_eq!(pkt.timestamp, Duration::from_secs_f64(0.099));

        assert!(source.pop_packet().unwrap().is_none());
        source.stop().unwrap();
    }

    fn make_broadcast(index: Arc<Mp4Index>) -> LocalBroadcast {
        let broadcast = LocalBroadcast::new();
        let config = config_from_index(&index.info);
        broadcast
            .video()
            .set(VideoInput::pre_encoded(
                "video/h264-mp4",
                config,
                move || -> Result<Box<dyn PreEncodedVideoSource>> {
                    Ok(Box::new(Mp4PreEncodedSource::new(index.clone())))
                },
            ))
            .unwrap();
        broadcast
    }

    #[tokio::test]
    async fn test_start_subscriber() {
        let (index, _tmp) = create_test_index();
        let index = Arc::new(index);
        let frame_count = index.frames.len();

        let broadcast = make_broadcast(index.clone());
        let consumer = broadcast.consume();
        let remote = RemoteBroadcast::new("test", consumer).await.unwrap();

        let subscriber = start_subscriber(remote, frame_count).await.unwrap();
        assert_eq!(subscriber.len(), frame_count);

        let frames = subscriber.frames();
        assert!(frames[0].is_keyframe);
    }

    #[tokio::test]
    async fn test_local_broadcast_roundtrip_frame_data() {
        let (index, _tmp) = create_test_index();
        let index = Arc::new(index);
        let frame_count = index.frames.len();

        let broadcast = make_broadcast(index);
        let consumer = broadcast.consume();
        let remote = RemoteBroadcast::new("test", consumer).await.unwrap();

        let track_name = remote
            .catalog()
            .select_video_rendition(iroh_live::media::format::Quality::Highest)
            .unwrap();

        let (mut packet_source, _config) = remote.raw_video_track(&track_name).unwrap();

        let mut received = 0usize;
        for _ in 0..frame_count {
            match packet_source.read().await.unwrap() {
                Some(pkt) => {
                    received += 1;
                    use bytes::Buf;
                    assert!(pkt.payload.has_remaining(), "payload should not be empty");
                }
                None => break,
            }
        }

        assert_eq!(received, frame_count);
    }
}
