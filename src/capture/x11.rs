use xcap::Monitor;

use crate::capture::{Capture, CaptureError, CaptureFrame};

pub struct X11Capture {
    monitor: Monitor,
}

impl X11Capture {
    pub fn new() -> Option<Self> {
        let monitors = Monitor::all().ok()?;
        let monitor = monitors.into_iter().next()?;
        Some(Self { monitor })
    }
}

impl Capture for X11Capture {
    fn capture_frame(&mut self) -> Result<CaptureFrame, CaptureError> {
        let img = self
            .monitor
            .capture_image()
            .map_err(|e| CaptureError::Other(e.to_string()))?;
        let w = img.width();
        let h = img.height();
        Ok(CaptureFrame::Rgba {
            data: img.into_vec(),
            width: w,
            height: h,
        })
    }

    fn resolution(&self) -> (u32, u32) {
        (self.monitor.width().unwrap_or(0), self.monitor.height().unwrap_or(0))
    }
}
