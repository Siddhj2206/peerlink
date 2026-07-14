use std::ffi::CString;
use std::os::raw::c_int;
use std::os::unix::io::AsRawFd;
use std::ptr;

use libc;

use ffmpeg_next::ffi::*;

use crate::capture::{CaptureFrame, DmaBufPlane};
use crate::encode::Encoder;

mod va {
    #![allow(non_camel_case_types, non_snake_case, dead_code)]

    use std::ffi::c_void;

    pub type VADisplay = *mut c_void;
    pub type VASurfaceID = u32;
    pub type VAStatus = i32;

    pub const VA_STATUS_SUCCESS: VAStatus = 0;
    pub const VA_RT_FORMAT_YUV420: u32 = 0x00000001;
    pub const VA_SURFACE_ATTRIB_SETTABLE: u32 = 0x00000002;
    pub const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME: u32 = 0x20000000;
    pub const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2: u32 = 0x40000000;

    pub const VA_SURFACE_ATTRIB_MEMORY_TYPE: u32 = 6;
    pub const VA_SURFACE_ATTRIB_EXTERNAL_BUFFER_DESCRIPTOR: u32 = 7;

    pub const VA_GENERIC_VALUE_TYPE_INTEGER: u32 = 1;
    pub const VA_GENERIC_VALUE_TYPE_POINTER: u32 = 3;

    pub const DRM_FORMAT_R8: u32 = 0x20203852;
    pub const DRM_FORMAT_GR88: u32 = 0x38385247;
    pub const VA_FOURCC_NV12: u32 = 0x3231564e;

    #[repr(C)]
    pub union VaValue {
        pub i: i32,
        pub p: *mut c_void,
    }

    #[repr(C)]
    pub struct VaGenericValue {
        pub type_: u32,
        pub value: VaValue,
    }

    #[repr(C)]
    pub struct VaSurfaceAttrib {
        pub type_: u32,
        pub flags: u32,
        pub value: VaGenericValue,
    }

    #[repr(C)]
    pub struct VADRMPRIMESurfaceObject {
        pub fd: i32,
        pub size: u32,
        pub drm_format_modifier: u64,
    }

    #[repr(C)]
    pub struct VADRMPRIMESurfaceLayer {
        pub drm_format: u32,
        pub num_planes: u32,
        pub object_index: [u32; 4],
        pub offset: [u32; 4],
        pub pitch: [u32; 4],
    }

    #[repr(C)]
    pub struct VADRMPRIMESurfaceDescriptor {
        pub fourcc: u32,
        pub width: u32,
        pub height: u32,
        pub num_objects: u32,
        pub objects: [VADRMPRIMESurfaceObject; 4],
        pub num_layers: u32,
        pub layers: [VADRMPRIMESurfaceLayer; 4],
    }

    #[link(name = "va")]
    unsafe extern "C" {
        pub fn vaCreateSurfaces(
            dpy: VADisplay,
            format: u32,
            width: u32,
            height: u32,
            surfaces: *mut VASurfaceID,
            num_surfaces: u32,
            attrib_list: *mut VaSurfaceAttrib,
            num_attribs: u32,
        ) -> VAStatus;

        pub fn vaDestroySurfaces(
            dpy: VADisplay,
            surfaces: *mut VASurfaceID,
            num_surfaces: u32,
        ) -> VAStatus;

        pub fn vaSyncSurface(
            dpy: VADisplay,
            surface: VASurfaceID,
        ) -> VAStatus;
    }
}

#[repr(C)]
struct VaapiDeviceContext {
    display: va::VADisplay,
}

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
                    self.encode_rgba(data.as_slice(), *width, *height)
                }
                CaptureFrame::DmaBuf {
                    width,
                    height,
                    fourcc,
                    modifier,
                    planes,
                } => self.encode_dma_buf(*width, *height, *fourcc, *modifier, planes),
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

