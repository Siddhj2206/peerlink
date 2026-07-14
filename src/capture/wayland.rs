use crate::capture::{Capture, CaptureError, CaptureFrame};

pub struct WaylandCapture {
    #[cfg(feature = "wayland")]
    inner: WaylandCaptureInner,
}

// ---------------------------------------------------------------------------
// stub (no wayland feature)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "wayland"))]
impl WaylandCapture {
    pub fn new() -> Option<Self> {
        None
    }
}

#[cfg(not(feature = "wayland"))]
impl Capture for WaylandCapture {
    fn capture_frame(&mut self) -> Result<CaptureFrame, CaptureError> {
        Err(CaptureError::Other(
            "Wayland capture not yet implemented".into(),
        ))
    }

    fn resolution(&self) -> (u32, u32) {
        (0, 0)
    }
}

// ---------------------------------------------------------------------------
// full implementation (wayland feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "wayland")]
use std::sync::mpsc;

#[cfg(feature = "wayland")]
struct WaylandCaptureInner {
    width: u32,
    height: u32,
    rx: mpsc::Receiver<CaptureFrame>,
    _pw_thread: pw_stream::PwThread,
}

#[cfg(feature = "wayland")]
impl WaylandCapture {
    pub fn new() -> Option<Self> {
        let info = portal::open_session()?;
        let (tx, rx) = mpsc::channel();
        let pw_thread = pw_stream::PwThread::start(info, tx).ok()?;
        Some(WaylandCapture {
            inner: WaylandCaptureInner {
                width: pw_thread.width,
                height: pw_thread.height,
                rx,
                _pw_thread: pw_thread,
            },
        })
    }
}

#[cfg(feature = "wayland")]
impl Capture for WaylandCapture {
    fn capture_frame(&mut self) -> Result<CaptureFrame, CaptureError> {
        match self.inner.rx.try_recv() {
            Ok(frame) => Ok(frame),
            Err(mpsc::TryRecvError::Empty) => Err(CaptureError::NoFrame),
            Err(mpsc::TryRecvError::Disconnected) => Err(CaptureError::Disconnected),
        }
    }

    fn resolution(&self) -> (u32, u32) {
        (self.inner.width, self.inner.height)
    }
}

// --- portal (ashpd) -------------------------------------------------------

#[cfg(feature = "wayland")]
mod portal {
    use ashpd::desktop::screencast::{
        CursorMode, OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, SourceType,
        StartCastOptions, Streams,
    };
    use ashpd::desktop::CreateSessionOptions;
    use std::os::unix::io::OwnedFd;

    pub struct StreamInfo {
        pub fd: OwnedFd,
        pub node_id: u32,
        pub width: u32,
        pub height: u32,
    }

    pub fn open_session() -> Option<StreamInfo> {
        async_io::block_on(async {
            let proxy = Screencast::new().await.ok()?;

            let session = proxy
                .create_session(CreateSessionOptions::default())
                .await
                .ok()?;

            proxy
                .select_sources(
                    &session,
                    SelectSourcesOptions::default()
                        .set_cursor_mode(CursorMode::Metadata)
                        .set_sources(SourceType::Monitor | SourceType::Window)
                        .set_multiple(true),
                )
                .await
                .ok()?;

            let streams: Streams = proxy
                .start(&session, None, StartCastOptions::default())
                .await
                .ok()?
                .response()
                .ok()?;

            let fd: OwnedFd = proxy
                .open_pipe_wire_remote(&session, OpenPipeWireRemoteOptions::default())
                .await
                .ok()?;

            let s = streams.streams().first()?;
            let node_id = s.pipe_wire_node_id();
            let (w, h) = s.size().unwrap_or((1920, 1080));

            Some(StreamInfo {
                fd,
                node_id,
                width: w as u32,
                height: h as u32,
            })
        })
    }
}

// --- PipeWire stream ------------------------------------------------------

#[cfg(feature = "wayland")]
mod pw_stream {
    use crate::capture::{CaptureFrame, DmaBufPlane};
    use pipewire as pw;
    use std::os::unix::io::{FromRawFd, OwnedFd};
    use std::sync::mpsc;
    use std::thread;

    use super::portal::StreamInfo;

    /// PipeWire runs on a background thread; this handle lives on the
    /// main thread so we can quit the mainloop when dropped.
    pub struct PwThread {
        pub width: u32,
        pub height: u32,
        mainloop: MainLoopHandle,
        handle: Option<thread::JoinHandle<()>>,
    }

    // SAFETY: pw::main_loop::MainLoopRc uses Rc internally but the C object
    // behind it is thread-safe.  We only call quit() from the main thread
    // while the background thread is inside run() — pw_main_loop_quit is safe.
    struct MainLoopHandle(pw::main_loop::MainLoopRc);
    unsafe impl Send for MainLoopHandle {}

