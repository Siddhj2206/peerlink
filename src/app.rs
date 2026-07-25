use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Frame, OutputCommand, Panel, RichText, Vec2};

use crate::{
    chat::{ChatEngine, ChatMessage, LocalChatEngine},
    codec,
    demux::{self, VideoFrame, VideoInfo},
    room::RoomState,
    ticket::PartyTicket,
};

const APP_NAME: &str = "PeerLink";
const MIN_CHAT_WIDTH: f32 = 150.0;
const DEFAULT_CHAT_WIDTH: f32 = 280.0;

pub struct PeerLinkApp {
    state: RoomState,
    chat_visible: bool,
    ticket_input: String,
    room_code: String,
    chat_input: String,
    chat_engine: LocalChatEngine,
    chat_messages: Vec<ChatMessage>,

    // video playback
    video_path: Option<PathBuf>,
    video_info: Option<VideoInfo>,
    video_frames: Vec<VideoFrame>,
    current_frame: usize,
    is_playing: bool,
    playback_started: Option<Instant>,
    elapsed: f64,
    total_frames: usize,
}

impl Default for PeerLinkApp {
    fn default() -> Self {
        Self {
            state: RoomState::Idle,
            chat_visible: true,
            ticket_input: String::new(),
            room_code: String::new(),
            chat_input: String::new(),
            chat_engine: LocalChatEngine::new("You".into()),
            chat_messages: Vec::new(),

            video_path: None,
            video_info: None,
            video_frames: Vec::new(),
            current_frame: 0,
            is_playing: false,
            playback_started: None,
            elapsed: 0.0,
            total_frames: 0,
        }
    }
}

impl eframe::App for PeerLinkApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &self.state {
            RoomState::Idle | RoomState::Error { .. } => self.render_home(ui),
            RoomState::Joining { .. } => self.render_home(ui),
            RoomState::Hosting { .. } | RoomState::Joined { .. } => self.render_player(ui),
        }
    }
}

impl PeerLinkApp {
    fn render_home(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            self.render_error_banner(ui);

            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.2);
                ui.heading(RichText::new(APP_NAME).size(36.0));
                ui.add_space(8.0);
                ui.label("Watch parties, peer-to-peer.");
                ui.add_space(32.0);

