pub mod openh264;

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub trait Decoder {
    fn decode(&mut self, data: &[u8]) -> Result<DecodedFrame, String>;
}