unsafe fn create_vaapi_frames_ctx(
    hw_device_ctx: *mut AVBufferRef,
    width: u32,
    height: u32,
    sw_format: AVPixelFormat,
) -> Result<*mut AVBufferRef, String> {
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

unsafe fn get_va_display(hw_device_ref: *mut AVBufferRef) -> Result<va::VADisplay, String> {
    let hwdev = &*((*hw_device_ref).data as *mut AVHWDeviceContext);
    let va_ctx = &*(hwdev.hwctx as *const VaapiDeviceContext);
    Ok(va_ctx.display)
}

unsafe fn drain_packets(
    codec_ctx: *mut AVCodecContext,
    packet: *mut AVPacket,
) -> Result<Vec<u8>, String> {
    let mut packets = Vec::new();
    loop {
        let ret = avcodec_receive_packet(codec_ctx, packet);
        if ret == -(EAGAIN as c_int) || ret == AVERROR_EOF as c_int {
            break;
        }
        if ret < 0 {
            return Err(format!("avcodec_receive_packet failed: {ret}"));
        }
        let slice = std::slice::from_raw_parts((*packet).data, (*packet).size as usize);
        packets.extend_from_slice(slice);
        av_packet_unref(packet);
    }
    Ok(packets)
}

impl VaapiEncoder {
    fn ensure_init(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.state.is_none()
            || self
                .state
                .as_ref()
                .map_or(true, |s| s.width != width || s.height != height)
        {
            self.state = None;
            self.init(width, height)
        } else {
            Ok(())
        }
    }

    unsafe fn encode_rgba(&mut self, data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
        let w = width;
        let h = height;

        self.ensure_init(w, h)?;

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

        drain_packets(state.codec_ctx, state.packet)
    }

    unsafe fn encode_dma_buf(
        &mut self,
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
        planes: &[DmaBufPlane],
    ) -> Result<Vec<u8>, String> {
        if width == 0 || height == 0 || planes.is_empty() {
            return Err("invalid DMA-BUF frame: zero dimensions or no planes".into());
        }

        self.ensure_init(width, height)?;

        let state = self.state.as_mut().unwrap();

        let display = get_va_display(state.hw_device_ctx)?;

        let surface_id =
            import_dmabuf_surface(display, width, height, fourcc, modifier, planes)?;

        let mut frame = av_frame_alloc();
        if frame.is_null() {
            va::vaDestroySurfaces(display, &surface_id as *const _ as *mut va::VASurfaceID, 1);
            return Err("av_frame_alloc failed".into());
        }
        (*frame).format = AVPixelFormat::AV_PIX_FMT_VAAPI as c_int;
        (*frame).width = width as c_int;
        (*frame).height = height as c_int;
        (*frame).hw_frames_ctx = av_buffer_ref(state.hw_frames_ctx);
        (*frame).data[3] = surface_id as usize as *mut u8;
        (*frame).pts = AV_NOPTS_VALUE;

        let ret = avcodec_send_frame(state.codec_ctx, frame);
        av_frame_free(&mut frame as *mut *mut AVFrame);
        if ret < 0 && ret != -(EAGAIN as c_int) {
            va::vaDestroySurfaces(display, &surface_id as *const _ as *mut va::VASurfaceID, 1);
            return Err(format!("avcodec_send_frame failed: {ret}"));
        }

        let result = drain_packets(state.codec_ctx, state.packet);

        va::vaSyncSurface(display, surface_id);
        va::vaDestroySurfaces(display, &surface_id as *const _ as *mut va::VASurfaceID, 1);

        result
    }
}

unsafe fn import_dmabuf_surface(
    display: va::VADisplay,
    width: u32,
    height: u32,
    fourcc: u32,
    modifier: u64,
    planes: &[DmaBufPlane],
) -> Result<va::VASurfaceID, String> {
    let raw_fd = planes[0].fd.as_raw_fd();
    let dup_fd = libc::dup(raw_fd);
    if dup_fd < 0 {
        return Err(format!("dup failed for DMA-BUF fd: {dup_fd}"));
    }

    let obj_size = planes[0].size;

    let (second_stride, second_offset) = if planes.len() > 1 {
        (planes[1].stride, planes[1].offset)
    } else {
        (planes[0].stride, planes[0].offset + width * height)
    };

    let mut desc = va::VADRMPRIMESurfaceDescriptor {
        fourcc,
        width,
        height,
        num_objects: 1,
        objects: [
            va::VADRMPRIMESurfaceObject {
                fd: dup_fd,
                size: obj_size,
                drm_format_modifier: modifier,
            },
            std::mem::zeroed(),
            std::mem::zeroed(),
            std::mem::zeroed(),
        ],
        num_layers: 2,
        layers: [
            va::VADRMPRIMESurfaceLayer {
                drm_format: va::DRM_FORMAT_R8,
                num_planes: 1,
                object_index: [0, 0, 0, 0],
                offset: [planes[0].offset, 0, 0, 0],
                pitch: [planes[0].stride, 0, 0, 0],
            },
            va::VADRMPRIMESurfaceLayer {
                drm_format: va::DRM_FORMAT_GR88,
                num_planes: 1,
                object_index: [0, 0, 0, 0],
                offset: [second_offset, 0, 0, 0],
                pitch: [second_stride, 0, 0, 0],
            },
            std::mem::zeroed(),
            std::mem::zeroed(),
        ],
    };

    let mem_type_attr = va::VaSurfaceAttrib {
        type_: va::VA_SURFACE_ATTRIB_MEMORY_TYPE,
        flags: va::VA_SURFACE_ATTRIB_SETTABLE,
        value: va::VaGenericValue {
            type_: va::VA_GENERIC_VALUE_TYPE_INTEGER,
            value: va::VaValue {
                i: va::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2 as i32,
            },
        },
    };

    let ext_desc_attr = va::VaSurfaceAttrib {
        type_: va::VA_SURFACE_ATTRIB_EXTERNAL_BUFFER_DESCRIPTOR,
        flags: va::VA_SURFACE_ATTRIB_SETTABLE,
        value: va::VaGenericValue {
            type_: va::VA_GENERIC_VALUE_TYPE_POINTER,
            value: va::VaValue {
                p: &mut desc as *mut _ as *mut std::ffi::c_void,
            },
        },
    };

    let mut attrs = [mem_type_attr, ext_desc_attr];
    let mut surface_id: va::VASurfaceID = 0;

    let status = va::vaCreateSurfaces(
        display,
        va::VA_RT_FORMAT_YUV420,
        width,
        height,
        &mut surface_id,
        1,
        attrs.as_mut_ptr(),
        attrs.len() as u32,
    );

    if status != va::VA_STATUS_SUCCESS {
        return Err(format!(
            "vaCreateSurfaces(DRM_PRIME_2) failed with status {status}"
        ));
    }

    Ok(surface_id)
}