    // SAFETY: PwObjects are only accessed from the spawned PW thread.
    struct PwObjects {
        mainloop: pw::main_loop::MainLoopRc,
        _context: pw::context::ContextRc,
        _core: pw::core::CoreRc,
        _stream: pw::stream::StreamRc,
        _listener: pw::stream::StreamListener<mpsc::Sender<CaptureFrame>>,
    }
    unsafe impl Send for PwObjects {}

    impl Drop for PwThread {
        fn drop(&mut self) {
            self.mainloop.0.quit();
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    // ------------------------------------------------------------------
    // SPA pod builders
    // ------------------------------------------------------------------

    mod pod {
        use pipewire as pw;

        fn serialise(value: &pw::spa::pod::Value) -> Vec<u8> {
            pw::spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                value,
            )
            .unwrap()
            .0
            .into_inner()
        }

        use pw::spa::param::format::*;
        use pw::spa::param::ParamType;
        use pw::spa::pod::{
            ChoiceValue, Object, Property, PropertyFlags, Value,
        };
        use pw::spa::utils::{
            Choice, ChoiceEnum, ChoiceFlags, Id, Rectangle, SpaTypes,
        };

        fn make_format(
            formats: &[u32],
            default_size: Rectangle,
            min_size: Rectangle,
            max_size: Rectangle,
        ) -> Object {
            Object {
                type_: SpaTypes::ObjectParamFormat.as_raw(),
                id: ParamType::EnumFormat.as_raw(),
                properties: vec![
                    Property {
                        key: FormatProperties::MediaType.as_raw(),
                        flags: PropertyFlags::empty(),
                        value: Value::Id(Id(MediaType::Video.as_raw())),
                    },
                    Property {
                        key: FormatProperties::MediaSubtype.as_raw(),
                        flags: PropertyFlags::empty(),
                        value: Value::Id(Id(MediaSubtype::Raw.as_raw())),
                    },
                    Property {
                        key: FormatProperties::VideoFormat.as_raw(),
                        flags: PropertyFlags::empty(),
                        value: Value::Choice(ChoiceValue::Id(Choice::<Id>(
                            ChoiceFlags::empty(),
                            ChoiceEnum::<Id>::Enum {
                                default: Id(formats[0]),
                                alternatives: formats
                                    .iter()
                                    .map(|&f| Id(f))
                                    .collect(),
                            },
                        ))),
                    },
                    Property {
                        key: FormatProperties::VideoSize.as_raw(),
                        flags: PropertyFlags::empty(),
                        value: Value::Choice(ChoiceValue::Rectangle(Choice::<Rectangle>(
                            ChoiceFlags::empty(),
                            ChoiceEnum::<Rectangle>::Range {
                                default: default_size,
                                min: min_size,
                                max: max_size,
                            },
                        ))),
                    },
                ],
            }
        }

        /// NV12 DMA-BUF format + SHM fallback.
        pub fn format_pods() -> (Vec<u8>, Vec<u8>) {
            use pw::spa::param::video::VideoFormat;

            let default_size = Rectangle { width: 320, height: 240 };
            let min_size = Rectangle { width: 1, height: 1 };
            let max_size = Rectangle { width: 8192, height: 8192 };

            // Pod 1: NV12 only (DMA-BUF capable)
            let nv12 = serialise(&Value::Object(make_format(
                &[VideoFormat::NV12.as_raw()],
                default_size,
                min_size,
                max_size,
            )));

            // Pod 2: SHM fallback (NV12, BGRA, RGBA — no modifier)
            let shm = serialise(&Value::Object(make_format(
                &[
                    VideoFormat::NV12.as_raw(),
                    VideoFormat::BGRA.as_raw(),
                    VideoFormat::RGBA.as_raw(),
                ],
                default_size,
                min_size,
                max_size,
            )));

            (nv12, shm)
        }

        /// Accept DMA-BUF, MemFd, or MemPtr.
        pub fn buffer_param() -> Vec<u8> {
            use pw::spa::buffer::DataType;

            let dt = (1 << DataType::DmaBuf.as_raw())
                | (1 << DataType::MemFd.as_raw())
                | (1 << DataType::MemPtr.as_raw());

            serialise(&Value::Object(Object {
                type_: SpaTypes::ObjectParamBuffers.as_raw(),
                id: ParamType::Buffers.as_raw(),
                properties: vec![Property {
                    key: pw::spa::sys::SPA_PARAM_BUFFERS_dataType,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(dt),
                }],
            }))
        }

        /// Meta params: Header + VideoTransform.
        pub fn meta_params() -> Vec<Vec<u8>> {
            use std::mem::size_of;

            fn meta(type_id: u32, size: i32) -> Vec<u8> {
                serialise(&Value::Object(Object {
                    type_: SpaTypes::ObjectParamMeta.as_raw(),
                    id: ParamType::Meta.as_raw(),
                    properties: vec![
                        Property {
                            key: pw::spa::sys::SPA_PARAM_META_type,
                            flags: PropertyFlags::empty(),
                            value: Value::Id(Id(type_id)),
                        },
                        Property {
                            key: pw::spa::sys::SPA_PARAM_META_size,
                            flags: PropertyFlags::empty(),
                            value: Value::Int(size),
                        },
                    ],
                }))
            }

            let hdr_size = size_of::<pw::spa::sys::spa_meta_header>() as i32;
            vec![
                meta(pw::spa::sys::SPA_META_Header, hdr_size),
                meta(pw::spa::sys::SPA_META_VideoTransform, 0),
            ]
        }
    }

