use std::{
    f32
};

use nalgebra::{vector, Vector2};

use yanyaengine::FontsContainer;

use crate::{
    client::{
        game_state::UiControls
    },
    common::{
        colors::Lcha,
        MixColorLch,
        EaseOut
    }
};

pub use crate::client::game_state::ui::controller::*;


pub const TITLE_PADDING: f32 = 15.0;
pub const TINY_PADDING: f32 = 5.0;
pub const TINY_SMALL_PADDING: f32 = 7.5;
pub const SMALL_PADDING: f32 = 10.0;
pub const MEDIUM_PADDING: f32 = 15.0;
pub const ITEM_PADDING: f32 = SMALL_PADDING;
pub const BODY_PADDING: f32 = 20.0;
pub const NOTIFICATION_PADDING: f32 = TINY_PADDING;

pub const BUTTON_SIZE: f32 = 40.0;
pub const SCROLLBAR_WIDTH: f32 = SMALL_PADDING;

pub const SEPARATOR_SIZE: f32 = 3.0;

pub const BIG_TEXT_SIZE: u32 = 30;
pub const MEDIUM_TEXT_SIZE: u32 = 25;
pub const SMALL_TEXT_SIZE: u32 = 20;
pub const SMALLEST_TEXT_SIZE: u32 = 15;

pub const WHITE_COLOR: Lcha = Lcha{l: 100.0, c: 0.0, h: 0.0, a: 1.0};
pub const GRAY_COLOR: Lcha = Lcha{l: 5.0, c: 0.0, h: 0.0, a: 1.0};
pub const BLACK_COLOR: Lcha = Lcha{l: 0.0, c: 0.0, h: 0.0, a: 1.0};

pub const BACKGROUND_COLOR: Lcha = Lcha{l: 94.0, c: 18.0, h: ACCENT_COLOR.h, a: 1.0};
pub const ACCENT_COLOR: Lcha = Lcha{l: 78.0, c: 42.8, h: 5.943, a: 1.0};
pub const ACCENT_COLOR_FADED: Lcha = Lcha{l: 90.0, c: 25.0, ..ACCENT_COLOR};

pub const SPECIAL_COLOR_ONE: Lcha = Lcha{h: ACCENT_COLOR.h - f32::consts::PI * 2.0 / 3.0, ..ACCENT_COLOR};
pub const SPECIAL_COLOR_TWO: Lcha = Lcha{l: 96.3, c: 81.2, h: ACCENT_COLOR.h + f32::consts::PI * 2.0 / 3.0, a: 1.0};

#[derive(Clone)]
pub struct TextboxInfo
{
    pub text: String,
    limit: Option<usize>,
    position: u32,
    animation: f32
}

impl TextboxInfo
{
    pub fn new(text: String) -> Self
    {
        Self::new_inner(text, None)
    }

    pub fn new_with_limit(text: String, limit: usize) -> Self
    {
        Self::new_inner(text, Some(limit))
    }

    fn new_inner(text: String, limit: Option<usize>) -> Self
    {
        Self{
            position: text.chars().count() as u32,
            text,
            limit,
            animation: 0.0
        }
    }

