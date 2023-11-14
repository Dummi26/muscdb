use std::{
    any::Any,
    collections::HashMap,
    io::Cursor,
    net::TcpStream,
    sync::{mpsc::Sender, Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use musicdb_lib::{
    data::{database::Database, queue::Queue, AlbumId, ArtistId, CoverId, SongId},
    load::ToFromBytes,
    server::{get, Command},
};
use speedy2d::{
    color::Color,
    dimen::{UVec2, Vec2},
    font::Font,
    image::ImageHandle,
    shape::Rectangle,
    window::{
        KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, UserEventSender,
        VirtualKeyCode, WindowCreationOptions, WindowHandler, WindowHelper,
    },
    Graphics2D,
};

use crate::{
    gui_base::Panel,
    gui_notif::{NotifInfo, NotifOverlay},
    gui_screen::GuiScreen,
    gui_text::Label,
    textcfg,
};

pub enum GuiEvent {
    Refresh,
    UpdatedQueue,
    UpdatedLibrary,
    Exit,
}

pub fn hotkey_deselect_all(modifiers: &ModifiersState, key: Option<VirtualKeyCode>) -> bool {
    !modifiers.logo()
        && !modifiers.alt()
        && modifiers.ctrl()
        && !modifiers.shift()
        && matches!(key, Some(VirtualKeyCode::S))
}
pub fn hotkey_select_all(modifiers: &ModifiersState, key: Option<VirtualKeyCode>) -> bool {
    !modifiers.logo()
        && !modifiers.alt()
        && modifiers.ctrl()
        && !modifiers.shift()
        && matches!(key, Some(VirtualKeyCode::A))
}
pub fn hotkey_select_albums(modifiers: &ModifiersState, key: Option<VirtualKeyCode>) -> bool {
    !modifiers.logo()
        && !modifiers.alt()
        && modifiers.ctrl()
        && modifiers.shift()
        && matches!(key, Some(VirtualKeyCode::A))
}
pub fn hotkey_select_songs(modifiers: &ModifiersState, key: Option<VirtualKeyCode>) -> bool {
    !modifiers.logo()
        && !modifiers.alt()
        && modifiers.ctrl()
        && modifiers.shift()
        && matches!(key, Some(VirtualKeyCode::S))
}

pub fn main(
    database: Arc<Mutex<Database>>,
    connection: TcpStream,
    get_con: get::Client<TcpStream>,
    event_sender_arc: Arc<Mutex<Option<UserEventSender<GuiEvent>>>>,
) {
    let mut config_file = super::get_config_file_path();
    config_file.push("config_gui.toml");
    let mut font = None;
    let mut line_height = 32.0;
    let mut scroll_pixels_multiplier = 1.0;
    let mut scroll_lines_multiplier = 3.0;
    let mut scroll_pages_multiplier = 0.75;
    let status_bar_text;
    let idle_top_text;
    let idle_side1_text;
    let idle_side2_text;
    match std::fs::read_to_string(&config_file) {
        Ok(cfg) => {
            if let Ok(table) = cfg.parse::<toml::Table>() {
                if let Some(path) = table["font"].as_str() {
                    if let Ok(bytes) = std::fs::read(path) {
                        if let Ok(f) = Font::new(&bytes) {
                            font = Some(f);
                        } else {
                            eprintln!("[toml] couldn't load font")
                        }
                    } else {
                        eprintln!("[toml] couldn't read font file")
                    }
                }
                if let Some(v) = table.get("line_height").and_then(|v| v.as_float()) {
                    line_height = v as _;
                }
                if let Some(v) = table
                    .get("scroll_pixels_multiplier")
                    .and_then(|v| v.as_float())
                {
                    scroll_pixels_multiplier = v;
                }
                if let Some(v) = table
                    .get("scroll_lines_multiplier")
                    .and_then(|v| v.as_float())
                {
                    scroll_lines_multiplier = v;
                }
                if let Some(v) = table
                    .get("scroll_pages_multiplier")
                    .and_then(|v| v.as_float())
                {
                    scroll_pages_multiplier = v;
                }
                if let Some(t) = table.get("text").and_then(|v| v.as_table()) {
                    if let Some(v) = t.get("status_bar").and_then(|v| v.as_str()) {
                        match v.parse() {
                            Ok(v) => status_bar_text = v,
                            Err(e) => {
                                eprintln!("[toml] `text.status_bar couldn't be parsed: {e}`");
                                std::process::exit(30);
                            }
                        }
                    } else {
                        eprintln!("[toml] missing the required `text.status_bar` string value.");
                        std::process::exit(30);
                    }
                    if let Some(v) = t.get("idle_top").and_then(|v| v.as_str()) {
                        match v.parse() {
                            Ok(v) => idle_top_text = v,
                            Err(e) => {
                                eprintln!("[toml] `text.idle_top couldn't be parsed: {e}`");
                                std::process::exit(30);
                            }
                        }
                    } else {
                        eprintln!("[toml] missing the required `text.idle_top` string value.");
                        std::process::exit(30);
                    }
                    if let Some(v) = t.get("idle_side1").and_then(|v| v.as_str()) {
                        match v.parse() {
                            Ok(v) => idle_side1_text = v,
                            Err(e) => {
                                eprintln!("[toml] `text.idle_side1 couldn't be parsed: {e}`");
                                std::process::exit(30);
                            }
                        }
                    } else {
                        eprintln!("[toml] missing the required `text.idle_side1` string value.");
                        std::process::exit(30);
                    }
                    if let Some(v) = t.get("idle_side2").and_then(|v| v.as_str()) {
                        match v.parse() {
                            Ok(v) => idle_side2_text = v,
                            Err(e) => {
                                eprintln!("[toml] `text.idle_side2 couldn't be parsed: {e}`");
                                std::process::exit(30);
                            }
                        }
                    } else {
                        eprintln!("[toml] missing the required `text.idle_side2` string value.");
                        std::process::exit(30);
                    }
                } else {
                    eprintln!("[toml] missing the required `[text]` section!");
                    std::process::exit(30);
                }
            } else {
                eprintln!("Couldn't parse config file {config_file:?} as toml!");
                std::process::exit(30);
            }
        }
        Err(e) => {
            eprintln!("[exit] no config file found at {config_file:?}: {e}");
            if let Some(p) = config_file.parent() {
                _ = std::fs::create_dir_all(p);
            }
            if std::fs::write(&config_file, include_bytes!("config_gui.toml")).is_ok() {
                eprintln!("[info] created a default config file.");
            }
            std::process::exit(25);
        }
    }
    let font = if let Some(v) = font {
        v
    } else {
        eprintln!("[toml] required: font = <string>");
        std::process::exit(30);
    };

    let window = speedy2d::Window::<GuiEvent>::new_with_user_events(
        "MusicDB Client",
        WindowCreationOptions::new_windowed(
            speedy2d::window::WindowSize::MarginPhysicalPixels(0),
            None,
        ),
    )
    .expect("couldn't open window");
    *event_sender_arc.lock().unwrap() = Some(window.create_user_event_sender());
    let sender = window.create_user_event_sender();
    window.run_loop(Gui::new(
        font,
        database,
        connection,
        Arc::new(Mutex::new(get_con)),
        event_sender_arc,
        sender,
        line_height,
        scroll_pixels_multiplier,
        scroll_lines_multiplier,
        scroll_pages_multiplier,
        GuiConfig {
            status_bar_text,
            idle_top_text,
            idle_side1_text,
            idle_side2_text,
            filter_presets_song: vec![
                (
                    "Fav".to_owned(),
                    crate::gui_library::FilterType::TagEq("Fav".to_owned()),
                ),
                (
                    "Year".to_owned(),
                    crate::gui_library::FilterType::TagWithValueInt("Year".to_owned(), 1990, 2000),
                ),
            ],
            filter_presets_album: vec![
                (
                    "Fav".to_owned(),
                    crate::gui_library::FilterType::TagEq("Fav".to_owned()),
                ),
                (
                    "Year".to_owned(),
                    crate::gui_library::FilterType::TagWithValueInt("Year".to_owned(), 1990, 2000),
                ),
            ],
            filter_presets_artist: vec![
                (
                    "Fav".to_owned(),
                    crate::gui_library::FilterType::TagEq("Fav".to_owned()),
                ),
                (
                    "Year".to_owned(),
                    crate::gui_library::FilterType::TagWithValueInt("Year".to_owned(), 1990, 2000),
                ),
            ],
        },
    ));
}

pub struct GuiConfig {
    pub status_bar_text: textcfg::TextBuilder,
    pub idle_top_text: textcfg::TextBuilder,
    pub idle_side1_text: textcfg::TextBuilder,
    pub idle_side2_text: textcfg::TextBuilder,
    pub filter_presets_song: Vec<(String, crate::gui_library::FilterType)>,
    pub filter_presets_album: Vec<(String, crate::gui_library::FilterType)>,
    pub filter_presets_artist: Vec<(String, crate::gui_library::FilterType)>,
}

pub struct Gui {
    pub event_sender: UserEventSender<GuiEvent>,
    pub database: Arc<Mutex<Database>>,
    pub connection: TcpStream,
    pub get_con: Arc<Mutex<get::Client<TcpStream>>>,
    pub gui: GuiScreen,
    pub notif_sender:
        Sender<Box<dyn FnOnce(&NotifOverlay) -> (Box<dyn GuiElem>, NotifInfo) + Send>>,
    pub size: UVec2,
    pub mouse_pos: Vec2,
    pub font: Font,
    pub covers: Option<HashMap<CoverId, GuiServerImage>>,
    pub custom_images: Option<HashMap<String, GuiServerImage>>,
    pub modifiers: ModifiersState,
    pub dragging: Option<(
        Dragging,
        Option<Box<dyn FnMut(&mut DrawInfo, &mut Graphics2D)>>,
    )>,
    pub high_performance: bool,
    pub line_height: f32,
    pub last_height: f32,
    pub scroll_pixels_multiplier: f64,
    pub scroll_lines_multiplier: f64,
    pub scroll_pages_multiplier: f64,
    pub gui_config: Option<GuiConfig>,
    last_performance_check: Instant,
    average_frame_time_ms: u32,
    frames_drawn: u32,
}
impl Gui {
    fn new(
        font: Font,
        database: Arc<Mutex<Database>>,
        connection: TcpStream,
        get_con: Arc<Mutex<get::Client<TcpStream>>>,
        event_sender_arc: Arc<Mutex<Option<UserEventSender<GuiEvent>>>>,
        event_sender: UserEventSender<GuiEvent>,
        line_height: f32,
        scroll_pixels_multiplier: f64,
        scroll_lines_multiplier: f64,
        scroll_pages_multiplier: f64,
        gui_config: GuiConfig,
    ) -> Self {
        let (notif_overlay, notif_sender) = NotifOverlay::new();
        let notif_sender_two = notif_sender.clone();
        database.lock().unwrap().update_endpoints.push(
            musicdb_lib::data::database::UpdateEndpoint::Custom(Box::new(move |cmd| match cmd {
                Command::Resume
                | Command::Pause
                | Command::Stop
                | Command::Save
                | Command::InitComplete => {}
                Command::NextSong
                | Command::QueueUpdate(..)
                | Command::QueueAdd(..)
                | Command::QueueInsert(..)
                | Command::QueueRemove(..)
                | Command::QueueGoto(..)
                | Command::QueueSetShuffle(..) => {
                    if let Some(s) = &*event_sender_arc.lock().unwrap() {
                        _ = s.send_event(GuiEvent::UpdatedQueue);
                    }
                }
                Command::SyncDatabase(..)
                | Command::AddSong(_)
                | Command::AddAlbum(_)
                | Command::AddArtist(_)
                | Command::AddCover(_)
                | Command::ModifySong(_)
                | Command::ModifyAlbum(_)
                | Command::ModifyArtist(_)
                | Command::RemoveSong(_)
                | Command::RemoveAlbum(_)
                | Command::RemoveArtist(_)
                | Command::SetSongDuration(..) => {
                    if let Some(s) = &*event_sender_arc.lock().unwrap() {
                        _ = s.send_event(GuiEvent::UpdatedLibrary);
                    }
                }
                Command::ErrorInfo(t, d) => {
                    let (t, d) = (t.clone(), d.clone());
                    notif_sender_two
                        .send(Box::new(move |_| {
                            (
                                Box::new(Panel::with_background(
                                    GuiElemCfg::default(),
                                    [Label::new(
                                        GuiElemCfg::default(),
                                        if t.is_empty() {
                                            format!("Server message\n{d}")
                                        } else {
                                            format!("Server error ({t})\n{d}")
                                        },
                                        Color::WHITE,
                                        None,
                                        Vec2::new(0.5, 0.5),
                                    )],
                                    Color::from_rgba(0.0, 0.0, 0.0, 0.8),
                                )),
                                if t.is_empty() {
                                    NotifInfo::new(Duration::from_secs(2))
                                } else {
                                    NotifInfo::new(Duration::from_secs(5))
                                        .with_highlight(Color::RED)
                                },
                            )
                        }))
                        .unwrap();
                }
            })),
        );
        let no_animations = false;
        Gui {
            event_sender,
            database,
            connection,
            get_con,
            gui: GuiScreen::new(
                GuiElemCfg::default(),
                notif_overlay,
                no_animations,
                line_height,
                scroll_pixels_multiplier,
                scroll_lines_multiplier,
                scroll_pages_multiplier,
            ),
            notif_sender,
            size: UVec2::ZERO,
            mouse_pos: Vec2::ZERO,
            font,
            covers: Some(HashMap::new()),
            custom_images: Some(HashMap::new()),
            // font: Font::new(include_bytes!("/usr/share/fonts/TTF/FiraSans-Regular.ttf")).unwrap(),
            modifiers: ModifiersState::default(),
            dragging: None,
            high_performance: no_animations,
            line_height,
            last_height: 720.0,
            scroll_pixels_multiplier,
            scroll_lines_multiplier,
            scroll_pages_multiplier,
            gui_config: Some(gui_config),
            last_performance_check: Instant::now(),
            average_frame_time_ms: 0,
            frames_drawn: 0,
        }
    }
}

/// the trait implemented by all Gui elements.
/// feel free to override the methods you wish to use.
#[allow(unused)]
pub trait GuiElem {
    fn config(&self) -> &GuiElemCfg;
    fn config_mut(&mut self) -> &mut GuiElemCfg;
    /// note: drawing happens from the last to the first element, while priority is from first to last.
    /// if you wish to add a "high priority" child to a Vec<GuiElem> using push, .rev() the iterator in this method and change draw_rev to false.
    fn children(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_>;
    /// defaults to true.
    fn draw_rev(&self) -> bool {
        true
    }
    fn any(&self) -> &dyn Any;
    fn any_mut(&mut self) -> &mut dyn Any;
    fn elem(&self) -> &dyn GuiElem;
    fn elem_mut(&mut self) -> &mut dyn GuiElem;
    /// handles drawing.
    fn draw(&mut self, info: &mut DrawInfo, g: &mut Graphics2D) {}
    /// an event that is invoked whenever a mouse button is pressed on the element.
    fn mouse_down(&mut self, button: MouseButton) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    /// an event that is invoked whenever a mouse button that was pressed on the element is released anywhere.
    fn mouse_up(&mut self, button: MouseButton) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    /// an event that is invoked after a mouse button was pressed and released on the same GUI element.
    fn mouse_pressed(&mut self, button: MouseButton) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    fn mouse_wheel(&mut self, diff: f32) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    fn char_watch(&mut self, modifiers: ModifiersState, key: char) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    fn char_focus(&mut self, modifiers: ModifiersState, key: char) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    fn key_watch(
        &mut self,
        modifiers: ModifiersState,
        down: bool,
        key: Option<VirtualKeyCode>,
        scan: KeyScancode,
    ) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    fn key_focus(
        &mut self,
        modifiers: ModifiersState,
        down: bool,
        key: Option<VirtualKeyCode>,
        scan: KeyScancode,
    ) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    /// When something is dragged and released over this element
    fn dragged(&mut self, dragged: Dragging) -> Vec<GuiAction> {
        Vec::with_capacity(0)
    }
    fn updated_library(&mut self) {}
    fn updated_queue(&mut self) {}
}
impl<T: GuiElem + ?Sized> GuiElemInternal for T {}
pub(crate) trait GuiElemInternal: GuiElem {
    fn _draw(&mut self, info: &mut DrawInfo, g: &mut Graphics2D) {
        if !self.config_mut().enabled {
            return;
        }
        // adjust info
        let npos = adjust_area(&info.pos, &self.config_mut().pos);
        let ppos = std::mem::replace(&mut info.pos, npos);
        if info.child_has_keyboard_focus {
            if self.config().keyboard_focus_index == usize::MAX {
                info.has_keyboard_focus = true;
                info.child_has_keyboard_focus = false;
            }
        }
        // call trait's draw function
        self.draw(info, g);
        // reset info
        info.has_keyboard_focus = false;
        let focus_path = info.child_has_keyboard_focus;
        // children (in reverse order - first element has the highest priority)
        let kbd_focus_index = self.config().keyboard_focus_index;
        if self.draw_rev() {
            for (i, c) in self
                .children()
                .collect::<Vec<_>>()
                .into_iter()
                .enumerate()
                .rev()
            {
                info.child_has_keyboard_focus = focus_path && i == kbd_focus_index;
                c._draw(info, g);
            }
        } else {
            for (i, c) in self.children().enumerate() {
                info.child_has_keyboard_focus = focus_path && i == kbd_focus_index;
                c._draw(info, g);
            }
        }
        // reset pt. 2
        info.child_has_keyboard_focus = focus_path;
        self.config_mut().pixel_pos = std::mem::replace(&mut info.pos, ppos);
    }
    /// recursively applies the function to all gui elements below and including this one
    fn _recursive_all(&mut self, f: &mut dyn FnMut(&mut dyn GuiElem)) {
        f(self.elem_mut());
        for c in self.children() {
            c._recursive_all(f);
        }
    }
    fn _mouse_event(
        &mut self,
        condition: &mut dyn FnMut(&mut dyn GuiElem) -> Option<Vec<GuiAction>>,
        pos: Vec2,
    ) -> Option<Vec<GuiAction>> {
        for c in &mut self.children() {
            if c.config().enabled {
                if c.config().pixel_pos.contains(pos) {
                    if let Some(v) = c._mouse_event(condition, pos) {
                        return Some(v);
                    }
                }
            }
        }
        condition(self.elem_mut())
    }
    fn _release_drag(
        &mut self,
        dragged: &mut Option<Dragging>,
        pos: Vec2,
    ) -> Option<Vec<GuiAction>> {
        self._mouse_event(
            &mut |v| {
                if v.config().drag_target {
                    if let Some(d) = dragged.take() {
                        return Some(v.dragged(d));
                    }
                }
                None
            },
            pos,
        )
    }
    fn _mouse_button(
        &mut self,
        button: MouseButton,
        down: bool,
        pos: Vec2,
    ) -> Option<Vec<GuiAction>> {
        if down {
            self._mouse_event(
                &mut |v: &mut dyn GuiElem| {
                    if v.config().mouse_events {
                        match button {
                            MouseButton::Left => v.config_mut().mouse_down.0 = true,
                            MouseButton::Middle => v.config_mut().mouse_down.1 = true,
                            MouseButton::Right => v.config_mut().mouse_down.2 = true,
                            MouseButton::Other(_) => {}
                        }
                        Some(v.mouse_down(button))
                    } else {
                        None
                    }
                },
                pos,
            )
        } else {
            let mut vec = vec![];
            if let Some(a) = self._mouse_event(
                &mut |v: &mut dyn GuiElem| {
                    let down = v.config().mouse_down;
                    if v.config().mouse_events
                        && ((button == MouseButton::Left && down.0)
                            || (button == MouseButton::Middle && down.1)
                            || (button == MouseButton::Right && down.2))
                    {
                        Some(v.mouse_pressed(button))
                    } else {
                        None
                    }
                },
                pos,
            ) {
                vec.extend(a);
            };
            self._recursive_all(&mut |v| {
                if v.config().mouse_events {
                    match button {
                        MouseButton::Left => v.config_mut().mouse_down.0 = false,
                        MouseButton::Middle => v.config_mut().mouse_down.1 = false,
                        MouseButton::Right => v.config_mut().mouse_down.2 = false,
                        MouseButton::Other(_) => {}
                    }
                    vec.extend(v.mouse_up(button));
                }
            });
            Some(vec)
        }
    }
    fn _mouse_wheel(&mut self, diff: f32, pos: Vec2) -> Option<Vec<GuiAction>> {
        self._mouse_event(
            &mut |v| {
                if v.config().scroll_events {
                    Some(v.mouse_wheel(diff))
                } else {
                    None
                }
            },
            pos,
        )
    }
    fn _keyboard_event(
        &mut self,
        f_focus: &mut dyn FnMut(&mut dyn GuiElem, &mut Vec<GuiAction>),
        f_watch: &mut dyn FnMut(&mut dyn GuiElem, &mut Vec<GuiAction>),
    ) -> Vec<GuiAction> {
        let mut o = vec![];
        self._keyboard_event_inner(&mut Some(f_focus), f_watch, &mut o, true);
        o
    }
    fn _keyboard_event_inner(
        &mut self,
        f_focus: &mut Option<&mut dyn FnMut(&mut dyn GuiElem, &mut Vec<GuiAction>)>,
        f_watch: &mut dyn FnMut(&mut dyn GuiElem, &mut Vec<GuiAction>),
        events: &mut Vec<GuiAction>,
        focus: bool,
    ) {
        f_watch(self.elem_mut(), events);
        let focus_index = self.config().keyboard_focus_index;
        for (i, child) in self.children().enumerate() {
            child._keyboard_event_inner(f_focus, f_watch, events, focus && i == focus_index);
        }
        if focus {
            // we have focus and no child has consumed f_focus
            if let Some(f) = f_focus.take() {
                f(self.elem_mut(), events)
            }
        }
    }
    fn _keyboard_move_focus(&mut self, decrement: bool, refocus: bool) -> bool {
        let mut focus_index = if refocus {
            usize::MAX
        } else {
            self.config().keyboard_focus_index
        };
        let allow_focus = self.config().keyboard_events_focus;
        let mut children = self.children().collect::<Vec<_>>();
        if focus_index == usize::MAX {
            if decrement {
                focus_index = children.len().saturating_sub(1);
            } else {
                focus_index = 0;
            }
        }
        let mut changed = refocus;
        let ok = loop {
            if let Some(child) = children.get_mut(focus_index) {
                if child._keyboard_move_focus(decrement, changed) {
                    break true;
                } else {
                    changed = true;
                    if !decrement {
                        focus_index += 1;
                    } else {
                        focus_index = focus_index.wrapping_sub(1);
                    }
                }
            } else {
                focus_index = usize::MAX;
                break allow_focus && refocus;
            }
        };
        self.config_mut().keyboard_focus_index = focus_index;
        ok
    }
    fn _keyboard_reset_focus(&mut self) -> bool {
        let mut index = usize::MAX;
        for (i, c) in self.children().enumerate() {
            if c._keyboard_reset_focus() {
                index = i;
                break;
            }
        }
        let wants = std::mem::replace(&mut self.config_mut().request_keyboard_focus, false);
        self.config_mut().keyboard_focus_index = index;
        index != usize::MAX || wants
    }
}

pub trait GuiElemWrapper {
    fn as_elem(&self) -> &dyn GuiElem;
    fn as_elem_mut(&mut self) -> &mut dyn GuiElem;
}
impl<T: GuiElem> GuiElemWrapper for Box<T> {
    fn as_elem(&self) -> &dyn GuiElem {
        self.as_ref()
    }
    fn as_elem_mut(&mut self) -> &mut dyn GuiElem {
        self.as_mut()
    }
}
impl GuiElemWrapper for Box<dyn GuiElem> {
    fn as_elem(&self) -> &dyn GuiElem {
        self.as_ref()
    }
    fn as_elem_mut(&mut self) -> &mut dyn GuiElem {
        self.as_mut()
    }
}

impl<T: GuiElemWrapper> GuiElem for T {
    fn config(&self) -> &GuiElemCfg {
        self.as_elem().config()
    }
    fn config_mut(&mut self) -> &mut GuiElemCfg {
        self.as_elem_mut().config_mut()
    }
    fn children(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        self.as_elem_mut().children()
    }
    fn draw_rev(&self) -> bool {
        self.as_elem().draw_rev()
    }
    fn any(&self) -> &dyn Any {
        self.as_elem().any()
    }
    fn any_mut(&mut self) -> &mut dyn Any {
        self.as_elem_mut().any_mut()
    }
    fn elem(&self) -> &dyn GuiElem {
        self.as_elem().elem()
    }
    fn elem_mut(&mut self) -> &mut dyn GuiElem {
        self.as_elem_mut().elem_mut()
    }
    fn draw(&mut self, info: &mut DrawInfo, g: &mut Graphics2D) {
        self.as_elem_mut().draw(info, g)
    }
    fn mouse_down(&mut self, button: MouseButton) -> Vec<GuiAction> {
        self.as_elem_mut().mouse_down(button)
    }
    fn mouse_up(&mut self, button: MouseButton) -> Vec<GuiAction> {
        self.as_elem_mut().mouse_up(button)
    }
    fn mouse_pressed(&mut self, button: MouseButton) -> Vec<GuiAction> {
        self.as_elem_mut().mouse_pressed(button)
    }
    fn mouse_wheel(&mut self, diff: f32) -> Vec<GuiAction> {
        self.as_elem_mut().mouse_wheel(diff)
    }
    fn char_watch(&mut self, modifiers: ModifiersState, key: char) -> Vec<GuiAction> {
        self.as_elem_mut().char_watch(modifiers, key)
    }
    fn char_focus(&mut self, modifiers: ModifiersState, key: char) -> Vec<GuiAction> {
        self.as_elem_mut().char_focus(modifiers, key)
    }
    fn key_watch(
        &mut self,
        modifiers: ModifiersState,
        down: bool,
        key: Option<VirtualKeyCode>,
        scan: KeyScancode,
    ) -> Vec<GuiAction> {
        self.as_elem_mut().key_watch(modifiers, down, key, scan)
    }
    fn key_focus(
        &mut self,
        modifiers: ModifiersState,
        down: bool,
        key: Option<VirtualKeyCode>,
        scan: KeyScancode,
    ) -> Vec<GuiAction> {
        self.as_elem_mut().key_focus(modifiers, down, key, scan)
    }
    fn dragged(&mut self, dragged: Dragging) -> Vec<GuiAction> {
        self.as_elem_mut().dragged(dragged)
    }
    fn updated_library(&mut self) {
        self.as_elem_mut().updated_library()
    }
    fn updated_queue(&mut self) {
        self.as_elem_mut().updated_queue()
    }
}

pub trait GuiElemChildren {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_>;
    fn len(&self) -> usize;
}
impl GuiElemChildren for () {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new([].into_iter())
    }
    fn len(&self) -> usize {
        0
    }
}
impl<const N: usize, T: GuiElem> GuiElemChildren for [T; N] {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new(self.iter_mut().map(|v| v.elem_mut()))
    }
    fn len(&self) -> usize {
        N
    }
}
impl<T: GuiElem> GuiElemChildren for Vec<T> {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new(self.iter_mut().map(|v| v.elem_mut()))
    }
    fn len(&self) -> usize {
        self.len()
    }
}
impl<A: GuiElem, B: GuiElem> GuiElemChildren for (A, B) {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new([self.0.elem_mut(), self.1.elem_mut()].into_iter())
    }
    fn len(&self) -> usize {
        2
    }
}
impl<A: GuiElem, B: GuiElem, C: GuiElem> GuiElemChildren for (A, B, C) {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new([self.0.elem_mut(), self.1.elem_mut(), self.2.elem_mut()].into_iter())
    }
    fn len(&self) -> usize {
        3
    }
}
impl<A: GuiElem, B: GuiElem, C: GuiElem, D: GuiElem> GuiElemChildren for (A, B, C, D) {
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new(
            [
                self.0.elem_mut(),
                self.1.elem_mut(),
                self.2.elem_mut(),
                self.3.elem_mut(),
            ]
            .into_iter(),
        )
    }
    fn len(&self) -> usize {
        4
    }
}
impl<A: GuiElem, B: GuiElem, C: GuiElem, D: GuiElem, E: GuiElem> GuiElemChildren
    for (A, B, C, D, E)
{
    fn iter(&mut self) -> Box<dyn Iterator<Item = &mut dyn GuiElem> + '_> {
        Box::new(
            [
                self.0.elem_mut(),
                self.1.elem_mut(),
                self.2.elem_mut(),
                self.3.elem_mut(),
                self.4.elem_mut(),
            ]
            .into_iter(),
        )
    }
    fn len(&self) -> usize {
        5
    }
}

