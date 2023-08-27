use std::rc::Rc;

use speedy2d::{
    color::Color,
    dimen::Vec2,
    font::{FormattedTextBlock, TextLayout, TextOptions},
    shape::Rectangle,
    window::{ModifiersState, MouseButton},
};

use crate::gui::{GuiAction, GuiElem, GuiElemCfg, GuiElemTrait};

/*

Some basic structs to use everywhere,
except they are all text-related.

*/

#[derive(Clone)]
pub struct Label {
    config: GuiElemCfg,
    children: Vec<GuiElem>,
    pub content: Content,
    pub pos: Vec2,
}
#[derive(Clone)]
pub struct Content {
    text: String,
    color: Color,
    background: Option<Color>,
    formatted: Option<Rc<FormattedTextBlock>>,
}
impl Content {
    pub fn get_text(&self) -> &String {
        &self.text
    }
    pub fn get_color(&self) -> &Color {
        &self.color
    }
    /// causes text layout reset
    pub fn text(&mut self) -> &mut String {
        self.formatted = None;
        &mut self.text
    }
    pub fn color(&mut self) -> &mut Color {
        &mut self.color
    }
}
impl Label {
    pub fn new(
        config: GuiElemCfg,
        text: String,
        color: Color,
        background: Option<Color>,
        pos: Vec2,
    ) -> Self {
        Self {
            config,
            children: vec![],
            content: Content {
                text,
                color,
                background,
                formatted: None,
            },
            pos,
        }
    }
}
impl GuiElemTrait for Label {
    fn config(&self) -> &GuiElemCfg {
        &self.config
    }
    fn config_mut(&mut self) -> &mut GuiElemCfg {
        &mut self.config
    }
    fn children(&mut self) -> Box<dyn Iterator<Item = &mut GuiElem> + '_> {
        Box::new(self.children.iter_mut())
    }
    fn any(&self) -> &dyn std::any::Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn clone_gui(&self) -> Box<dyn GuiElemTrait> {
        Box::new(self.clone())
    }
    fn draw(&mut self, info: &mut crate::gui::DrawInfo, g: &mut speedy2d::Graphics2D) {
        if self.config.pixel_pos.size() != info.pos.size() {
            // resize
            self.content.formatted = None;
        }
        let text = if let Some(text) = &self.content.formatted {
            text
        } else {
            let l = info
                .font
                .layout_text(&self.content.text, 1.0, TextOptions::new());
            let l = info.font.layout_text(
                &self.content.text,
                (info.pos.width() / l.width()).min(info.pos.height() / l.height()),
                TextOptions::new(),
            );
            self.content.formatted = Some(l);
            self.content.formatted.as_ref().unwrap()
        };
        let top_left = Vec2::new(
            info.pos.top_left().x + self.pos.x * (info.pos.width() - text.width()),
            info.pos.top_left().y + self.pos.y * (info.pos.height() - text.height()),
        );
        if let Some(bg) = self.content.background {
            g.draw_rectangle(
                Rectangle::new(
                    top_left,
                    Vec2::new(top_left.x + text.width(), top_left.y + text.height()),
                ),
                bg,
            );
        }
        g.draw_text(top_left, self.content.color, text);
    }
}

// TODO! this, but requires keyboard events first

/// a single-line text fields for users to type text into.
#[derive(Clone)]
pub struct TextField {
    config: GuiElemCfg,
    pub children: Vec<GuiElem>,
}
impl TextField {
    pub fn new(config: GuiElemCfg, hint: String, color_hint: Color, color_input: Color) -> Self {
        Self {
            config: config.w_mouse().w_keyboard_focus(),
            children: vec![
                GuiElem::new(Label::new(
                    GuiElemCfg::default(),
                    String::new(),
                    color_input,
                    None,
                    Vec2::new(0.0, 0.5),
                )),
                GuiElem::new(Label::new(
                    GuiElemCfg::default(),
                    hint,
                    color_hint,
                    None,
                    Vec2::new(0.5, 0.5),
                )),
            ],
        }
    }
}
impl GuiElemTrait for TextField {
    fn config(&self) -> &GuiElemCfg {
        &self.config
    }
    fn config_mut(&mut self) -> &mut GuiElemCfg {
        &mut self.config
    }
    fn children(&mut self) -> Box<dyn Iterator<Item = &mut GuiElem> + '_> {
        Box::new(self.children.iter_mut())
    }
    fn any(&self) -> &dyn std::any::Any {
        self
    }
    fn any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn clone_gui(&self) -> Box<dyn GuiElemTrait> {
        Box::new(self.clone())
    }
    fn draw(&mut self, info: &mut crate::gui::DrawInfo, g: &mut speedy2d::Graphics2D) {
        let (t, c) = if info.has_keyboard_focus {
            (3.0, Color::WHITE)
        } else {
            (1.0, Color::GRAY)
        };
        g.draw_line(info.pos.top_left(), info.pos.top_right(), t, c);
        g.draw_line(info.pos.bottom_left(), info.pos.bottom_right(), t, c);
        g.draw_line(info.pos.top_left(), info.pos.bottom_left(), t, c);
        g.draw_line(info.pos.top_right(), info.pos.bottom_right(), t, c);
    }
    fn mouse_pressed(&mut self, button: MouseButton) -> Vec<GuiAction> {
        self.config.request_keyboard_focus = true;
        vec![GuiAction::ResetKeyboardFocus]
    }
    fn char_focus(&mut self, modifiers: ModifiersState, key: char) -> Vec<GuiAction> {
        if !(modifiers.ctrl() || modifiers.alt() || modifiers.logo()) && !key.is_control() {
            let content = &mut self.children[0].try_as_mut::<Label>().unwrap().content;
            let was_empty = content.get_text().is_empty();
            content.text().push(key);
            if was_empty {
                self.children[1].inner.config_mut().enabled = false;
            }
        }
        vec![]
    }
    fn key_focus(
        &mut self,
        modifiers: ModifiersState,
        down: bool,
        key: Option<speedy2d::window::VirtualKeyCode>,
        _scan: speedy2d::window::KeyScancode,
    ) -> Vec<GuiAction> {
        if down
            && !(modifiers.alt() || modifiers.logo())
            && key == Some(speedy2d::window::VirtualKeyCode::Backspace)
        {
            let content = &mut self.children[0].try_as_mut::<Label>().unwrap().content;
            if !content.get_text().is_empty() {
                if modifiers.ctrl() {
                    for s in [true, false, true] {
                        while !content.get_text().is_empty()
                            && content.get_text().ends_with(' ') == s
                        {
                            content.text().pop();
                        }
                    }
                } else {
                    content.text().pop();
                }
                if content.get_text().is_empty() {
                    self.children[1].inner.config_mut().enabled = true;
                }
            }
        }
        vec![]
    }
}
