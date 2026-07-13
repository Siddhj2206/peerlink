use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr;

use ffmpeg_next::ffi::*;

use crate::capture::CaptureFrame;
use crate::encode::Encoder;

pub struct VaapiEncoder {
    state: Option<EncoderState>,
}

struct EncoderState {
    hw_device_ctx: *mut AVBufferRef,
    hw_frames_ctx: *mut AVBufferRef,
    codec_ctx: *mut AVCodecContext,
    sws_ctx: *mut SwsContext,
    cpu_frame: *mut AVFrame,
    packet: *mut AVPacket,
    width: u32,
    height: u32,
}

impl VaapiEncoder {
    pub fn new() -> Result<Self, String> {
        Ok(Self { state: None })
    }

    fn init(&mut self, width: u32, height: u32) -> Result<(), String> {
        unsafe {
            let hw_device_ctx = create_vaapi_device()?;

            let codec_name = CString::new("h264_vaapi").unwrap();
            let codec = avcodec_find_encoder_by_name(codec_name.as_ptr());
            if codec.is_null() {
                return Err("h264_vaapi encoder not found".into());
            }

            let codec_ctx = avcodec_alloc_context3(codec);
            if codec_ctx.is_null() {
                return Err("avcodec_alloc_context3 failed".into());
            }

            (*codec_ctx).width = width as c_int;
            (*codec_ctx).height = height as c_int;
            (*codec_ctx).time_base = AVRational {
                num: 1,
                den: 60,
            };
            (*codec_ctx).framerate = AVRational {
                num: 60,
                den: 1,
            };
            (*codec_ctx).sample_aspect_ratio = AVRational {
                num: 1,
                den: 1,
            };
            (*codec_ctx).pix_fmt = AVPixelFormat::AV_PIX_FMT_VAAPI;
            (*codec_ctx).codec_type = AVMediaType::AVMEDIA_TYPE_VIDEO;
            (*codec_ctx).bit_rate = 4_000_000;
            (*codec_ctx).gop_size = 60;
            (*codec_ctx).max_b_frames = 0;
            (*codec_ctx).flags |= AV_CODEC_FLAG_GLOBAL_HEADER as c_int;

            let hw_frames_ctx =
                create_vaapi_frames_ctx(hw_device_ctx, width, height, AVPixelFormat::AV_PIX_FMT_NV12)?;
            (*codec_ctx).hw_frames_ctx = av_buffer_ref(hw_frames_ctx);

            let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
            if ret < 0 {
                return Err(format!("avcodec_open2 failed: {ret}"));
            }

            let cpu_frame = av_frame_alloc();
            if cpu_frame.is_null() {
                return Err("av_frame_alloc failed for cpu frame".into());
            }
            (*cpu_frame).format = AVPixelFormat::AV_PIX_FMT_NV12 as c_int;
            (*cpu_frame).width = width as c_int;
            (*cpu_frame).height = height as c_int;
            let ret = av_frame_get_buffer(cpu_frame, 0);
            if ret < 0 {
                return Err(format!("av_frame_get_buffer failed: {ret}"));
            }

            let sws_ctx = sws_getContext(
                width as c_int,
                height as c_int,
                AVPixelFormat::AV_PIX_FMT_RGBA,
                width as c_int,
                height as c_int,
                AVPixelFormat::AV_PIX_FMT_NV12,
                SwsFlags::SWS_BILINEAR as c_int,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if sws_ctx.is_null() {
                return Err("sws_getContext failed".into());
            }

            let packet = av_packet_alloc();
            if packet.is_null() {
                return Err("av_packet_alloc failed".into());
            }

            self.state = Some(EncoderState {
                hw_device_ctx,
                hw_frames_ctx,
                codec_ctx,
                sws_ctx,
                cpu_frame,
                packet,
                width,
                height,
            });

            Ok(())
        }
    }
}

impl Encoder for VaapiEncoder {
    fn encode(&mut self, frame: &CaptureFrame) -> Result<Vec<u8>, String> {
        unsafe {
            match frame {
                CaptureFrame::Rgba { data, width, height } => {
                    let w = *width;
                    let h = *height;

                    if self.state.is_none()
                        || self
                            .state
                            .as_ref()
                            .map_or(true, |s| s.width != w || s.height != h)
                    {
                        self.state = None;
                        self.init(w, h)?;
                    }

                    let state = self.state.as_mut().unwrap();

                    let mut src_frame = av_frame_alloc();
                    if src_frame.is_null() {
                        return Err("av_frame_alloc failed".into());
                    }
                    (*src_frame).format = AVPixelFormat::AV_PIX_FMT_RGBA as c_int;
                    (*src_frame).width = w as c_int;
                    (*src_frame).height = h as c_int;
                    (*src_frame).data[0] = data.as_ptr() as *mut u8;
                    (*src_frame).linesize[0] = (w * 4) as c_int;

                    let ret = sws_scale(
                        state.sws_ctx,
                        &(*src_frame).data as *const _ as *const *const u8,
                        (*src_frame).linesize.as_ptr(),
                        0,
                        h as c_int,
                        (*state.cpu_frame).data.as_mut_ptr(),
                        (*state.cpu_frame).linesize.as_mut_ptr(),
                    );
                    av_frame_free(&mut src_frame as *mut *mut AVFrame);
                    if ret < 0 {
                        return Err(format!("sws_scale failed: {ret}"));
                    }

                    let mut hw_frame = av_frame_alloc();
                    if hw_frame.is_null() {
                        return Err("av_frame_alloc failed for hw frame".into());
                    }
                    (*hw_frame).hw_frames_ctx = av_buffer_ref(state.hw_frames_ctx);
                    let ret = av_hwframe_get_buffer((*hw_frame).hw_frames_ctx, hw_frame, 0);
                    if ret < 0 {
                        av_frame_free(&mut hw_frame as *mut *mut AVFrame);
                        return Err(format!("av_hwframe_get_buffer failed: {ret}"));
                    }

                    let ret = av_hwframe_transfer_data(hw_frame, state.cpu_frame, 0);
                    if ret < 0 {
                        av_frame_free(&mut hw_frame as *mut *mut AVFrame);
                        return Err(format!("av_hwframe_transfer_data failed: {ret}"));
                    }

                    (*hw_frame).pts = AV_NOPTS_VALUE;
                    let ret = avcodec_send_frame(state.codec_ctx, hw_frame);
                    av_frame_free(&mut hw_frame as *mut *mut AVFrame);
                    if ret < 0 && ret != -(EAGAIN as c_int) {
                        return Err(format!("avcodec_send_frame failed: {ret}"));
                    }

                    let mut packets = Vec::new();
                    loop {
                        let ret = avcodec_receive_packet(state.codec_ctx, state.packet);
                        if ret == -(EAGAIN as c_int) || ret == AVERROR_EOF as c_int {
                            break;
                        }
                        if ret < 0 {
                            return Err(format!("avcodec_receive_packet failed: {ret}"));
                        }
                        let slice = std::slice::from_raw_parts(
                            (*state.packet).data,
                            (*state.packet).size as usize,
                        );
                        packets.extend_from_slice(slice);
                        av_packet_unref(state.packet);
                    }

                    Ok(packets)
                }
                CaptureFrame::DmaBuf { .. } => {
                    Err("VAAPI DMA-BUF path not yet implemented".into())
                }
            }
        }
    }
}

impl Drop for VaapiEncoder {
    fn drop(&mut self) {
        if let Some(mut state) = self.state.take() {
            unsafe {
                if !state.packet.is_null() {
                    av_packet_free(&mut state.packet as *mut *mut AVPacket);
                }
                if !state.cpu_frame.is_null() {
                    av_frame_free(&mut state.cpu_frame as *mut *mut AVFrame);
                }
                if !state.sws_ctx.is_null() {
                    sws_freeContext(state.sws_ctx);
                }
                if !state.codec_ctx.is_null() {
                    avcodec_free_context(&mut state.codec_ctx as *mut *mut AVCodecContext);
                }
                if !state.hw_frames_ctx.is_null() {
                    av_buffer_unref(&mut state.hw_frames_ctx as *mut *mut AVBufferRef);
                }
                if !state.hw_device_ctx.is_null() {
                    av_buffer_unref(&mut state.hw_device_ctx as *mut *mut AVBufferRef);
                }
            }
        }
    }
}

unsafe fn create_vaapi_device() -> Result<*mut AVBufferRef, String> {
    unsafe {
        let mut hw_device_ctx = ptr::null_mut();
        let ret = av_hwdevice_ctx_create(
            &mut hw_device_ctx as *mut *mut AVBufferRef,
            AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            ptr::null(),
            ptr::null_mut(),
            0,
        );
        if ret < 0 {
            return Err(format!("av_hwdevice_ctx_create failed: {ret}"));
        }
        Ok(hw_device_ctx)
    }
}

unsafe fn create_vaapi_frames_ctx(
    hw_device_ctx: *mut AVBufferRef,
    width: u32,
    height: u32,
    sw_format: AVPixelFormat,
) -> Result<*mut AVBufferRef, String> {
    unsafe {
        let hw_frames_ctx = av_hwframe_ctx_alloc(hw_device_ctx);
        if hw_frames_ctx.is_null() {
            return Err("av_hwframe_ctx_alloc failed".into());
        }

        let frames_ctx = &mut *((*hw_frames_ctx).data as *mut AVHWFramesContext);
        (*frames_ctx).format = AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*frames_ctx).sw_format = sw_format;
        (*frames_ctx).width = width as c_int;
        (*frames_ctx).height = height as c_int;
        (*frames_ctx).initial_pool_size = 5;

        let ret = av_hwframe_ctx_init(hw_frames_ctx);
        if ret < 0 {
            return Err(format!("av_hwframe_ctx_init failed: {ret}"));
        }

        Ok(hw_frames_ctx)
    }
}
