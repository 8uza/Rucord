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
    texture_rx: mpsc::Receiver<Vec<u8>>,
    input: String,
    tokens: Vec<(String, String)>,
    view_mode: ViewMode,
    online_users: Vec<(String, String)>,
    home_search: String,
    server_ctx_menu: Option<String>,
    dm_ctx_menu: Option<String>,
}

#[derive(Clone, PartialEq)]
enum ViewMode { Home, Chat }
struct App {
    state: Arc<Mutex<AppState>>,
    tx: mpsc::Sender<Result<String, String>>,
    rx: mpsc::Receiver<Result<String, String>>,
    texture_tx: mpsc::Sender<Vec<u8>>,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::channel();
        let (texture_tx, texture_rx) = mpsc::channel();
        let st = AppState {
            client_id: String::new(),
            user_token: String::new(),
            token: None,
            status: String::new(),
            logging_in: false,
            guilds: Vec::new(),
            channels: Vec::new(),
            guild_channels: Vec::new(),
            selected_guild: None,
            selected_channel: None,
            messages: Vec::new(),
            guild_icons: HashMap::new(),
            viewing_url: None,
            viewing_texture: None,
            texture_rx,
            input: String::new(),
            tokens: load_tokens(),
            view_mode: ViewMode::Home,
            online_users: Vec::new(),
            home_search: String::new(),
            server_ctx_menu: None,
            dm_ctx_menu: None,
        };
        let mut st = st;
        let c = load_cache();
        st.guilds = c.guilds;
        st.channels = c.channels;
        st.guild_channels = c.guild_channels;
        Self { state: Arc::new(Mutex::new(st)), tx, rx, texture_tx }
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

