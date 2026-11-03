mod discord;

use eframe::egui;
use std::collections::HashMap;
use std::fs;
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use aes_gcm::aead::generic_array::GenericArray;
use base64::{Engine as _, engine::general_purpose};
use std::sync::{mpsc, Arc, Mutex};
use std::path::PathBuf;
fn tokens_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join("Rucord");
    let _ = fs::create_dir_all(&dir);
    dir.join("tokens.json")
}
fn load_tokens() -> Vec<(String,String)> {
    if let Ok(data) = fs::read_to_string(tokens_path()) {
        // decrypt
        if let Ok(bytes) = general_purpose::STANDARD.decode(data.trim()) {
            let key = GenericArray::from_slice(b"0123456789abcdef0123456789abcdef");
            let cipher = Aes256Gcm::new(key);
            let nonce = GenericArray::from_slice(b"unique nonce");
            if let Ok(pt) = cipher.decrypt(nonce, bytes.as_ref()) {
                if let Ok(s) = String::from_utf8(pt) {
                    if let Ok(v) = serde_json::from_str::<Vec<(String,String)>>(&s) { return v; }
                }
            }
        }
        // fallback plain
        if let Ok(v) = serde_json::from_str::<Vec<(String,String)>>(&data) { return v; }
    }
    vec![]
}
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Cache { guilds: Vec<discord::Guild>, channels: Vec<discord::Channel>, guild_channels: Vec<(String, Vec<discord::Channel>)> }
fn cache_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join("Rucord");
    let _ = fs::create_dir_all(&dir);
    dir.join("cache.json")
}
fn load_cache() -> Cache {
    if let Ok(data) = fs::read_to_string(cache_path()) {
        if let Ok(c) = serde_json::from_str::<Cache>(&data) { return c; }
    }
    Cache::default()
}
fn save_cache(guilds: &[discord::Guild], channels: &[discord::Channel], guild_channels: &[(String, Vec<discord::Channel>)]) {
    let c = Cache { guilds: guilds.to_vec(), channels: channels.to_vec(), guild_channels: guild_channels.to_vec() };
    if let Ok(data) = serde_json::to_string(&c) { let _ = fs::write(cache_path(), data); }
}
fn save_tokens(tokens: &[(String,String)]) {
    if let Ok(json) = serde_json::to_string(tokens) {
        let key = GenericArray::from_slice(b"0123456789abcdef0123456789abcdef");
        let cipher = Aes256Gcm::new(key);
        let nonce = GenericArray::from_slice(b"unique nonce");
        if let Ok(ct) = cipher.encrypt(nonce, json.as_bytes()) {
            let enc = general_purpose::STANDARD.encode(ct);
            let _ = fs::write(tokens_path(), enc);
        }
    }
}

#[derive(Default)]
struct AppState {
    client_id: String,
    user_token: String,
    token: Option<String>,
    status: String,
    logging_in: bool,
    guilds: Vec<discord::Guild>,
    channels: Vec<discord::Channel>,
    guild_channels: Vec<(String, Vec<discord::Channel>)>,
    selected_guild: Option<String>,
    selected_channel: Option<String>,
    messages: Vec<discord::Message>,
    guild_icons: HashMap<String, egui::TextureHandle>,
    viewing_url: Option<String>,
    viewing_texture: Option<(egui::TextureHandle, [f32; 2])>,
    input: String,
    tokens: Vec<(String, String)>,
}
struct App {
    state: Arc<Mutex<AppState>>,
    tx: mpsc::Sender<Result<String, String>>,
    rx: mpsc::Receiver<Result<String, String>>,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut st = AppState::default();
        st.tokens = load_tokens();
        let c = load_cache();
        st.guilds = c.guilds;
        st.channels = c.channels;
        st.guild_channels = c.guild_channels;
        Self { state: Arc::new(Mutex::new(st)), tx, rx }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(res) = self.rx.try_recv() {
            let mut s = self.state.lock().unwrap();
            s.logging_in = false;
            match res {
                Ok(t) => { s.token = Some(t); s.status = "Logged in".into(); }
                Err(e) => { s.status = format!("Login failed: {}", e); }
            }
        }

