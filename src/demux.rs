use std::path::Path;
use std::time::Duration;

use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};
use shiguredo_mp4::TrackKind;

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub timestamp: Duration,
    pub is_keyframe: bool,
    pub track_id: u32,
}

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: u16,
    pub height: u16,
    pub duration: Duration,
    pub frame_count: usize,
    pub track_count: usize,
}

#[derive(Debug)]
pub enum DemuxError {
    Io(std::io::Error),
    Shiguredo(String),
    NoVideoTrack,
}

impl std::fmt::Display for DemuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Shiguredo(s) => write!(f, "Demux error: {s}"),
            Self::NoVideoTrack => write!(f, "No video track found"),
        }
    }
}

impl std::error::Error for DemuxError {}

impl From<std::io::Error> for DemuxError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<shiguredo_mp4::demux::DemuxError> for DemuxError {
    fn from(e: shiguredo_mp4::demux::DemuxError) -> Self {
        Self::Shiguredo(e.to_string())
    }
}

pub fn demux_file(path: &Path) -> Result<(VideoInfo, Vec<VideoFrame>), DemuxError> {
    let file_data = std::fs::read(path)?;
    let file_len = file_data.len() as u64;

    let mut demuxer = Mp4FileDemuxer::new();

    while let Some(required) = demuxer.required_input() {
        let size = required.size.unwrap_or((file_len - required.position) as usize);
        let end = (required.position + size as u64).min(file_len);
        let data = &file_data[required.position as usize..end as usize];
        let input = Input {
            position: required.position,
            data,
        };
        demuxer.handle_input(input);
    }

    let tracks = demuxer.tracks()?;

    let video_track = tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .ok_or(DemuxError::NoVideoTrack)?;

    let video_track_id = video_track.track_id;
    let timescale = video_track.timescale.get() as f64;
    let duration_secs = if timescale > 0.0 {
        Duration::from_secs_f64(video_track.duration as f64 / timescale)
    } else {
        Duration::ZERO
    };
    let track_count = tracks.len();
    let mut width = 0u16;
    let mut height = 0u16;

    let mut frames = Vec::new();

    loop {
        match demuxer.next_sample() {
            Ok(Some(sample)) => {
                if sample.track.track_id != video_track_id {
                    continue;
                }

                if width == 0 && height == 0
                    && let Some(sample_entry) = sample.sample_entry
                    && let Some((w, h)) = sample_entry.video_resolution()
                {
                    width = w;
                    height = h;
                }

                let timestamp = if timescale > 0.0 {
                    Duration::from_secs_f64(sample.timestamp as f64 / timescale)
                } else {
                    Duration::ZERO
                };

                let end = (sample.data_offset + sample.data_size as u64).min(file_len);
                let data = file_data[sample.data_offset as usize..end as usize].to_vec();

                frames.push(VideoFrame {
                    data,
                    timestamp,
                    is_keyframe: sample.keyframe,
                    track_id: sample.track.track_id,
                });
            }
            Ok(None) => break,
            Err(e) => return Err(e.into()),
        }
    }

    let info = VideoInfo {
        width,
        height,
        duration: duration_secs,
        frame_count: frames.len(),
        track_count,
    };

    Ok((info, frames))
}

pub fn seek_to_keyframe(frames: &[VideoFrame], target: Duration) -> usize {
    if frames.is_empty() {
        return 0;
    }

    let mut best = 0;
    for (i, frame) in frames.iter().enumerate() {
        if frame.is_keyframe && frame.timestamp <= target {
            best = i;
        }
        if frame.timestamp > target {
            break;
        }
    }
    let last = frames.len() - 1;
    if best == 0 && !frames[0].is_keyframe && target > frames[last].timestamp {
        let mut last_kf = 0;
        for (i, f) in frames.iter().enumerate() {
            if f.is_keyframe {
                last_kf = i;
            }
        }
        best = last_kf;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame(timestamp_secs: f64, is_keyframe: bool) -> VideoFrame {
        let timestamp = Duration::from_secs_f64(timestamp_secs);
        VideoFrame {
            data: vec![],
            timestamp,
            is_keyframe,
            track_id: 1,
        }
    }

    #[test]
    fn test_seek_to_keyframe_exact() {
        let frames = vec![
            sample_frame(0.0, true),
            sample_frame(1.0, false),
            sample_frame(2.0, true),
            sample_frame(3.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(2.0)), 2);
    }

    #[test]
    fn test_seek_to_keyframe_before_first() {
        let frames = vec![
            sample_frame(1.0, true),
            sample_frame(2.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(0.0)), 0);
    }

    #[test]
    fn test_seek_to_keyframe_after_last() {
        let frames = vec![
            sample_frame(0.0, true),
            sample_frame(1.0, false),
            sample_frame(2.0, true),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(5.0)), 2);
    }

    #[test]
    fn test_seek_to_keyframe_between_keyframes() {
        let frames = vec![
            sample_frame(0.0, true),
            sample_frame(1.0, false),
            sample_frame(2.0, false),
            sample_frame(3.0, true),
            sample_frame(4.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(2.5)), 0);
    }

    #[test]
    fn test_seek_to_keyframe_empty() {
        let frames = vec![];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(1.0)), 0);
    }

    #[test]
    fn test_seek_to_keyframe_midpoint() {
        let frames = vec![
            sample_frame(0.0, false),
            sample_frame(1.0, true),
            sample_frame(2.0, false),
            sample_frame(3.0, true),
            sample_frame(4.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(2.0)), 1);
    }

    #[test]
    fn test_seek_to_keyframe_after_last_no_keyframe_at_end() {
        let frames = vec![
            sample_frame(0.0, true),
            sample_frame(1.0, false),
            sample_frame(2.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(5.0)), 0);
    }

    #[test]
    fn test_seek_to_keyframe_all_keyframes() {
        let frames = vec![
            sample_frame(0.0, true),
            sample_frame(1.0, true),
            sample_frame(2.0, true),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(1.5)), 1);
    }

    #[test]
    fn test_video_frame_construction() {
        let frame = VideoFrame {
            data: vec![0u8; 10],
            timestamp: Duration::from_secs_f64(5.0),
            is_keyframe: true,
            track_id: 1,
        };
        assert_eq!(frame.data.len(), 10);
        assert_eq!(frame.timestamp, Duration::from_secs_f64(5.0));
        assert!(frame.is_keyframe);
    }

    #[test]
    fn test_video_frame_non_keyframe() {
        let frame = VideoFrame {
            data: vec![],
            timestamp: Duration::ZERO,
            is_keyframe: false,
            track_id: 1,
        };
        assert!(!frame.is_keyframe);
    }

    #[test]
    fn test_demux_error_display() {
        let err = DemuxError::NoVideoTrack;
        assert_eq!(err.to_string(), "No video track found");

        let err = DemuxError::Shiguredo("parse error".into());
        assert_eq!(err.to_string(), "Demux error: parse error");

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DemuxError::Io(io_err);
        assert!(err.to_string().contains("IO error"));
    }
}