        // load viewed image (non-blocking)
        {
            let mut s = self.state.lock().unwrap();
            // receive downloaded image bytes from background thread
            if let Ok(bytes) = s.texture_rx.try_recv() {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let tex = ctx.load_texture("view", color, egui::TextureOptions::default());
                    s.viewing_texture = Some((tex, [size[0] as f32, size[1] as f32]));
                }
            }
            if let Some(url) = s.viewing_url.clone() {
                if s.viewing_texture.is_none() {
                    let tx = self.texture_tx.clone();
                    std::thread::spawn(move || {
                        if let Ok(resp) = reqwest::blocking::get(&url) {
                            if let Ok(bytes) = resp.bytes() {
                                let _ = tx.send(bytes.to_vec());
                            }
                        }
                    });
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
                egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Home").clicked() {
                            s.view_mode = ViewMode::Home;
                            s.selected_channel = None;
                            s.messages.clear();
                            s.status = "Home".into();
                            if let Some(t) = s.token.clone() {
                                let state = self.state.clone();
                                let token_clone = s.token.clone();
                                std::thread::spawn(move || {
                                    let token = token_clone.as_deref().unwrap_or("");
                                    if let Ok(chs) = discord::dm_channels(token) {
                                        let mut st = state.lock().unwrap();
                                        st.channels = chs.clone();
                                        st.online_users = chs.iter()
                                            .filter_map(|c| {
                                                let name = c.recipients.first()?;
                                                let display = name.global_name.clone().unwrap_or_else(|| name.username.clone());
                                                Some((c.id.clone(), display))
                                            })
                                            .collect();
                                        st.selected_channel = None;
                                        st.messages.clear();
                                        save_cache(&st.guilds, &st.channels, &st.guild_channels);
                                    }
                                });
                            }
                        }
                        ui.label("Rucord");
                    });
                });
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
                                    let btn = ui.add(egui::Button::image((tex.id(), egui::vec2(32.0, 32.0))).frame(false)).on_hover_text(&g.name);
                                    btn.context_menu(|ui| {
                                        if ui.button("Mute Server").clicked() {
                                            if let Some(t) = s.token.clone() {
                                                let state = self.state.clone();
                                                let gid_mute = g.id.clone();
                                                std::thread::spawn(move || {
                                                    let _ = discord::mute_guild(&state.lock().unwrap().token.clone().unwrap_or_default(), &gid_mute);
                                                });
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Leave Server").clicked() {
                                            if let Some(t) = s.token.clone() {
                                                let state = self.state.clone();
                                                let gid_leave = g.id.clone();
                                                std::thread::spawn(move || {
                                                    let _ = discord::leave_guild(&state.lock().unwrap().token.clone().unwrap_or_default(), &gid_leave);
                                                    let state2 = state.clone();
                                                    std::thread::spawn(move || {
                                                        if let Ok(gs) = discord::guilds(&state2.lock().unwrap().token.clone().unwrap_or_default()) {
                                                            let mut st3 = state2.lock().unwrap();
                                                            st3.guilds = gs;
                                                            if st3.selected_guild.as_deref() == Some(&gid_leave) {
                                                                st3.selected_guild = None;
                                                                st3.channels.clear();
                                                                st3.messages.clear();
                                                            }
                                                        }
                                                    });
                                                });
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Create Invite").clicked() {
                                            if let Some(t) = s.token.clone() {
                                                let state = self.state.clone();
                                                let gid_invite = g.id.clone();
                                                std::thread::spawn(move || {
                                                    if let Ok(inv) = discord::create_invite(&state.lock().unwrap().token.clone().unwrap_or_default(), &gid_invite) {
                                                        if let Some(code) = inv.get("code").and_then(|c| c.as_str()) {
                                                            let url = format!("https://discord.gg/{}", code);
                                                            let _ = open::that(&url);
                                                        }
                                                    }
                                                });
                                            }
                                            ui.close_menu();
                                        }
                                    });
                                    btn.clicked()
                                } else { false };
                                if click {
                                    s.view_mode = ViewMode::Chat;
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
                                if ui.add(egui::Button::new(&g.name).frame(true).min_size(egui::vec2(40.0, 32.0))).on_hover_text(&g.name).clicked() {
                                    s.view_mode = ViewMode::Chat;
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
                            s.view_mode = ViewMode::Chat;
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
                    if ui.button("Logout").clicked() { s.token = None; }
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    match s.view_mode {
                        ViewMode::Home => {
                            ui.heading("Home");
                            ui.label("Friends / Direct Messages");
                            ui.add_space(5.0);
                            ui.horizontal(|ui| {
                                ui.label("Search:");
                                ui.text_edit_singleline(&mut s.home_search);
                            });
                            ui.add_space(5.0);
                            let search = s.home_search.to_lowercase();
                            let filtered: Vec<(String, String)> = s.online_users.iter()
                                .filter(|(_, name)| name.to_lowercase().contains(&search))
                                .cloned()
                                .collect();
                            let mut open_chat: Option<String> = None;
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                if filtered.is_empty() {
                                    ui.label("No users found.");
                                } else {
                                    for (cid, name) in &filtered {
                                        ui.horizontal(|ui| {
                                            let rsp = ui.selectable_label(s.selected_channel.as_deref() == Some(cid), name);
                                            rsp.context_menu(|ui| {
                                                if ui.button("Close DM").clicked() {
                                                    if let Some(t) = s.token.clone() {
                                                        let state = self.state.clone();
                                                        let cid_close = cid.clone();
                                                        std::thread::spawn(move || {
                                                            let _ = discord::close_dm(&t, &cid_close);
                                                            let state2 = state.clone();
                                                            std::thread::spawn(move || {
                                                                if let Some(token) = state2.lock().unwrap().token.clone() {
                                                                    if let Ok(chs) = discord::dm_channels(&token) {
                                                                        let mut st3 = state2.lock().unwrap();
                                                                        st3.channels = chs.clone();
                                                                        st3.online_users = chs.iter()
                                                                            .filter_map(|c| {
                                                                                let name = c.recipients.first()?;
                                                                                let display = name.global_name.clone().unwrap_or_else(|| name.username.clone());
                                                                                Some((c.id.clone(), display))
                                                                            })
                                                                            .collect();
                                                                    }
                                                                }
                                                            });
                                                        });
                                                    }
                                                    ui.close_menu();
                                                }
                                                if ui.button("Block User").clicked() {
                                                    if let Some(c) = s.channels.iter().find(|c| c.id == *cid) {
                                                        if let Some(u) = c.recipients.first() {
                                                            let state = self.state.clone();
                                                            let uid_block = u.id.clone();
                                                            std::thread::spawn(move || {
                                                                let _ = discord::block_user(&state.lock().unwrap().token.clone().unwrap_or_default(), &uid_block);
                                                            });
                                                        }
                                                    }
                                                    ui.close_menu();
                                                }
                                                if ui.button("Unfriend").clicked() {
                                                    if let Some(c) = s.channels.iter().find(|c| c.id == *cid) {
                                                        if let Some(u) = c.recipients.first() {
                                                            let state = self.state.clone();
                                                            let uid_unfriend = u.id.clone();
                                                            std::thread::spawn(move || {
                                                                let _ = discord::unfriend_user(&state.lock().unwrap().token.clone().unwrap_or_default(), &uid_unfriend);
                                                            });
                                                        }
                                                    }
                                                    ui.close_menu();
                                                }
                                            });
                                            if rsp.clicked() {
                                                open_chat = Some(cid.clone());
                                            }
                                        });
                                    }
                                }
                            });
                            if let Some(cid) = open_chat {
                                s.view_mode = ViewMode::Chat;
                                s.selected_channel = Some(cid.clone());
                                s.messages.clear();
                                if let Some(t) = s.token.clone() {
                                    let state = self.state.clone();
                                    let cid = cid.clone();
                                    std::thread::spawn(move || {
                                        if let Ok(ms) = discord::messages(&t, &cid, 50) {
                                            state.lock().unwrap().messages = ms;
                                        }
                                    });
                                }
                            }
                        }
                        ViewMode::Chat => {
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
                        }
                    }
                });
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Rucord", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}












