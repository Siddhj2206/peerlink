pub mod openh264;
pub mod nvenc;
pub mod vaapi;

use crate::capture::CaptureFrame;

pub trait Encoder {
    fn encode(&mut self, frame: &CaptureFrame) -> Result<Vec<u8>, String>;
}

pub enum EncoderKind {
    Openh264,
    Vaapi,
    Nvenc,
}
