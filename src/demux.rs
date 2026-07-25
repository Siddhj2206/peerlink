use std::path::{Path, PathBuf};
use std::time::Duration;

use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};
use shiguredo_mp4::TrackKind;

#[derive(Debug, Clone)]
pub struct FrameEntry {
    pub offset: u64,
    pub size: u32,
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

#[derive(Debug, Clone)]
pub struct Mp4Index {
    pub info: VideoInfo,
    pub frames: Vec<FrameEntry>,
    pub path: PathBuf,
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

pub fn demux_file(path: &Path) -> Result<Mp4Index, DemuxError> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();

    let mut demuxer = Mp4FileDemuxer::new();
    let mut buf = Vec::new();

    while let Some(required) = demuxer.required_input() {
        let size = required.size.unwrap_or((file_len - required.position) as usize);
        buf.resize(size, 0u8);
        file.seek(SeekFrom::Start(required.position))?;
        file.read_exact(&mut buf)?;
        demuxer.handle_input(Input {
            position: required.position,
            data: &buf,
        });
    }

    let tracks = demuxer.tracks()?;

    let video_track = tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .ok_or(DemuxError::NoVideoTrack)?;

    let video_track_id = video_track.track_id;
    let timescale = video_track.timescale.get() as f64;
    let duration = if timescale > 0.0 {
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

                frames.push(FrameEntry {
                    offset: sample.data_offset,
                    size: sample.data_size as u32,
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
        duration,
        frame_count: frames.len(),
        track_count,
    };

    Ok(Mp4Index {
        info,
        frames,
        path: path.to_path_buf(),
    })
}

#[allow(dead_code)]
pub fn read_frame_data(index: &Mp4Index, frame_index: usize) -> Result<Vec<u8>, DemuxError> {
    use std::io::{Read, Seek, SeekFrom};

    let entry = &index.frames[frame_index];
    let mut file = std::fs::File::open(&index.path)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut buf = vec![0u8; entry.size as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn seek_to_keyframe(frames: &[FrameEntry], target: Duration) -> usize {
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

    fn sample_entry(timestamp_secs: f64, is_keyframe: bool) -> FrameEntry {
        let timestamp = Duration::from_secs_f64(timestamp_secs);
        FrameEntry {
            offset: 0,
            size: 0,
            timestamp,
            is_keyframe,
            track_id: 1,
        }
    }

    #[test]
    fn test_seek_to_keyframe_exact() {
        let frames = vec![
            sample_entry(0.0, true),
            sample_entry(1.0, false),
            sample_entry(2.0, true),
            sample_entry(3.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(2.0)), 2);
    }

    #[test]
    fn test_seek_to_keyframe_before_first() {
        let frames = vec![
            sample_entry(1.0, true),
            sample_entry(2.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(0.0)), 0);
    }

    #[test]
    fn test_seek_to_keyframe_after_last() {
        let frames = vec![
            sample_entry(0.0, true),
            sample_entry(1.0, false),
            sample_entry(2.0, true),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(5.0)), 2);
    }

    #[test]
    fn test_seek_to_keyframe_between_keyframes() {
        let frames = vec![
            sample_entry(0.0, true),
            sample_entry(1.0, false),
            sample_entry(2.0, false),
            sample_entry(3.0, true),
            sample_entry(4.0, false),
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
            sample_entry(0.0, false),
            sample_entry(1.0, true),
            sample_entry(2.0, false),
            sample_entry(3.0, true),
            sample_entry(4.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(2.0)), 1);
    }

    #[test]
    fn test_seek_to_keyframe_after_last_no_keyframe_at_end() {
        let frames = vec![
            sample_entry(0.0, true),
            sample_entry(1.0, false),
            sample_entry(2.0, false),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(5.0)), 0);
    }

    #[test]
    fn test_seek_to_keyframe_all_keyframes() {
        let frames = vec![
            sample_entry(0.0, true),
            sample_entry(1.0, true),
            sample_entry(2.0, true),
        ];
        assert_eq!(seek_to_keyframe(&frames, Duration::from_secs_f64(1.5)), 1);
    }

    #[test]
    fn test_frame_entry_construction() {
        let entry = FrameEntry {
            offset: 1234,
            size: 5678,
            timestamp: Duration::from_secs_f64(5.0),
            is_keyframe: true,
            track_id: 1,
        };
        assert_eq!(entry.offset, 1234);
        assert_eq!(entry.size, 5678);
        assert_eq!(entry.timestamp, Duration::from_secs_f64(5.0));
        assert!(entry.is_keyframe);
    }

    #[test]
    fn test_frame_entry_non_keyframe() {
        let entry = FrameEntry {
            offset: 0,
            size: 0,
            timestamp: Duration::ZERO,
            is_keyframe: false,
            track_id: 1,
        };
        assert!(!entry.is_keyframe);
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
