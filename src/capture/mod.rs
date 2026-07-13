pub mod x11;
pub mod wayland;

use std::os::unix::io::OwnedFd;

#[derive(Debug)]
pub struct DmaBufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
    pub size: u32,
}

#[derive(Debug)]
pub enum CaptureFrame {
    Rgba {
        data: Vec<u8>,
        width: u32,
        height: u32,
    },
    DmaBuf {
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
        planes: Vec<DmaBufPlane>,
    },
}

#[derive(Debug)]
pub enum CaptureError {
    NoFrame,
    Disconnected,
    Io(std::io::Error),
    NotInitialized,
    Other(String),
}

impl From<std::io::Error> for CaptureError {
    fn from(e: std::io::Error) -> Self {
        CaptureError::Io(e)
    }
}

pub trait Capture {
    fn capture_frame(&mut self) -> Result<CaptureFrame, CaptureError>;
    fn resolution(&self) -> (u32, u32);
}