#[derive(Debug, Clone)]
/// The config for any gui element.
pub struct GuiElemCfg {
    pub enabled: bool,
    /// if true, indicates that something (text size, screen size, ...) has changed
    /// and you should probably relayout and redraw from scratch.
    pub redraw: bool,
    /// Position relative to the parent where this element should be drawn.
    /// ((0, 0), (1, 1)) is the default and fills all available space.
    /// ((0, 0.5), (0.5, 1)) fills the bottom left quarter.
    pub pos: Rectangle,
    /// the pixel position after the last call to draw().
    /// in draw, use info.pos instead, as pixel_pos is only updated *after* draw().
    /// this can act like a "previous pos" field within draw.
    pub pixel_pos: Rectangle,
    /// which mouse buttons were pressed down while the mouse was on this element and haven't been released since? (Left/Middle/Right)
    pub mouse_down: (bool, bool, bool),
    /// Set this to true to receive mouse click events when the mouse is within this element's bounds
    pub mouse_events: bool,
    /// Set this to true to receive scroll events when the mouse is within this element's bounds
    pub scroll_events: bool,
    /// allows elements to watch all keyboard events, regardless of keyboard focus.
    pub keyboard_events_watch: bool,
    /// indicates that this element can have the keyboard focus
    pub keyboard_events_focus: bool,
    /// index of the child that has keyboard focus. if usize::MAX, `self` has focus.
    /// will automatically be changed when Tab is pressed (Tab skips elements with keyboard_events_focus == false)
    pub keyboard_focus_index: usize,
    /// if this is true and ResetKeyboardFocus is returned, this element may get the keyboard focus (guaranteed if no other element has this set to true)
    pub request_keyboard_focus: bool,
    /// if this is true, things can be dragged into this element via drag-n-drop
    pub drag_target: bool,
}
#[allow(unused)]
impl GuiElemCfg {
    pub fn at(pos: Rectangle) -> Self {
        Self {
            pos,
            ..Default::default()
        }
    }
    pub fn w_mouse(mut self) -> Self {
        self.mouse_events = true;
        self
    }
    pub fn w_scroll(mut self) -> Self {
        self.scroll_events = true;
        self
    }
    pub fn w_keyboard_watch(mut self) -> Self {
        self.keyboard_events_watch = true;
        self
    }
    pub fn w_keyboard_focus(mut self) -> Self {
        self.keyboard_events_focus = true;
        self
    }
    pub fn w_drag_target(mut self) -> Self {
        self.drag_target = true;
        self
    }
    pub fn force_redraw(mut self) -> Self {
        self.redraw = true;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}
impl Default for GuiElemCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            redraw: false,
            pos: Rectangle::new(Vec2::ZERO, Vec2::new(1.0, 1.0)),
            pixel_pos: Rectangle::ZERO,
            mouse_down: (false, false, false),
            mouse_events: false,
            scroll_events: false,
            keyboard_events_watch: false,
            keyboard_events_focus: false,
            keyboard_focus_index: usize::MAX,
            request_keyboard_focus: false,
            drag_target: false,
        }
    }
}
#[allow(unused)]
pub enum GuiAction {
    OpenMain,
    /// false -> prevent idling, true -> end idling even if already idle
    EndIdle(bool),
    SetHighPerformance(bool),
    OpenSettings(bool),
    ShowNotification(Box<dyn FnOnce(&NotifOverlay) -> (Box<dyn GuiElem>, NotifInfo) + Send>),
    /// Build the GuiAction(s) later, when we have access to the Database (can turn an AlbumId into a QueueContent::Folder, etc)
    Build(Box<dyn FnOnce(&mut Database) -> Vec<Self>>),
    SendToServer(Command),
    ContextMenu(Option<Box<dyn GuiElem>>),
    /// unfocuses all gui elements, then assigns keyboard focus to one with config().request_keyboard_focus == true if there is one.
    ResetKeyboardFocus,
    SetDragging(
        Option<(
            Dragging,
            Option<Box<dyn FnMut(&mut DrawInfo, &mut Graphics2D)>>,
        )>,
    ),
    SetLineHeight(f32),
    LoadCover(CoverId),
    /// Run a custom closure with mutable access to the Gui struct
    Do(Box<dyn FnMut(&mut Gui)>),
    Exit,
}
pub enum Dragging {
    Artist(ArtistId),
    Album(AlbumId),
    Song(SongId),
    Queue(Queue),
    Queues(Vec<Queue>),
}