        // load viewed image
        {
            let mut s = self.state.lock().unwrap();
            if let Some(url) = s.viewing_url.clone() {
                if s.viewing_texture.is_none() {
                    if let Ok(resp) = reqwest::blocking::get(&url) {
                        if let Ok(bytes) = resp.bytes() {
                            if let Ok(img) = image::load_from_memory(&bytes) {
                                let rgba = img.to_rgba8();
                                let size = [rgba.width() as usize, rgba.height() as usize];
                                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                let tex = ctx.load_texture("view", color, egui::TextureOptions::default());
                                s.viewing_texture = Some((tex, [size[0] as f32, size[1] as f32]));
                            }
                        }
                    }
                }
            }
            if let Some((tex, img_size)) = s.viewing_texture.clone() {
                egui::Window::new("")
                    .title_bar(false)
                    .resizable(false)
                    .collapsible(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.centered_and_justified(|ui| {
                            let tex_id = tex.id();
                            let screen = ctx.available_rect();
                            let max_w = screen.width() * 0.9;
                            let max_h = screen.height() * 0.9;
                            let scale = (max_w / img_size[0]).min(max_h / img_size[1]).min(1.0);
                            ui.image((tex_id, egui::Vec2::new(img_size[0] * scale, img_size[1] * scale)));
                            ui.add_space(10.0);
                            if ui.button("Close").clicked() { s.viewing_url = None; s.viewing_texture = None; }
                        });
                    });
                let mut s2 = self.state.lock().unwrap();
                if ctx.input(|i| i.pointer.any_click()) && !ctx.is_pointer_over_area() {
                    s2.viewing_url = None;
                    s2.viewing_texture = None;
                }
                return;
            }
        }
        let mut s = self.state.lock().unwrap();
        egui::CentralPanel::default().show(ctx, |ui| {
            if s.token.is_none() {
                ui.heading("Rucord - Login");
                ui.label("Paste a user token:");
                ui.text_edit_singleline(&mut s.user_token);
                if ui.button("Add token").clicked() && !s.user_token.is_empty() {
                    let tok = s.user_token.clone();
                    let name = discord::me(&tok).map(|u| u.username).unwrap_or_else(|_| "unknown".into());
                    s.tokens.push((name, tok));
                    s.user_token.clear();
                    save_tokens(&s.tokens);
                }
                ui.label("Saved tokens:");
                let mut to_select: Option<String> = None;
                for (name, tok) in &s.tokens {
                    if ui.selectable_label(s.token.as_deref() == Some(tok), name).clicked() {
                        to_select = Some(tok.clone());
                    }
                }
                if let Some(t) = to_select { 
                    s.token = Some(t.clone()); 
                    s.status = "Selected token".into();
                    let state = self.state.clone();
                    std::thread::spawn(move || {
                        if let Ok(chs) = discord::dm_channels(&t) {
                            let mut st = state.lock().unwrap();
                            st.channels = chs;
                                    save_cache(&st.guilds, &st.channels, &st.guild_channels);
                            st.selected_channel = None;
                            st.messages.clear();
                        }
                    });
                }
            } else {
                // fetch guilds on first
                if s.guilds.is_empty() {
                    if let Some(t) = s.token.clone() {
                        let state = self.state.clone();
                        std::thread::spawn(move || {
                            if let Ok(gs) = discord::guilds(&t) {
                                let mut st = state.lock().unwrap();
                                st.guilds = gs.clone();
                                save_cache(&st.guilds, &st.channels, &st.guild_channels);
                            }
                        });
                    }
                }
                egui::SidePanel::left("servers").show(ctx, |ui| {
                    ui.heading("Servers");
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for g in s.guilds.clone() {
                            let selected = s.selected_guild.as_deref() == Some(&g.id);
                            if let Some(url) = discord::guild_icon_url(&g) {
                                if !s.guild_icons.contains_key(&g.id) {
                                    if let Ok(resp) = reqwest::blocking::get(&url) {
                                        if let Ok(bytes) = resp.bytes() {
                                            if let Ok(img) = image::load_from_memory(&bytes) {
                                                let rgba = img.to_rgba8();
                                                let size = [rgba.width() as usize, rgba.height() as usize];
                                                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                                let tex = ctx.load_texture(format!("guild-{}", g.id), color, egui::TextureOptions::default());
                                                s.guild_icons.insert(g.id.clone(), tex);
                                            }
                                        }
                                    }
                                }
                                let click = if let Some(tex) = s.guild_icons.get(&g.id) {
                                    ui.add(egui::Button::image((tex.id(), egui::vec2(32.0, 32.0))).frame(false)).on_hover_text(&g.name).clicked()
                                } else { false };
                                if click {
                                    s.selected_guild = Some(g.id.clone());
                                    s.channels.clear();
                                    if let Some(t) = s.token.clone() {
                                        let gid = g.id.clone();
                                        let state = self.state.clone();
                                        std::thread::spawn(move || {
                                            if let Ok(chs) = discord::channels(&t, &gid) {
                                                let mut st = state.lock().unwrap();
                                                st.channels = chs.clone();
                                                st.guild_channels.retain(|(id, _)| id != &gid);
                                                st.guild_channels.push((gid.clone(), chs));
                                                save_cache(&st.guilds, &st.channels, &st.guild_channels);
                                                st.selected_channel = None;
                                                st.messages.clear();
                                            }
                                        });
                                    }
                                }
                            } else {
                                if ui.selectable_label(selected, &g.name).clicked() {
                                    s.selected_guild = Some(g.id.clone());
                                    s.channels.clear();
                                    if let Some(t) = s.token.clone() {
                                        let gid = g.id.clone();
                                        let state = self.state.clone();
                                        std::thread::spawn(move || {
                                            if let Ok(chs) = discord::channels(&t, &gid) {
                                                let mut st = state.lock().unwrap();
                                                st.channels = chs.clone();
                                                st.guild_channels.retain(|(id, _)| id != &gid);
                                                st.guild_channels.push((gid.clone(), chs));
                                                save_cache(&st.guilds, &st.channels, &st.guild_channels);
                                                st.selected_channel = None;
                                                st.messages.clear();
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    });
                });
                egui::SidePanel::left("channels").show(ctx, |ui| {
                    ui.heading("Channels");
                    for c in s.channels.clone() {
let name = c.name.clone().unwrap_or_else(|| {
    if c.recipients.is_empty() { format!("ch-{}", c.id) } else { c.recipients.iter().map(|u| u.username.clone()).collect::<Vec<_>>().join(", ") }
});
                        if ui.selectable_label(s.selected_channel.as_deref() == Some(&c.id), &name).clicked() {
                            s.selected_channel = Some(c.id.clone());
                            s.messages.clear();
                            if let Some(t) = s.token.clone() {
                                let cid = c.id.clone();
                                let state = self.state.clone();
                                std::thread::spawn(move || {
                                    if let Ok(ms) = discord::messages(&t, &cid, 50) {
                                        state.lock().unwrap().messages = ms;
                                    }
                                });
                            }
                        }
                    }
                });
                egui::TopBottomPanel::bottom("input").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut s.input);
                        if ui.button("Send").clicked() {
                            if let (Some(t), Some(cid)) = (s.token.clone(), s.selected_channel.clone()) {
                                let content = s.input.clone();
                                let state = self.state.clone();
                                std::thread::spawn(move || {
                                    let _ = discord::send_message(&t, &cid, &content);
                                    if let Ok(ms) = discord::messages(&t, &cid, 50) {
                                        state.lock().unwrap().messages = ms;
                                    }
                                });
                                s.input.clear();
                            }
                        }
                    });
                    if ui.button("Home").clicked() {
                        s.selected_channel = None;
                        s.messages.clear();
                        s.status = "Home".into();
                        if let Some(t) = s.token.clone() {
                            let state = self.state.clone();
                            std::thread::spawn(move || {
                                if let Ok(chs) = discord::dm_channels(&t) {
                                    let mut st = state.lock().unwrap();
                                    st.channels = chs;
                                    save_cache(&st.guilds, &st.channels, &st.guild_channels);
                                    st.selected_channel = None;
                                    st.messages.clear();
                                }
                            });
                        }
                    }
                    if ui.button("Logout").clicked() { s.token = None; }
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Chat");
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        let mut to_view: Option<String> = None;
                        for m in s.messages.iter().rev() {
                            ui.label(format!("{}: {}", m.author.username, m.content));
                            for a in &m.attachments {
                                ui.horizontal(|ui| {
                                    if ui.button(format!("[{}]", a.filename)).clicked() { to_view = Some(a.url.clone()); }
                                });
                            }
                        }
                        if let Some(u) = to_view { s.viewing_url = Some(u); s.viewing_texture = None; }
                    });
                });
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Rucord", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}



