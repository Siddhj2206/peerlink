use crate::capture::CaptureFrame;
use crate::encode::Encoder;

pub struct NvencEncoder;

impl NvencEncoder {
    pub fn new() -> Result<Self, String> {
        Err("NVENC encoder not yet implemented".into())
    }
}

impl Encoder for NvencEncoder {
    fn encode(&mut self, _frame: &CaptureFrame) -> Result<Vec<u8>, String> {
        Err("NVENC encoder not yet implemented".into())
    }
}