/// GuiElems have access to this within draw.
/// Except for `actions`, they should not change any of these values - GuiElem::draw will handle everything automatically.
pub struct DrawInfo<'a> {
    pub time: Instant,
    pub actions: Vec<GuiAction>,
    pub pos: Rectangle,
    pub database: &'a mut Database,
    pub font: &'a Font,
    /// absolute position of the mouse on the screen.
    /// compare this to `pos` to find the mouse's relative position.
    pub mouse_pos: Vec2,
    pub helper: Option<&'a mut WindowHelper<GuiEvent>>,
    pub get_con: Arc<Mutex<get::Client<TcpStream>>>,
    pub covers: &'a mut HashMap<CoverId, GuiServerImage>,
    pub custom_images: &'a mut HashMap<String, GuiServerImage>,
    pub has_keyboard_focus: bool,
    pub child_has_keyboard_focus: bool,
    /// the height of one line of text (in pixels)
    pub line_height: f32,
    pub dragging: Option<(
        Dragging,
        Option<Box<dyn FnMut(&mut DrawInfo, &mut Graphics2D)>>,
    )>,
    pub context_menu: Option<Box<dyn GuiElem>>,
    pub gui_config: &'a mut GuiConfig,
    pub high_performance: bool,
}

pub fn adjust_area(outer: &Rectangle, rel_area: &Rectangle) -> Rectangle {
    Rectangle::new(
        adjust_pos(outer, rel_area.top_left()),
        adjust_pos(outer, rel_area.bottom_right()),
    )
}
pub fn adjust_pos(outer: &Rectangle, rel_pos: &Vec2) -> Vec2 {
    Vec2::new(
        outer.top_left().x + outer.width() * rel_pos.x,
        outer.top_left().y + outer.height() * rel_pos.y,
    )
}