                match &self.state {
                    RoomState::Idle => self.render_idle_actions(ui),
                    RoomState::Joining { .. } => self.render_joining_input(ui),
                    _ => {}
                }
            });
        });
    }

    fn render_error_banner(&mut self, ui: &mut egui::Ui) {
        let message = match &self.state {
            RoomState::Error { message } => Some(message.clone()),
            _ => None,
        };
        if let Some(msg) = message {
            Frame::group(ui.style())
                .fill(ui.visuals().error_fg_color.linear_multiply(0.15))
                .stroke(egui::Stroke::new(1.0, ui.visuals().error_fg_color))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&msg).color(ui.visuals().error_fg_color));
                        if ui.button("Dismiss").clicked() {
                            let old_state = std::mem::replace(&mut self.state, RoomState::Idle);
                            self.state = old_state.dismiss_error();
                        }
                    });
                });
        }
    }

    fn render_idle_actions(&mut self, ui: &mut egui::Ui) {
        let create_btn = egui::Button::new(RichText::new("Create Room").size(18.0))
            .min_size(Vec2::new(200.0, 48.0));
        let join_btn = egui::Button::new(RichText::new("Join Room").size(18.0))
            .min_size(Vec2::new(200.0, 48.0));
        let open_btn = egui::Button::new(RichText::new("Open MP4…").size(18.0))
            .min_size(Vec2::new(200.0, 48.0));

        if ui.add(create_btn).clicked() {
            self.create_room();
        }

        ui.add_space(12.0);

        if ui.add(join_btn).clicked() {
            self.state = RoomState::start_joining(String::new());
        }

        ui.add_space(12.0);

        if ui.add(open_btn).clicked() {
            self.open_file();
        }
    }

    fn render_joining_input(&mut self, ui: &mut egui::Ui) {
        ui.label("Paste a party ticket to join a room:");
        ui.add_space(8.0);

        let input = egui::TextEdit::singleline(&mut self.ticket_input)
            .hint_text("party<base32encoded>")
            .desired_width(360.0);
        ui.add(input);

        ui.add_space(12.0);

        ui.horizontal(|ui| {
            let can_join = !self.ticket_input.is_empty();
            let join_btn = egui::Button::new(RichText::new("Join").size(16.0))
                .min_size(Vec2::new(120.0, 40.0));
            if ui.add_enabled(can_join, join_btn).clicked() {
                self.join_room();
            }

            ui.add_space(8.0);

            if ui
                .add(egui::Button::new("Cancel").min_size(Vec2::new(120.0, 40.0)))
                .clicked()
            {
                self.state = RoomState::Idle;
                self.ticket_input.clear();
            }
        });
    }

    fn render_player(&mut self, ui: &mut egui::Ui) {
        self.poll_chat();
        self.update_playback();

        Panel::top("player_top").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("← Leave").clicked() {
                    let old_state = std::mem::replace(&mut self.state, RoomState::Idle);
                    self.state = old_state.leave();
                    self.room_code.clear();
                }

                ui.add_space(12.0);

                if let Some(code) = self.state.room_code() {
                    ui.heading(format!("Room: {code}"));

                    if let RoomState::Hosting { ticket_string, .. } = &self.state {
                        ui.add_space(12.0);
                        if ui.button("Copy Ticket").clicked() {
                            ui.ctx().send_cmd(OutputCommand::CopyText(ticket_string.clone()));
                        }
                    }
                } else if self.video_path.is_some() {
                    ui.heading("Local Playback");
                }
            });
        });

        if self.chat_visible {
            Panel::right("chat_panel")
                .resizable(true)
                .default_size(DEFAULT_CHAT_WIDTH)
                .min_size(MIN_CHAT_WIDTH)
                .show(ui, |ui| {
                    self.render_chat_panel(ui);
                });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.render_video_area(ui);
        });

        Panel::bottom("player_controls").show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if ui.selectable_label(true, "⏪").clicked() {
                    let third = self.total_frames / 3;
                    let target = self.current_frame.saturating_sub(third);
                    let ts = self
                        .video_frames
                        .get(target)
                        .map(|f| f.timestamp)
                        .unwrap_or_default();
                    self.seek_to(ts);
                }

                if ui
                    .selectable_label(true, if self.is_playing { "⏸" } else { "▶" })
                    .clicked()
                {
                    self.is_playing = !self.is_playing;
                    if self.is_playing {
                        if self.current_frame >= self.total_frames.saturating_sub(1) {
                            self.current_frame = 0;
                        }
                        self.playback_started = Some(Instant::now());
                        self.elapsed = self
                            .video_frames
                            .get(self.current_frame)
                            .map(|f| f.timestamp.as_secs_f64())
                            .unwrap_or(0.0);
                    }
                }

                if ui.selectable_label(true, "⏩").clicked() {
                    let third = self.total_frames / 3;
                    let target = (self.current_frame + third).min(self.total_frames.saturating_sub(1));
                    let ts = self
                        .video_frames
                        .get(target)
                        .map(|f| f.timestamp)
                        .unwrap_or_default();
                    self.seek_to(ts);
                }

                ui.add_space(8.0);

                let total_dur = self
                    .video_frames
                    .last()
                    .map(|f| f.timestamp.as_secs_f64())
                    .unwrap_or(0.0);
                let progress = if total_dur > 0.0 {
                    self.elapsed / total_dur
                } else {
                    0.0
                };
                let mut progress_f32 = progress as f32;
                let slider = ui.add(
                    egui::Slider::new(&mut progress_f32, 0.0..=1.0)
                        .text("")
                        .show_value(false),
                );
                if slider.changed() {
                    let seek_time = progress_f32 as f64 * total_dur;
                    self.seek_to(Duration::from_secs_f64(seek_time));
                }

                ui.add_space(4.0);
                let pos = format_time(self.elapsed);
                let dur = format_time(total_dur);
                ui.label(format!("{pos} / {dur}"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let chat_toggle = egui::Button::new(if self.chat_visible { "Chat ▸" } else { "Chat ◂" });
                    if ui.add(chat_toggle).clicked() {
                        self.chat_visible = !self.chat_visible;
                    }
                });
            });
        });
    }

    fn reset_chat(&mut self) {
        self.chat_engine = LocalChatEngine::new("You".into());
        self.chat_messages.clear();
    }

    fn poll_chat(&mut self) {
        let msgs = self.chat_engine.drain_messages();
        self.chat_messages.extend(msgs);
    }

    fn render_video_area(&mut self, ui: &mut egui::Ui) {
        if let Some(info) = &self.video_info {
            let frame = self.video_frames.get(self.current_frame);
            let file = self
                .video_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let dur = format_time(info.duration.as_secs_f64());
            let kf_count = self.video_frames.iter().filter(|f| f.is_keyframe).count();
            let frame_info = frame
                .map(|f| {
                    let ts = format_time(f.timestamp.as_secs_f64());
                    let kf = if f.is_keyframe { " [K]" } else { "" };
                    format!("Frame {}/{}, {ts}{kf}", self.current_frame + 1, info.frame_count)
                })
                .unwrap_or_default();

            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                ui.label("🎥");
                ui.add_space(4.0);
                ui.label(format!("{file} — {dur}"));
                ui.add_space(4.0);
                ui.label(format!("{} frames ({} keyframes, {} tracks)", info.frame_count, kf_count, info.track_count));
                if info.width > 0 {
                    ui.label(format!("{}×{}", info.width, info.height));
                }
                ui.add_space(4.0);
                ui.label(frame_info);
                if self.is_playing {
                    ui.add_space(4.0);
                    ui.label("▶ Playing");
                }
            });
        } else if let Some(code) = self.state.room_code() {
            let label = format!("Video stream for room: {code}");
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.4);
                ui.label("🎥");
                ui.add_space(8.0);
                ui.label(label);
                ui.label("(Open an MP4 file to start playback)");
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.4);
                ui.label("🎥");
                ui.add_space(8.0);
                ui.label("Open an MP4 file to start playback");
            });
        }
    }

    fn render_chat_panel(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label(RichText::new("Chat").size(16.0).strong());
            ui.separator();

            let max_height = ui.available_height() - 60.0;
            let scroll = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(max_height);
            let _ = scroll.show(ui, |ui| {
                if self.chat_messages.is_empty() {
                    ui.label("No messages yet.");
                } else {
                    for msg in &self.chat_messages {
                        let line = format!("{}: {}", msg.author, msg.content);
                        ui.label(line);
                    }
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                let input = egui::TextEdit::singleline(&mut self.chat_input)
                    .hint_text("Type a message...")
                    .desired_width(ui.available_width() - 60.0);
                let input_resp = ui.add(input);

                let send_clicked = ui.button("Send").clicked();
                let enter_pressed = input_resp.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if send_clicked || enter_pressed {
                    let content = self.chat_input.trim().to_string();
                    if !content.is_empty() {
                        self.chat_engine.send(content);
                        self.chat_input.clear();
                    }
                }
            });
        });
    }

    fn create_room(&mut self) {
        let room_code = codec::generate_room_code();
        let topic_id = codec::words_to_topic_id(&room_code);
        let namespace_id = iroh_docs::NamespaceId::from(&rand::random::<[u8; 32]>());
        let ticket = PartyTicket::new(topic_id, namespace_id);
        let ticket_string = ticket.to_string_encoded();

        self.room_code = room_code.clone();
        self.reset_chat();
        self.state = RoomState::start_hosting(room_code, ticket_string, topic_id, namespace_id);
    }

    fn open_file(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("MP4 Video", &["mp4", "MP4"])
            .pick_file();
        if let Some(path) = path {
            match demux::demux_file(&path) {
                Ok((info, frames)) => {
                    self.video_path = Some(path);
                    self.video_info = Some(info);
                    self.video_frames = frames;
                    self.current_frame = 0;
                    self.is_playing = false;
                    self.playback_started = None;
                    self.elapsed = 0.0;
                    self.total_frames = self.video_frames.len();
                }
                Err(e) => {
                    tracing::error!("Failed to demux file: {e}");
                    self.state = RoomState::fail(format!("Failed to demux file: {e}"));
                }
            }
        }
    }

    fn seek_to(&mut self, seek_to: Duration) {
        self.current_frame = demux::seek_to_keyframe(&self.video_frames, seek_to);
        if self.is_playing {
            self.playback_started = Some(Instant::now());
            self.elapsed = seek_to.as_secs_f64();
        }
    }

    fn update_playback(&mut self) {
        if !self.is_playing || self.video_frames.is_empty() {
            return;
        }

        let now = Instant::now();
        if let Some(start) = self.playback_started {
            self.elapsed = now.duration_since(start).as_secs_f64();
        } else {
            self.playback_started = Some(now);
            self.elapsed = 0.0;
        }

        let total_duration = self
            .video_frames
            .last()
            .map(|f| f.timestamp.as_secs_f64())
            .unwrap_or(0.0);

        if self.elapsed >= total_duration {
            self.is_playing = false;
            self.current_frame = self.video_frames.len().saturating_sub(1);
            return;
        }

        let seek = Duration::from_secs_f64(self.elapsed);
        self.current_frame = demux::seek_to_keyframe(&self.video_frames, seek);

        if self.current_frame < self.video_frames.len() {
            while self.current_frame + 1 < self.video_frames.len()
                && self.video_frames[self.current_frame + 1].timestamp <= seek
            {
                self.current_frame += 1;
            }
        }
    }

    fn join_room(&mut self) {
        let ticket_str = self.ticket_input.trim().to_string();
        if ticket_str.is_empty() {
            return;
        }

        match PartyTicket::parse(&ticket_str) {
            Ok(ticket) => {
                let topic_id = ticket.topic_id();
                let namespace_id = ticket.namespace_id();
                let state = std::mem::replace(&mut self.state, RoomState::Idle);
                self.reset_chat();
                self.state = state.join(ticket_str.clone(), topic_id, namespace_id);
                self.room_code = ticket_str.clone();
            }
            Err(e) => {
                self.state = RoomState::fail(format!("Invalid ticket: {e}"));
            }
        }
    }
}

fn format_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