    // ------------------------------------------------------------------
    // stream setup
    // ------------------------------------------------------------------

    static PW_INIT: std::sync::Once = std::sync::Once::new();

    impl PwThread {
        pub fn start(info: StreamInfo, tx: mpsc::Sender<CaptureFrame>) -> Result<Self, String> {
            PW_INIT.call_once(|| pw::init());

            let width = info.width;
            let height = info.height;
            let node_id = info.node_id;

            let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;

            let context =
                pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;

            let core = context
                .connect_fd_rc(info.fd, None)
                .map_err(|e| e.to_string())?;

            let mut stream_props = pw::properties::PropertiesBox::new();
            stream_props.insert(*pw::keys::MEDIA_TYPE, "Video");
            stream_props.insert(*pw::keys::MEDIA_CATEGORY, "Capture");
            stream_props.insert(*pw::keys::MEDIA_ROLE, "Screen");

            let stream = pw::stream::StreamRc::new(core.clone(), "peerlink-capture", stream_props)
                .map_err(|e| e.to_string())?;

            let listener = stream
                .add_local_listener_with_user_data(tx)
                .process(move |s, sender: &mut mpsc::Sender<CaptureFrame>| {
                    while let Some(mut buf) = s.dequeue_buffer() {
                        let datas = buf.datas_mut();
                        if datas.is_empty() {
                            continue;
                        }

                        let mut planes = Vec::new();
                        for d in datas.iter() {
                            let raw = d.fd();
                            if raw < 0 {
                                continue;
                            }
                            let duped = unsafe { libc::dup(raw) };
                            if duped < 0 {
                                continue;
                            }
                            let owned = unsafe { OwnedFd::from_raw_fd(duped) };
                            let chunk = d.chunk();
                            planes.push(DmaBufPlane {
                                fd: owned,
                                offset: chunk.offset(),
                                stride: chunk.stride() as u32,
                                size: chunk.size() as u32,
                            });
                        }

                        if !planes.is_empty() {
                            let frame = CaptureFrame::DmaBuf {
                                width,
                                height,
                                fourcc: 0x3231564e,
                                modifier: 0,
                                planes,
                            };
                            let _ = sender.send(frame);
                        }
                    }
                })
                .register()
                .map_err(|e| e.to_string())?;

            // build and connect negotiation params
            let (nv12_bytes, shm_bytes) = pod::format_pods();
            let buf_bytes = pod::buffer_param();
            let meta_bytes = pod::meta_params();

            let nv12_pod =
                pw::spa::pod::Pod::from_bytes(&nv12_bytes).ok_or("invalid NV12 pod")?;
            let shm_pod =
                pw::spa::pod::Pod::from_bytes(&shm_bytes).ok_or("invalid SHM pod")?;
            let buf_pod =
                pw::spa::pod::Pod::from_bytes(&buf_bytes).ok_or("invalid buffer pod")?;

            let mut pods: Vec<&pw::spa::pod::Pod> = vec![nv12_pod, shm_pod, buf_pod];
            for m in &meta_bytes {
                let meta_pod = pw::spa::pod::Pod::from_bytes(m).ok_or("invalid meta pod")?;
                pods.push(meta_pod);
            }
            let mut params = pods.as_mut_slice();

            stream
                .connect(
                    pw::spa::utils::Direction::Input,
                    Some(node_id),
                    pw::stream::StreamFlags::AUTOCONNECT
                        | pw::stream::StreamFlags::MAP_BUFFERS,
                    &mut params,
                )
                .map_err(|e| e.to_string())?;

            let objects = Box::new(PwObjects {
                mainloop: mainloop.clone(),
                _context: context,
                _core: core,
                _stream: stream,
                _listener: listener,
            });

            let handle = thread::spawn(move || {
                objects.mainloop.run();
            });

            Ok(PwThread {
                width,
                height,
                mainloop: MainLoopHandle(mainloop),
                handle: Some(handle),
            })
        }
    }
}