impl Gui {
    pub fn exec_gui_action(&mut self, action: GuiAction) {
        match action {
            GuiAction::Build(f) => {
                let actions = f(&mut *self.database.lock().unwrap());
                for action in actions {
                    self.exec_gui_action(action);
                }
            }
            GuiAction::SendToServer(cmd) => {
                if let Err(e) = cmd.to_bytes(&mut self.connection) {
                    eprintln!("Error sending command to server: {e}");
                }
            }
            GuiAction::ShowNotification(func) => _ = self.notif_sender.send(func),
            GuiAction::ResetKeyboardFocus => _ = self.gui._keyboard_reset_focus(),
            GuiAction::SetDragging(d) => self.dragging = d,
            GuiAction::SetHighPerformance(d) => self.high_performance = d,
            GuiAction::ContextMenu(m) => self.gui.c_context_menu = m,
            GuiAction::SetLineHeight(h) => {
                self.line_height = h;
                self.gui
                    ._recursive_all(&mut |e| e.config_mut().redraw = true);
            }
            GuiAction::LoadCover(id) => {
                self.covers
                    .as_mut()
                    .unwrap()
                    .insert(id, GuiServerImage::new_cover(id, Arc::clone(&self.get_con)));
            }
            GuiAction::Do(mut f) => f(self),
            GuiAction::Exit => _ = self.event_sender.send_event(GuiEvent::Exit),
            GuiAction::EndIdle(v) => {
                if v {
                    self.gui.unidle();
                } else {
                    self.gui.not_idle();
                }
            }
            GuiAction::OpenSettings(v) => {
                self.gui.idle.target = 0.0;
                self.gui.last_interaction = Instant::now();
                if self.gui.settings.0 != v {
                    self.gui.settings = (v, Some(Instant::now()));
                }
            }
            GuiAction::OpenMain => {
                self.gui.idle.target = 0.0;
                self.gui.last_interaction = Instant::now();
                if self.gui.settings.0 {
                    self.gui.settings = (false, Some(Instant::now()));
                }
            }
        }
    }
}
impl WindowHandler<GuiEvent> for Gui {
    fn on_draw(&mut self, helper: &mut WindowHelper<GuiEvent>, graphics: &mut Graphics2D) {
        let draw_start_time = Instant::now();
        graphics.draw_rectangle(
            Rectangle::new(Vec2::ZERO, self.size.into_f32()),
            Color::BLACK,
        );
        let mut dblock = self.database.lock().unwrap();
        let mut covers = self.covers.take().unwrap();
        let mut custom_images = self.custom_images.take().unwrap();
        let mut cfg = self.gui_config.take().unwrap();
        let mut info = DrawInfo {
            time: draw_start_time,
            actions: Vec::with_capacity(0),
            pos: Rectangle::new(Vec2::ZERO, self.size.into_f32()),
            database: &mut *dblock,
            font: &self.font,
            mouse_pos: self.mouse_pos,
            get_con: Arc::clone(&self.get_con),
            covers: &mut covers,
            custom_images: &mut custom_images,
            helper: Some(helper),
            has_keyboard_focus: false,
            child_has_keyboard_focus: true,
            line_height: self.line_height,
            high_performance: self.high_performance,
            dragging: self.dragging.take(),
            context_menu: self.gui.c_context_menu.take(),
            gui_config: &mut cfg,
        };
        self.gui._draw(&mut info, graphics);
        let actions = std::mem::replace(&mut info.actions, Vec::with_capacity(0));
        self.gui.c_context_menu = info.context_menu.take();
        self.dragging = info.dragging.take();
        if let Some((d, f)) = &mut self.dragging {
            if let Some(f) = f {
                f(&mut info, graphics);
            } else {
                match d {
                    Dragging::Artist(_) => graphics.draw_circle(
                        self.mouse_pos,
                        25.0,
                        Color::from_int_rgba(0, 100, 255, 100),
                    ),
                    Dragging::Album(_) => graphics.draw_circle(
                        self.mouse_pos,
                        25.0,
                        Color::from_int_rgba(0, 100, 255, 100),
                    ),
                    Dragging::Song(_) => graphics.draw_circle(
                        self.mouse_pos,
                        25.0,
                        Color::from_int_rgba(0, 100, 255, 100),
                    ),
                    Dragging::Queue(_) => graphics.draw_circle(
                        self.mouse_pos,
                        25.0,
                        Color::from_int_rgba(100, 0, 255, 100),
                    ),
                    Dragging::Queues(_) => graphics.draw_circle(
                        self.mouse_pos,
                        25.0,
                        Color::from_int_rgba(100, 0, 255, 100),
                    ),
                }
            }
        }
        // cleanup
        drop(info);
        self.gui_config = Some(cfg);
        self.covers = Some(covers);
        self.custom_images = Some(custom_images);
        drop(dblock);
        for a in actions {
            self.exec_gui_action(a);
        }
        let ft = draw_start_time.elapsed().as_millis() as u32;
        self.average_frame_time_ms = (self.average_frame_time_ms * 7 + ft) / 8;
        if !self.high_performance && self.average_frame_time_ms > 50 {
            self.high_performance = true;
            *self
                .gui
                .c_settings
                .c_scroll_box
                .children
                .performance_toggle
                .children
                .1
                .children[0]
                .content
                .text() = "On due to\nbad performance".to_string();
        }
        // #[cfg(debug_assertions)]
        {
            self.frames_drawn += 1;
            if draw_start_time
                .duration_since(self.last_performance_check)
                .as_secs()
                >= 1
            {
                self.last_performance_check = draw_start_time;
                eprintln!(
                    "[performance] {} fps | {}ms",
                    self.frames_drawn, self.average_frame_time_ms
                );
                self.frames_drawn = 0;
            }
        }
    }
    fn on_mouse_button_down(&mut self, helper: &mut WindowHelper<GuiEvent>, button: MouseButton) {
        if let Some(a) = self.gui._mouse_button(button, true, self.mouse_pos.clone()) {
            for a in a {
                self.exec_gui_action(a)
            }
        }
        helper.request_redraw();
    }
    fn on_mouse_button_up(&mut self, helper: &mut WindowHelper<GuiEvent>, button: MouseButton) {
        if self.dragging.is_some() {
            if let Some(a) = self.gui._release_drag(
                &mut self.dragging.take().map(|v| v.0),
                self.mouse_pos.clone(),
            ) {
                for a in a {
                    self.exec_gui_action(a)
                }
            }
        }
        if let Some(a) = self
            .gui
            ._mouse_button(button, false, self.mouse_pos.clone())
        {
            for a in a {
                self.exec_gui_action(a)
            }
        }
        helper.request_redraw();
    }
    fn on_mouse_wheel_scroll(
        &mut self,
        helper: &mut WindowHelper<GuiEvent>,
        distance: speedy2d::window::MouseScrollDistance,
    ) {
        let dist = match distance {
            MouseScrollDistance::Pixels { y, .. } => {
                (self.scroll_pixels_multiplier * y * self.scroll_lines_multiplier) as f32
            }
            MouseScrollDistance::Lines { y, .. } => {
                (self.scroll_lines_multiplier * y) as f32 * self.line_height
            }
            MouseScrollDistance::Pages { y, .. } => {
                (self.scroll_pages_multiplier * y * self.scroll_lines_multiplier) as f32
                    * self.last_height
            }
        };
        if let Some(a) = self.gui._mouse_wheel(dist, self.mouse_pos.clone()) {
            for a in a {
                self.exec_gui_action(a)
            }
        }
        helper.request_redraw();
    }
    fn on_keyboard_char(&mut self, helper: &mut WindowHelper<GuiEvent>, unicode_codepoint: char) {
        helper.request_redraw();
        for a in self.gui._keyboard_event(
            &mut |e, a| {
                if e.config().keyboard_events_focus {
                    a.append(&mut e.char_focus(self.modifiers.clone(), unicode_codepoint));
                }
            },
            &mut |e, a| {
                if e.config().keyboard_events_watch {
                    a.append(&mut e.char_watch(self.modifiers.clone(), unicode_codepoint));
                }
            },
        ) {
            self.exec_gui_action(a);
        }
    }
    fn on_key_down(
        &mut self,
        helper: &mut WindowHelper<GuiEvent>,
        virtual_key_code: Option<VirtualKeyCode>,
        scancode: KeyScancode,
    ) {
        helper.request_redraw();
        if let Some(VirtualKeyCode::Tab) = virtual_key_code {
            if !(self.modifiers.ctrl() || self.modifiers.alt() || self.modifiers.logo()) {
                self.gui._keyboard_move_focus(self.modifiers.shift(), false);
            }
        }
        for a in self.gui._keyboard_event(
            &mut |e, a| {
                if e.config().keyboard_events_focus {
                    a.append(&mut e.key_focus(
                        self.modifiers.clone(),
                        true,
                        virtual_key_code,
                        scancode,
                    ));
                }
            },
            &mut |e, a| {
                if e.config().keyboard_events_watch {
                    a.append(&mut e.key_watch(
                        self.modifiers.clone(),
                        true,
                        virtual_key_code,
                        scancode,
                    ));
                }
            },
        ) {
            self.exec_gui_action(a);
        }
    }
    fn on_key_up(
        &mut self,
        helper: &mut WindowHelper<GuiEvent>,
        virtual_key_code: Option<VirtualKeyCode>,
        scancode: KeyScancode,
    ) {
        helper.request_redraw();
        for a in self.gui._keyboard_event(
            &mut |e, a| {
                if e.config().keyboard_events_focus {
                    a.append(&mut e.key_focus(
                        self.modifiers.clone(),
                        false,
                        virtual_key_code,
                        scancode,
                    ));
                }
            },
            &mut |e, a| {
                if e.config().keyboard_events_watch {
                    a.append(&mut e.key_watch(
                        self.modifiers.clone(),
                        false,
                        virtual_key_code,
                        scancode,
                    ));
                }
            },
        ) {
            self.exec_gui_action(a);
        }
    }
    fn on_keyboard_modifiers_changed(
        &mut self,
        _helper: &mut WindowHelper<GuiEvent>,
        state: ModifiersState,
    ) {
        self.modifiers = state;
    }
    fn on_user_event(&mut self, helper: &mut WindowHelper<GuiEvent>, user_event: GuiEvent) {
        match user_event {
            GuiEvent::Refresh => helper.request_redraw(),
            GuiEvent::UpdatedLibrary => {
                self.gui._recursive_all(&mut |e| e.updated_library());
                helper.request_redraw();
            }
            GuiEvent::UpdatedQueue => {
                self.gui._recursive_all(&mut |e| e.updated_queue());
                helper.request_redraw();
            }
            GuiEvent::Exit => helper.terminate_loop(),
        }
    }
    fn on_mouse_move(&mut self, helper: &mut WindowHelper<GuiEvent>, position: Vec2) {
        self.mouse_pos = position;
        helper.request_redraw();
    }
    fn on_resize(&mut self, _helper: &mut WindowHelper<GuiEvent>, size_pixels: UVec2) {
        self.size = size_pixels;
        self.gui
            ._recursive_all(&mut |e| e.config_mut().redraw = true);
    }
}

