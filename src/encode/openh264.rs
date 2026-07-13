use openh264::encoder::Encoder as Openh264Api;
use openh264::formats::{RgbaSliceU8, YUVBuffer};

use crate::capture::CaptureFrame;
use crate::encode::Encoder;

pub struct Openh264Encoder {
    encoder: Openh264Api,
}

impl Openh264Encoder {
    pub fn new() -> Result<Self, openh264::Error> {
        let encoder = Openh264Api::new()?;
        Ok(Self { encoder })
    }
}

impl Encoder for Openh264Encoder {
    fn encode(&mut self, frame: &CaptureFrame) -> Result<Vec<u8>, String> {
        match frame {
            CaptureFrame::Rgba { data, width, height } => {
                let w = *width as usize;
                let h = *height as usize;
                let src = RgbaSliceU8::new(data, (w, h));
                let yuv = YUVBuffer::from_rgb_source(src);
                let bitstream = self
                    .encoder
                    .encode(&yuv)
                    .map_err(|e| format!("openh264 encode: {e}"))?;
                Ok(bitstream.to_vec())
            }
            CaptureFrame::DmaBuf { .. } => {
                Err("OpenH264 encoder does not support DMA-BUF frames".into())
            }
        }
    }
}