    pub fn update(&mut self, dt: f32)
    {
        self.animation = (self.animation + dt * 0.75).fract();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextboxPartId
{
    Body,
    Entry,
    Text,
    Line,
    CursorStart,
    Cursor
}

pub fn textbox_update<Id: Idable>(
    controls: &mut UiControls<Id>,
    fonts: &FontsContainer,
    id: fn(TextboxPartId) -> Id,
    body: TreeInserter<Id>,
    font_size: u32,
    info: &mut TextboxInfo
)
{
    let entry = body.update(id(TextboxPartId::Entry), UiElement{
        width: UiSize::FitChildren.into(),
        height: UiSize::FitChildren.into(),
        children_layout: UiLayout::Horizontal,
        ..Default::default()
    });

    let screen_size = entry.screen_size().max();

    let text_info = TextInfo::new_simple(font_size, info.text.clone());
    let text_width = fonts.calculate_bounds(&text_info, &Vector2::repeat(screen_size)).x;

    entry.update(id(TextboxPartId::Text), UiElement{
        texture: UiTexture::Text(text_info),
        mix: Some(MixColorLch::color(ACCENT_COLOR)),
        ..UiElement::fit_content()
    });

    let font_height = fonts.text_height(font_size, screen_size);
    entry.update(id(TextboxPartId::CursorStart), UiElement{
        width: 0.0.into(),
        height: font_height.into(),
        position: UiPosition::Inherit,
        ..Default::default()
    });

    if let Some(cursor_start) = entry.try_position()
    {
        let is_visible = info.animation < 0.5;

        let text_info = TextInfo::new_simple(
            font_size,
            info.text.chars().take(info.position as usize).collect::<String>()
        );

        let offset = if info.position == 0
        {
            Vector2::zeros()
        } else
        {
            fonts.calculate_bounds(&text_info, &Vector2::repeat(screen_size)) - vector![0.0, font_height]
        };

        let position = (cursor_start - vector![text_width * 0.5, 0.0]) + offset;

        entry.update(id(TextboxPartId::Cursor), UiElement{
            texture: UiTexture::Solid,
            mix: Some(MixColorLch::color(Lcha{a: if is_visible { 1.0 } else { 0.0 }, ..ACCENT_COLOR})),
            width: UiSize::Pixels(SEPARATOR_SIZE).into(),
            height: UiSize::Rest(1.0).into(),
            position: UiPosition::Absolute{position, align: UiPositionAlign::default()},
            ..Default::default()
        });
    }

    add_padding_vertical(body, UiSize::Pixels(5.0).into());

    body.update(id(TextboxPartId::Line), UiElement{
        texture: UiTexture::Solid,
        mix: Some(MixColorLch::color(ACCENT_COLOR)),
        width: UiSize::Rest(1.0).into(),
        height: UiSize::Pixels(SEPARATOR_SIZE).into(),
        ..Default::default()
    });

    if text_input_handle(controls, info.limit, &mut info.position, &mut info.text)
    {
        info.animation = 0.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiListPart
{
    BodyOuter,
    Body,
    Moving,
    Scrollbar,
    BarPad,
    Bar
}

pub struct UiListInfo<'a, Id>
{
    pub controls: &'a mut UiControls<Id>,
    pub mouse_taken: bool,
    pub item_height: f32,
    pub padding: f32,
    pub outer_width: UiElementSize<Id>,
    pub outer_height: UiElementSize<Id>,
    pub dt: f32
}

pub struct UiList<T>
{
    position: f32,
    target_position: f32,
    pub items: Vec<T>
}

impl<T> From<Vec<T>> for UiList<T>
{
    fn from(items: Vec<T>) -> Self
    {
        Self{
            position: 0.0,
            target_position: 0.0,
            items
        }
    }
}

impl<T> UiList<T>
{
    pub fn new() -> Self
    {
        Self::from(Vec::new())
    }

    pub fn update<Id: Idable>(
        &mut self,
        parent: TreeInserter<Id>,
        id: impl Fn(UiListPart) -> Id,
        mut info: UiListInfo<Id>,
        mut update_item: impl FnMut(&mut UiListInfo<Id>, TreeInserter<Id>, &T, bool)
    ) -> Option<usize>
    {
        assert!(parent.element().children_layout.is_horizontal());

        self.position = self.position.ease_out(self.target_position, 10.0, info.dt);

        let outer_body = parent.update(id(UiListPart::BodyOuter), UiElement{
            width: info.outer_width.clone(),
            height: info.outer_height.clone(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let body_id = id(UiListPart::Body);
        let body = outer_body.update(body_id.clone(), UiElement{
            width: info.outer_width.clone(),
            height: info.outer_height.clone(),
            scissor: true,
            ..Default::default()
        });

        let body_height = body.try_height()?;

        let item_height = info.item_height;

        let items_total = self.items.len() as f32 * item_height;
        let items_fit = (body_height / item_height).ceil() as usize + 2;

        let bottom_scroll = (items_total - body_height).max(0.0);
        let offset = bottom_scroll * self.position;

        let starting_item = (offset / item_height) as usize;

        let moving_offset = -body_height / 2.0 - offset;

        let moving_part = body.update(id(UiListPart::Moving), UiElement{
            position: UiPosition::Offset(body_id, Vector2::new(0.0, moving_offset)),
            children_layout: UiLayout::Vertical,
            width: info.outer_width.clone(),
            height: 0.0.into(),
            ..Default::default()
        });

        add_padding_vertical(moving_part, (offset - (offset % item_height) - item_height).max(0.0).into());

        let bar_height = (body_height / items_total).min(1.0);

        if bar_height < 1.0
        {
            let scrollbar_id = id(UiListPart::Scrollbar);
            let scrollbar = parent.update(scrollbar_id.clone(), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(ACCENT_COLOR_FADED)),
                width: UiSize::Pixels(SCROLLBAR_WIDTH).into(),
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            if let Some(value) = scrollbar_handle(
                info.controls,
                scrollbar,
                &scrollbar_id,
                bar_height,
                false,
                info.mouse_taken
            )
            {
                self.target_position = value;
            }

            scrollbar.update(id(UiListPart::BarPad), UiElement{
                height: UiSize::CopyElement(UiDirection::Vertical, (1.0 - bar_height) * self.position, scrollbar_id.clone()).into(),
                ..Default::default()
            });

            let bar = scrollbar.update(id(UiListPart::Bar), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::CopyElement(UiDirection::Vertical, bar_height, scrollbar_id.clone()).into(),
                animation: Animation::scrollbar_bar(),
                ..Default::default()
            });

            if (bar.is_mouse_inside() || info.controls.observe_action_held(&scrollbar_id)) && !info.mouse_taken
            {
                bar.element().mix.as_mut().unwrap().color = ACCENT_COLOR;
            }
        }

        let selected_index = if info.mouse_taken
        {
            None
        } else
        {
            body.mouse_position_inside().and_then(|position|
            {
                let fraction = offset % item_height;
                let index = starting_item + ((position.y * body_height + fraction) / item_height) as usize;

                (index < self.items.len()).then_some(index)
            })
        };

        self.items.iter().enumerate()
            .skip(starting_item.saturating_sub(1))
            .take(items_fit)
            .for_each(|(index, value)|
            {
                let is_selected = selected_index.map(|x| x == index).unwrap_or(false);
                update_item(&mut info, moving_part, value, is_selected);
            });

        add_padding_vertical(outer_body, UiSize::Pixels(info.padding).into());

        selected_index
    }
}
