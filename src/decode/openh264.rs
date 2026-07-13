use openh264::decoder::Decoder as Openh264Api;
use openh264::formats::YUVSource;

use crate::decode::{DecodedFrame, Decoder};

pub struct Openh264Decoder {
    decoder: Openh264Api,
}

impl Openh264Decoder {
    pub fn new() -> Result<Self, openh264::Error> {
        let decoder = Openh264Api::new()?;
        Ok(Self { decoder })
    }
}

impl Decoder for Openh264Decoder {
    fn decode(&mut self, data: &[u8]) -> Result<DecodedFrame, String> {
        let yuv = self
            .decoder
            .decode(data)
            .map_err(|e| format!("openh264 decode: {e}"))?
            .ok_or_else(|| "no frame from decoder".to_string())?;

        let (width, height) = yuv.dimensions();
        let mut rgba = vec![0u8; width * height * 4];
        yuv.write_rgba8(&mut rgba);

        Ok(DecodedFrame {
            data: rgba,
            width: width as u32,
            height: height as u32,
        })
    }
}
