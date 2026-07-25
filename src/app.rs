use eframe::egui::{self, Frame, OutputCommand, Panel, RichText, Vec2};

use crate::{
    chat::{ChatEngine, ChatMessage, LocalChatEngine},
    codec,
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

        if ui.add(create_btn).clicked() {
            self.create_room();
        }

        ui.add_space(12.0);

        if ui.add(join_btn).clicked() {
            self.state = RoomState::start_joining(String::new());
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
                }

                let playing = true;
                if ui
                    .selectable_label(true, if playing { "⏸" } else { "▶" })
                    .clicked()
                {
                }

                if ui.selectable_label(true, "⏩").clicked() {
                }

                ui.add_space(8.0);

                let _ = ui.add(
                    egui::Slider::new(&mut 0.0_f32, 0.0..=100.0)
                        .text("")
                        .show_value(false),
                );

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
        let code = self.state.room_code().unwrap_or("unknown").to_string();
        let label = format!("Video stream for room: {code}");
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.4);
            ui.label("🎥");
            ui.add_space(8.0);
            ui.label(label);
            ui.label("(Video playback coming in future milestone)");
        });
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