pub enum GuiServerImage {
    Loading(JoinHandle<Option<Vec<u8>>>),
    Loaded(ImageHandle),
    Error,
}
#[allow(unused)]
impl GuiServerImage {
    pub fn new_cover(id: CoverId, get_con: Arc<Mutex<get::Client<TcpStream>>>) -> Self {
        Self::Loading(std::thread::spawn(move || {
            get_con
                .lock()
                .unwrap()
                .cover_bytes(id)
                .ok()
                .and_then(|v| v.ok())
        }))
    }
    pub fn new_custom_file(file: String, get_con: Arc<Mutex<get::Client<TcpStream>>>) -> Self {
        Self::Loading(std::thread::spawn(move || {
            get_con
                .lock()
                .unwrap()
                .custom_file(&file)
                .ok()
                .and_then(|v| v.ok())
        }))
    }
    pub fn get(&self) -> Option<ImageHandle> {
        match self {
            Self::Loaded(handle) => Some(handle.clone()),
            Self::Loading(_) | Self::Error => None,
        }
    }
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Error)
    }
    pub fn get_init(&mut self, g: &mut Graphics2D) -> Option<ImageHandle> {
        match self {
            Self::Loaded(handle) => Some(handle.clone()),
            Self::Error => None,
            Self::Loading(t) => {
                if t.is_finished() {
                    let s = std::mem::replace(self, Self::Error);
                    if let Self::Loading(t) = s {
                        match t.join().unwrap() {
                            Some(bytes) => match g.create_image_from_file_bytes(
                                None,
                                speedy2d::image::ImageSmoothingMode::Linear,
                                Cursor::new(bytes),
                            ) {
                                Ok(handle) => {
                                    *self = Self::Loaded(handle.clone());
                                    Some(handle)
                                }
                                Err(e) => {
                                    eprintln!("[info] couldn't load cover from bytes: {e}");
                                    None
                                }
                            },
                            None => None,
                        }
                    } else {
                        *self = s;
                        None
                    }
                } else {
                    None
                }
            }
        }
    }
}

pub fn morph_rect(a: &Rectangle, b: &Rectangle, p: f32) -> Rectangle {
    let q = 1.0 - p;
    Rectangle::from_tuples(
        (
            a.top_left().x * q + b.top_left().x * p,
            a.top_left().y * q + b.top_left().y * p,
        ),
        (
            a.bottom_right().x * q + b.bottom_right().x * p,
            a.bottom_right().y * q + b.bottom_right().y * p,
        ),
    )
}
