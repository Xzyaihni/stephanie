use std::{
    f32,
    fmt::{self, Display},
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
    cell::RefCell,
    sync::Arc
};

use parking_lot::Mutex;

use image::RgbImage;

use nalgebra::Vector2;

use yanyaengine::{
    FontsContainer,
    Assets,
    game_object::*
};

use crate::{
    debug_config::*,
    client::{
        SlicedTexture,
        game_state::{
            GameState,
            UiAnatomyLocations,
            UiControls,
            UiEvent,
            GameUiEvent,
            UiReceiver
        }
    },
    common::{
        some_or_return,
        some_or_value,
        lerp,
        render_info::*,
        anatomy::*,
        colors::*,
        lazy_transform::*,
        f32_to_range,
        Side1d,
        EaseOut,
        Sprite,
        Item,
        ItemRarity,
        ItemId,
        InventoryItem,
        InventorySorter,
        Entity,
        ItemInfo,
        ItemsInfo,
        OnChangeInfo,
        entity::ClientEntities,
        world::{TILE_SIZE, TilePos}
    }
};

use element::*;
use controller::*;

pub mod element;
pub mod controller;


const TITLE_PADDING: f32 = 15.0;
const TINY_PADDING: f32 = 5.0;
const TINY_SMALL_PADDING: f32 = 7.5;
const SMALL_PADDING: f32 = 10.0;
const MEDIUM_PADDING: f32 = 15.0;
const ITEM_PADDING: f32 = SMALL_PADDING;
const BODY_PADDING: f32 = 20.0;
const NOTIFICATION_PADDING: f32 = TINY_PADDING;

const BUTTON_SIZE: f32 = 40.0;
const SCROLLBAR_WIDTH: f32 = SMALL_PADDING;
const SCROLLBAR_HEIGHT: f32 = BUTTON_SIZE * 5.0;

const SEPARATOR_SIZE: f32 = 3.0;

const BIG_TEXT_SIZE: u32 = 30;
const MEDIUM_TEXT_SIZE: u32 = 25;
const SMALL_TEXT_SIZE: u32 = 20;
const SMALLEST_TEXT_SIZE: u32 = 15;

pub const WHITE_COLOR: Lcha = Lcha{l: 100.0, c: 0.0, h: 0.0, a: 1.0};
pub const GRAY_COLOR: Lcha = Lcha{l: 5.0, c: 0.0, h: 0.0, a: 1.0};
pub const BLACK_COLOR: Lcha = Lcha{l: 0.0, c: 0.0, h: 0.0, a: 1.0};

pub const BACKGROUND_COLOR: Lcha = Lcha{l: 94.0, c: 18.0, h: ACCENT_COLOR.h, a: 1.0};
pub const ACCENT_COLOR: Lcha = Lcha{l: 78.0, c: 42.8, h: 5.943, a: 1.0};
pub const ACCENT_COLOR_FADED: Lcha = Lcha{l: 90.0, c: 25.0, ..ACCENT_COLOR};

pub const SPECIAL_COLOR: Lcha = Lcha{h: ACCENT_COLOR.h - f32::consts::PI, ..ACCENT_COLOR};

const MISSING_PART_COLOR: Lcha = Lcha{l: 50.0, a: 0.3, ..BLACK_COLOR};


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UiId
{
    Screen,
    Loading(LoadingPart),
    Paused(PausedPart),
    Health(HealthPart),
    DeathScreen(DeathScreenPart),
    Fade,
    Padding(u32),
    Console(ConsolePart),
    SeenNotification(Entity, SeenNotificationPart),
    Notification(Entity, NotificationKindInfo, NotificationPart),
    AnatomyNotification(Entity, AnatomyNotificationPart),
    Popup(u8, PopupPart),
    Window(UiIdWindow, WindowPart),
    BarsBody,
    BarsBodyInner,
    BarDisplay(BarDisplayKind, BarDisplayPart)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeathScreenPart
{
    Panel,
    Body,
    Text,
    InfoText,
    Button,
    ButtonText
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HealthPart
{
    OuterPanel,
    InnerPanel,
    PanelVertical,
    Panel,
    Body,
    Outline,
    Anatomy(ChangedPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PausedPart
{
    Cover,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadingPart
{
    Cover,
    Body,
    Text,
    BarBody,
    Bar
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeenNotificationPart
{
    Body,
    Clip,
    ClipBody,
    Back,
    Fill
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnatomyNotificationPart
{
    Body,
    Part(ChangedPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarDisplayKind
{
    Stamina,
    Cooldown
}

impl BarDisplayKind
{
    fn name(self) -> String
    {
        match self
        {
            Self::Stamina => "STAMINA".to_owned(),
            Self::Cooldown => "COOLDOWN".to_owned()
        }
    }
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarDisplayPart
{
    Body,
    Bar,
    BarFill,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsolePart
{
    Body,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationPart
{
    Icon,
    Body,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupPart
{
    Body,
    Button(u32, PopupButtonPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupButtonPart
{
    Body,
    Text,
    Separator
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WindowPart
{
    Panel,
    Body,
    Separator(SeparatorPart),
    Title(TitlePart),
    Inventory(InventoryPart),
    ItemInfo(ItemInfoPart),
    Stats,
    Anatomy(AnatomyPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeparatorPart
{
    Outer,
    Inner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnatomyPart
{
    BodyPart(ChangedPart),
    Tooltip(AnatomyTooltipPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemInfoPart
{
    Text,
    ImageBody,
    Image
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnatomyTooltipPart
{
    Panel,
    Title,
    Name,
    Body,
    Separator,
    Healthbar(BarId, BarDisplayPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarId
{
    Health,
    Brain(Side1d, BrainId)
}

impl Display for BarId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Health => write!(f, "HEALTH"),
            Self::Brain(_side, id) => write!(f, "{id}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InventoryPart
{
    Body,
    List(UiListPart),
    Item(InventoryItem, ItemPart),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ItemPart
{
    Indicator,
    Body,
    Icon(IconPart),
    Name
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconPart
{
    Body,
    Picture
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TitlePart
{
    Body,
    Text,
    Button(UiIdTitleButton)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UiListPart
{
    BodyOuter,
    Body,
    Moving,
    Scrollbar,
    BarPad,
    Bar
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiIdWindow
{
    Inventory(Entity),
    ItemInfo(ItemId),
    Stats(Entity),
    Anatomy(Entity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiIdTitleButton
{
    Anatomy(UiIdButtonPart),
    Stats(UiIdButtonPart),
    Close(UiIdButtonPart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiIdButtonPart
{
    Body,
    Icon
}

type UiController = Controller<UiId>;
type UiParentElement<'a> = TreeInserter<'a, UiId>;

#[derive(Debug, Clone, Copy)]
pub enum NotificationDoor
{
    Open,
    Close(bool)
}

impl Hash for NotificationDoor
{
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

impl PartialEq for NotificationDoor
{
    fn eq(&self, _other: &Self) -> bool
    {
        true
    }
}

impl Eq for NotificationDoor {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationIcon
{
    GoUp,
    GoDown,
    Door(NotificationDoor)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NotificationKindInfo
{
    Text{icon: NotificationIcon, text: String}
}

#[derive(Debug, Clone)]
pub struct NotificationInfo
{
    pub owner: Entity,
    pub lifetime: f32,
    pub kind: NotificationKindInfo,
    pub is_closed: bool
}

impl Hash for NotificationInfo
{
    fn hash<H: Hasher>(&self, state: &mut H)
    {
        self.owner.hash(state);
        self.kind.hash(state);
    }
}

pub type InventoryOnClick = Box<dyn FnMut(&Item, &ItemInfo, InventoryItem) -> Vec<GameUiEvent>>;

struct ButtonResult
{
    inside: bool,
    clicked: bool
}

fn handle_button(
    info: &mut UpdateInfo,
    button: UiParentElement
) -> ButtonResult
{
    if info.mouse_taken
    {
        return ButtonResult{inside: false, clicked: false};
    }

    let inside = button.is_mouse_inside();

    if inside
    {
        button.element().mix.as_mut().unwrap().color = ACCENT_COLOR;
    }

    ButtonResult{inside, clicked: inside && info.controls.take_click_down()}
}

fn draw_item_image(
    parent: UiParentElement,
    sprite: Sprite,
    id: UiId,
    inner_id: UiId,
    size: UiElementSize<UiId>
)
{
    let aspect = sprite.aspect();

    let image = parent.update(id.clone(), UiElement{
        width: size.clone(),
        height: size,
        children_layout: if aspect.x == 1.0 { UiLayout::Horizontal } else { UiLayout::Vertical },
        ..Default::default()
    });

    image.update(inner_id, UiElement{
        texture: UiTexture::CustomId(sprite.id),
        width: UiSize::CopyElement(UiDirection::Horizontal, aspect.x, id.clone()).into(),
        height: UiSize::CopyElement(UiDirection::Vertical, aspect.y, id).into(),
        ..Default::default()
    });
}

fn single_health_color(fraction: Option<f32>) -> Lcha
{
    fraction.map(|x|
    {
        let range = 0.8..=2.27;
        let h = f32_to_range(range, x);

        Lcha{l: 80.0, c: 100.0, h, a: 1.0}
    }).unwrap_or(MISSING_PART_COLOR)
}

fn health_color(anatomy: &Anatomy, id: ChangedPart) -> Lcha
{
    let health = anatomy.as_human().unwrap().get_health(id).and_then(|health|
    {
        (health > 0.0).then_some(health)
    });

    single_health_color(health)
}

pub struct UiList<T>
{
    position: f32,
    target_position: f32,
    items: Vec<T>
}

impl<T> UiList<T>
{
    fn new() -> Self
    {
        Self{
            position: 0.0,
            target_position: 0.0,
            items: Vec::new()
        }
    }

    fn update(
        &mut self,
        info: &mut UpdateInfo,
        parent: UiParentElement,
        id: impl Fn(UiListPart) -> UiId,
        item_height: f32,
        padding: f32,
        mut update_item: impl FnMut(&mut UpdateInfo, UiParentElement, &T, bool)
    ) -> Option<usize>
    {
        assert!(parent.element().children_layout.is_horizontal());

        self.position = self.position.ease_out(self.target_position, 10.0, info.dt);

        let outer_body = parent.update(id(UiListPart::BodyOuter), UiElement{
            width: UiSize::Rest(1.0).into(),
            height: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let body_id = id(UiListPart::Body);
        let body = outer_body.update(body_id.clone(), UiElement{
            width: UiSize::Rest(1.0).into(),
            height: UiSize::Rest(1.0).into(),
            scissor: true,
            ..Default::default()
        });

        let body_height = body.try_height()?;

        let items_total = self.items.len() as f32 * item_height;
        let items_fit = (body_height / item_height).ceil() as usize + 2;

        let bottom_scroll = (items_total - body_height).max(0.0);
        let offset = bottom_scroll * self.position;

        let starting_item = (offset / item_height) as usize;

        let moving_offset = -body_height / 2.0 - offset;

        let moving_part = body.update(id(UiListPart::Moving), UiElement{
            position: UiPosition::Offset(body_id, Vector2::new(0.0, moving_offset)),
            children_layout: UiLayout::Vertical,
            width: UiSize::Rest(1.0).into(),
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
                update_item(info, moving_part, value, is_selected);
            });

        add_padding_vertical(outer_body, UiSize::Pixels(padding).into());

        selected_index
    }
}

struct UiInventoryItem
{
    item: InventoryItem,
    name: String,
    rarity: ItemRarity,
    texture: Sprite
}

struct UiTitleButton
{
    id: fn(UiIdButtonPart) -> UiIdTitleButton,
    texture: String,
    action: Rc<dyn Fn(&mut GameState)>
}

pub struct UiInventory
{
    sorter: InventorySorter,
    entity: Entity,
    on_click: InventoryOnClick,
    list: UiList<UiInventoryItem>,
    buttons: Vec<UiTitleButton>,
    needs_update: bool
}

impl UiInventory
{
    fn items(&self, info: &UpdateInfo) -> Vec<UiInventoryItem>
    {
        let inventory = some_or_return!(info.entities.inventory(self.entity));

        let mut items: Vec<_> = inventory.items_ids().collect();
        items.sort_by(|a, b|
        {
            self.sorter.order(info.items_info, a.1, b.1)
        });

        items.into_iter().map(|(index, x)|
        {
            let item = info.items_info.get(x.id);
            UiInventoryItem{
                item: index,
                name: item.name.clone(),
                rarity: x.rarity,
                texture: item.texture
            }
        }).collect()
    }

    fn update_items(&mut self, info: &UpdateInfo)
    {
        if self.needs_update
        {
            let entity = self.entity;

            self.buttons = Vec::new();
            if info.entities.anatomy(self.entity).is_some()
            {
                self.buttons.push(UiTitleButton{
                    id: UiIdTitleButton::Anatomy,
                    texture: "ui/anatomy_button.png".to_owned(),
                    action: Rc::new(move |game_state|
                    {
                        game_state.ui.borrow_mut().create_window(WindowKind::Anatomy(entity));
                    })
                });
            }

            if info.entities.player(self.entity).is_some()
            {
                self.buttons.push(UiTitleButton{
                    id: UiIdTitleButton::Stats,
                    texture: "ui/stats_button.png".to_owned(),
                    action: Rc::new(move |game_state|
                    {
                        game_state.ui.borrow_mut().create_window(WindowKind::Stats(entity));
                    })
                });
            }

            self.list.items = self.items(info);
            self.needs_update = false;
        }
    }
}

enum WindowKind
{
    Inventory(UiInventory),
    ItemInfo(Item),
    Stats(Entity),
    Anatomy(Entity)
}

impl WindowKind
{
    fn update(&mut self, parent: UiParentElement, info: &mut UpdateInfo)
    {
        let this_window_id = self.as_id();
        let window_id = this_window_id;

        fn with_titlebar<'a, 'b>(
            window_id: UiIdWindow,
            parent: UiParentElement<'a>,
            info: &'b mut UpdateInfo,
            title: String,
            prepad: bool,
            buttons: &[UiTitleButton]
        ) -> UiParentElement<'a>
        {
            let id = move |part|
            {
                UiId::Window(window_id, part)
            };

            let titlebar_id = id(WindowPart::Title(TitlePart::Body));
            let titlebar = parent.update(titlebar_id.clone(), UiElement{
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::FitChildren),
                    size: UiSize::Rest(1.0)
                },
                ..Default::default()
            });

            if info.controls.observe_action_held(&titlebar_id)
            {
                info.dragging_currently = true;
            }

            let top_left_rounded = info.sliced_textures["top_left_rounded"];
            let top_right_rounded = info.sliced_textures["top_right_rounded"];

            let mut update_button = |button_id: fn(UiIdButtonPart) -> UiIdTitleButton, background, texture: String, action|
            {
                let id = |part| id(WindowPart::Title(TitlePart::Button(button_id(part))));

                let size: UiElementSize<UiId> = UiSize::Pixels(BUTTON_SIZE).into();

                let animation = Animation{
                    mix: Some(MixAnimation::default()),
                    ..Default::default()
                };

                let button = titlebar.update(id(UiIdButtonPart::Body), UiElement{
                    texture: background,
                    mix: Some(MixColorLch::color(Lcha{a: 0.0, ..ACCENT_COLOR})),
                    animation: animation.clone(),
                    width: size.clone(),
                    height: size,
                    ..Default::default()
                });

                let icon = button.update(id(UiIdButtonPart::Icon), UiElement{
                    texture: UiTexture::Custom(texture.into()),
                    mix: Some(MixColorLch::color(ACCENT_COLOR)),
                    animation,
                    width: UiSize::Rest(1.0).into(),
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                let result = handle_button(info, button);

                if result.inside
                {
                    icon.element().mix = Some(MixColorLch::color(BACKGROUND_COLOR));
                }

                if result.clicked
                {
                    info.user_receiver.push(UiEvent::Action(action));
                }
            };

            buttons.iter().enumerate().for_each(|(index, button)|
            {
                let background = if index == 0
                {
                    UiTexture::Sliced(top_left_rounded)
                } else
                {
                    UiTexture::Solid
                };

                update_button(
                    button.id.clone(),
                    background,
                    button.texture.clone(),
                    button.action.clone()
                );
            });

            let padding_size = UiElementSize{
                minimum_size: Some(UiMinimumSize::Pixels(TITLE_PADDING)),
                size: UiSize::Rest(1.0)
            };

            add_padding_horizontal(titlebar, padding_size.clone());
            titlebar.update(id(WindowPart::Title(TitlePart::Text)), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(25, title)),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                ..UiElement::fit_content()
            });
            add_padding_horizontal(titlebar, padding_size);

            update_button(
                |part| UiIdTitleButton::Close(part),
                UiTexture::Sliced(top_right_rounded),
                "ui/close_button.png".to_owned(),
                Rc::new(move |game_state|
                {
                    game_state.ui.borrow_mut().remove_window(&window_id);
                })
            );

            add_padding_vertical(parent, UiSize::Pixels(TINY_PADDING).into());

            let outer_separator = parent.update(id(WindowPart::Separator(SeparatorPart::Outer)), UiElement{
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Pixels(SEPARATOR_SIZE).into(),
                ..Default::default()
            });

            add_padding_horizontal(outer_separator, UiSize::Pixels(MEDIUM_PADDING).into());
            outer_separator.update(id(WindowPart::Separator(SeparatorPart::Inner)), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Rest(1.0).into(),
                animation: Animation::separator_wide(),
                ..Default::default()
            });
            add_padding_horizontal(outer_separator, UiSize::Pixels(MEDIUM_PADDING).into());

            add_padding_vertical(parent, UiSize::Pixels(TINY_PADDING).into());

            let body = parent.update(id(WindowPart::Body), UiElement{
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::FitChildren),
                    size: UiSize::Rest(1.0)
                },
                ..Default::default()
            });

            if prepad { add_padding_vertical(parent, UiSize::Pixels(TINY_SMALL_PADDING).into()); }

            body
        }

        let with_titlebar = move |parent, info, title, prepad, buttons|
        {
            with_titlebar(window_id, parent, info, title, prepad, buttons)
        };

        let name_of = |entity|
        {
            info.entities.named(entity).as_deref().cloned()
                .unwrap_or_else(|| "unnamed".to_owned())
        };

        let close_this = move |info: &mut UpdateInfo|
        {
            info.user_receiver.push(UiEvent::Action(Rc::new(move |game_state|
            {
                game_state.ui.borrow_mut().remove_window(&window_id);
            })));
        };

        match self
        {
            Self::Inventory(inventory) =>
            {
                let id = move |part|
                {
                    UiId::Window(window_id, WindowPart::Inventory(part))
                };

                let name = name_of(inventory.entity);

                let body = with_titlebar(parent, info, name, false, &inventory.buttons);

                body.element().height = UiSize::Pixels(SCROLLBAR_HEIGHT).into();

                let body = body.update(id(InventoryPart::Body), UiElement{
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Pixels(SCROLLBAR_HEIGHT * 2.0)
                    },
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                inventory.update_items(info);

                let font_size = SMALL_TEXT_SIZE;
                let item_height = info.fonts.text_height(font_size, body.screen_size().max());

                let picked_item = info.popup.as_ref().and_then(|x| (x.owner == inventory.entity).then_some(x.item));

                let selected = inventory.list.update(info, body, |list_part|
                {
                    id(InventoryPart::List(list_part))
                }, item_height, SMALL_PADDING, |_info, parent, item, is_selected|
                {
                    let is_picked = picked_item == Some(item.item);

                    let id = |part|
                    {
                        id(InventoryPart::Item(item.item, part))
                    };

                    let rarity_hue_chroma = item.rarity.hue_chroma();

                    let rarity_color = rarity_hue_chroma.map(|(h, c)| Lcha{l: 80.0, c, h, a: 1.0});

                    let colors_inverted = is_picked || is_selected;

                    let body_color = Lcha{
                        a: if colors_inverted { 1.0 } else { 0.0 },
                        ..ACCENT_COLOR
                    };

                    let text_color = if colors_inverted { BACKGROUND_COLOR } else { ACCENT_COLOR };

                    let body = parent.update(id(ItemPart::Body), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColorLch::color(body_color)),
                        width: UiSize::Rest(1.0).into(),
                        animation: Animation{
                            mix: Animation::button().mix,
                            scaling: Some(ScalingAnimation{
                                close_scaling: Vector2::new(0.0, 1.0),
                                close_mode: Scaling::EaseOut{decay: 20.0},
                                ..Default::default()
                            }),
                            position: Some(PositionAnimation{
                                start_mode: Connection::EaseOut{decay: 20.0, limit: None},
                                parent_relative: true,
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                    if let Some(color) = rarity_color
                    {
                        let half_padding = ITEM_PADDING * 0.5;

                        body.update(id(ItemPart::Indicator), UiElement{
                            texture: UiTexture::Solid,
                            mix: Some(MixColorLch::color(color)),
                            width: UiSize::Pixels(half_padding).into(),
                            height: UiSize::Rest(1.0).into(),
                            ..Default::default()
                        });

                        add_padding_horizontal(body, UiSize::Pixels(half_padding).into());
                    } else
                    {
                        add_padding_horizontal(body, UiSize::Pixels(ITEM_PADDING).into());
                    }

                    let icon_size = item_height * 0.9;

                    draw_item_image(
                        body,
                        item.texture,
                        id(ItemPart::Icon(IconPart::Body)),
                        id(ItemPart::Icon(IconPart::Picture)),
                        icon_size.into()
                    );

                    add_padding_horizontal(body, UiSize::Pixels(ITEM_PADDING / 2.0).into());

                    body.update(id(ItemPart::Name), UiElement{
                        texture: UiTexture::Text(TextInfo::new_simple(font_size, item.name.clone())),
                        mix: Some(MixColorLch::color(text_color)),
                        animation: Animation{
                            mix: Some(MixAnimation::default()),
                            ..Default::default()
                        },
                        ..UiElement::fit_content()
                    });
                });

                if let Some(index) = selected
                {
                    if info.controls.take_click_down()
                    {
                        let item_id = inventory.list.items[index].item;
                        let entity = inventory.entity;

                        if let Some(items_inventory) = info.entities.inventory(entity)
                        {
                            if let Some(item) = items_inventory.get(item_id)
                            {
                                let events = (inventory.on_click)(item, info.items_info.get(item.id), item_id);
                                let event = UiEvent::Action(Rc::new(move |game_state|
                                {
                                    game_state.ui.borrow_mut().create_popup(entity, item_id, events.clone());
                                }));

                                info.user_receiver.push(event);
                            }
                        }
                    }
                }
            },
            Self::ItemInfo(item) =>
            {
                let item_info = info.items_info.get(item.id);

                let title = format!("info about - {}", item_info.name);
                let body = with_titlebar(parent, info, title, true, &[]);

                let id = move |part|
                {
                    UiId::Window(window_id, WindowPart::ItemInfo(part))
                };

                let mut description = format!("{} weighs around {} kg", item_info.name, item_info.mass);

                description += &format!("\nand is about {} meters in size!", item_info.scale_scalar());

                if let Some(rarity_name) = item.rarity.name()
                {
                    description += &format!("\nit has {rarity_name} rarity which gives it these buffs:");
                }

                item.buffs.iter().for_each(|buff|
                {
                    description += &format!("\n{buff}");
                });

                add_padding_horizontal(body, UiSize::Pixels(BODY_PADDING).into());

                let size: UiElementSize<_> = UiSize::Pixels(64.0).into();

                draw_item_image(
                    body,
                    item_info.texture,
                    id(ItemInfoPart::ImageBody),
                    id(ItemInfoPart::Image),
                    size
                );

                add_padding_horizontal(body, UiSize::Pixels(BODY_PADDING).into());

                body.update(id(ItemInfoPart::Text), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(SMALL_TEXT_SIZE, description)),
                    mix: Some(MixColorLch::color(ACCENT_COLOR)),
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(body, UiSize::Pixels(BODY_PADDING).into());
            },
            Self::Stats(owner) =>
            {
                let title = name_of(*owner);
                let body = with_titlebar(parent, info, title, true, &[]);

                let player = if let Some(x) = info.entities.player(*owner)
                {
                    x
                } else
                {
                    close_this(info);
                    return;
                };

                let kills = player.kills;
                let kills_text = format!("kills: {kills}");

                add_padding_horizontal(body, UiSize::Pixels(BODY_PADDING).into());

                body.update(UiId::Window(window_id, WindowPart::Stats), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(SMALL_TEXT_SIZE, kills_text)),
                    mix: Some(MixColorLch::color(ACCENT_COLOR)),
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(body, UiSize::Pixels(BODY_PADDING).into());
            },
            Self::Anatomy(owner) =>
            {
                let title = name_of(*owner);
                let body = with_titlebar(parent, info, title, true, &[]);
                body.element().children_layout = UiLayout::Vertical;

                let id = move |part|
                {
                    UiId::Window(window_id, WindowPart::Anatomy(part))
                };

                let anatomy = if let Some(x) = info.entities.anatomy(*owner)
                {
                    x
                } else
                {
                    close_this(info);
                    return;
                };

                let selected_index = info.anatomy_locations.locations.iter().rposition(|(part_id, location)|
                {
                    let id = id(AnatomyPart::BodyPart(*part_id));
                    body.input_of(&id).mouse_position_inside().map(|position|
                    {
                        location.mask.is_inside(position)
                    }).unwrap_or(false)
                });

                info.anatomy_locations.locations.iter().enumerate().for_each(|(index, (part_id, location))|
                {
                    let selected = selected_index == Some(index);

                    let color = health_color(&anatomy, *part_id);
                    let health_color = if selected
                    {
                        color.with_added_lightness(20.0).with_added_chroma(-30.0)
                    } else
                    {
                        color
                    };

                    let twice_size: UiElementSize<_> = UiSize::FitContent(2.0).into();
                    body.update(id(AnatomyPart::BodyPart(*part_id)), UiElement{
                        texture: UiTexture::CustomId(location.id),
                        mix: Some(MixColorLch::color(health_color)),
                        position: UiPosition::Inherit,
                        width: twice_size.clone(),
                        height: twice_size,
                        animation: Animation{
                            mix: Some(MixAnimation{
                                decay: MixDecay{l: 50.0, c: 50.0, ..MixDecay::all(20.0)},
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                });

                if let Some(part_id) = selected_index.map(|x| info.anatomy_locations.locations[x].0)
                {
                    let id = |part|
                    {
                        id(AnatomyPart::Tooltip(part))
                    };

                    let body = body.update(id(AnatomyTooltipPart::Panel), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColorLch::color(Lcha{a: 0.5, ..BACKGROUND_COLOR})),
                        position: UiPosition::Absolute{position: info.mouse_position, align: UiPositionAlign{
                            horizontal: AlignHorizontal::Left,
                            vertical: AlignVertical::Top
                        }},
                        animation: Animation::tooltip(),
                        children_layout: UiLayout::Vertical,
                        ..Default::default()
                    });

                    let title = body.update(id(AnatomyTooltipPart::Title), UiElement{
                        width: UiElementSize{
                            minimum_size: Some(UiMinimumSize::FitChildren),
                            size: UiSize::Rest(1.0)
                        },
                        children_layout: UiLayout::Vertical,
                        ..Default::default()
                    });

                    title.update(id(AnatomyTooltipPart::Name), UiElement{
                        texture: UiTexture::Text(TextInfo::new_simple(SMALL_TEXT_SIZE, part_id.to_string())),
                        mix: Some(MixColorLch::color(ACCENT_COLOR)),
                        ..UiElement::fit_content()
                    });

                    body.update(id(AnatomyTooltipPart::Separator), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColorLch::color(ACCENT_COLOR)),
                        width: UiElementSize{
                            minimum_size: Some(UiMinimumSize::FitChildren),
                            size: UiSize::Rest(1.0)
                        },
                        height: UiSize::Pixels(SEPARATOR_SIZE).into(),
                        animation: Animation::separator_wide(),
                        ..Default::default()
                    });

                    let body = body.update(id(AnatomyTooltipPart::Body), UiElement{
                        width: UiElementSize{
                            minimum_size: Some(UiMinimumSize::FitChildren),
                            size: UiSize::Rest(1.0)
                        },
                        children_layout: UiLayout::Vertical,
                        ..Default::default()
                    });

                    let draw_separator = ||
                    {
                        add_padding_vertical(body, UiSize::Pixels(2.0).into());
                    };

                    let draw_bar = |bar_id, this_id|
                    {
                        let id = |part|
                        {
                            id(AnatomyTooltipPart::Healthbar(bar_id, part))
                        };

                        add_padding_horizontal(body, UiSize::Pixels(200.0).into());

                        let body = body.update(id(BarDisplayPart::Body), UiElement{
                            texture: UiTexture::Solid,
                            mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                            width: UiElementSize{
                                minimum_size: Some(UiMinimumSize::FitChildren),
                                size: UiSize::Rest(1.0)
                            },
                            children_layout: UiLayout::Vertical,
                            ..Default::default()
                        });

                        let text_id = id(BarDisplayPart::Text);

                        let bar_body = body.update(id(BarDisplayPart::Bar), UiElement{
                            width: UiSize::Rest(1.0).into(),
                            height: UiSize::CopyElement(UiDirection::Vertical, 1.0, text_id.clone()).into(),
                            ..Default::default()
                        });

                        let value: Option<f32> = anatomy.as_human().unwrap().get_health(this_id);

                        let health_color = single_health_color(value);

                        let value = value.unwrap_or(0.0);

                        bar_body.update(id(BarDisplayPart::BarFill), UiElement{
                            texture: UiTexture::Solid,
                            mix: Some(MixColorLch::color(health_color)),
                            width: UiSize::Rest(value).into(),
                            height: UiSize::Rest(1.0).into(),
                            ..Default::default()
                        });

                        add_padding_horizontal(bar_body, UiSize::Rest(1.0 - value).into());

                        body.update(text_id, UiElement{
                            texture: UiTexture::Text(TextInfo::new_simple(SMALLEST_TEXT_SIZE, bar_id.to_string())),
                            mix: Some(MixColorLch::color(WHITE_COLOR)),
                            position: UiPosition::Inherit,
                            ..UiElement::fit_content()
                        });
                    };

                    draw_separator();
                    match part_id
                    {
                        ChangedPart::Organ(OrganId::Brain(side, _)) =>
                        {
                            BrainId::iter().for_each(|brain_id|
                            {
                                let id = ChangedPart::Organ(OrganId::Brain(side, Some(brain_id)));

                                draw_bar(BarId::Brain(side.unwrap(), brain_id), id);
                                draw_separator();
                            });
                        },
                        _ =>
                        {
                            draw_bar(BarId::Health, part_id);
                            draw_separator();
                        }
                    }
                }
            }
        }

        let titlebar_id = UiId::Window(this_window_id, WindowPart::Title(TitlePart::Body));
        if !info.mouse_taken
            && parent.input_of(&titlebar_id).is_mouse_inside()
            && info.controls.poll_action_held(&titlebar_id)
        {
            if info.dragging_window.is_none()
            {
                *info.dragging_window = Some((this_window_id, info.mouse_position));
            }
        }
    }

    fn as_id(&self) -> UiIdWindow
    {
        match self
        {
            Self::Inventory(inventory) => UiIdWindow::Inventory(inventory.entity),
            Self::ItemInfo(item) => UiIdWindow::ItemInfo(item.id),
            Self::Stats(owner) => UiIdWindow::Stats(*owner),
            Self::Anatomy(owner) => UiIdWindow::Anatomy(*owner)
        }
    }
}

struct Window
{
    position: Vector2<f32>,
    kind: WindowKind
}

impl Window
{
    fn id(&self) -> UiId
    {
        UiId::Window(self.kind.as_id(), WindowPart::Panel)
    }

    fn update(&mut self, ui: &mut UiController, info: &mut UpdateInfo)
    {
        let id = self.id();

        let position = self.position + info.dragging_window.as_ref().and_then(|(drag_id, start)|
        {
            (self.kind.as_id() == *drag_id).then_some(*start)
        }).map(|start|
        {
            info.mouse_position - start
        }).unwrap_or_default();

        let body = ui.update(id, UiElement{
            texture: UiTexture::Sliced(info.sliced_textures["rounded"]),
            mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
            animation: Animation::normal(),
            position: UiPosition::Absolute{position, align: Default::default()},
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let screen_size = ui.screen_size() / ui.screen_size().max();

        if let Some(width) = body.try_width()
        {
            let half = width * 0.5;
            let limit = screen_size.x * 0.5;

            if (self.position.x - half) < -limit
            {
                self.position.x = half - limit;
            }

            if (self.position.x + half) > limit
            {
                self.position.x = limit - half;
            }
        }

        if let Some(height) = body.try_height()
        {
            let half = height * 0.5;
            let limit = screen_size.y * 0.5;

            if (self.position.y - half) < -limit
            {
                self.position.y = half - limit;
            }

            if (self.position.y + half) > limit
            {
                self.position.y = limit - half;
            }
        }

        self.kind.update(body, info);
    }
}

struct UpdateInfo<'a, 'b, 'c, 'd>
{
    entities: &'a ClientEntities,
    items_info: &'a ItemsInfo,
    sliced_textures: &'a HashMap<String, SlicedTexture>,
    fonts: &'a FontsContainer,
    anatomy_locations: &'a UiAnatomyLocations,
    popup: &'a Option<UiItemPopup>,
    dragging_window: &'d mut Option<(UiIdWindow, Vector2<f32>)>,
    dragging_currently: bool,
    mouse_position: Vector2<f32>,
    mouse_taken: bool,
    controls: &'b mut UiControls<UiId>,
    user_receiver: &'c mut UiReceiver,
    dt: f32
}

#[derive(Debug, Clone, Copy)]
struct BarDisplay
{
    lifetime: f32,
    value: f32
}

impl Default for BarDisplay
{
    fn default() -> Self
    {
        Self{lifetime: 0.0, value: 0.0}
    }
}

#[derive(Debug, Clone)]
struct UiItemPopup
{
    owner: Entity,
    item: InventoryItem,
    position: Vector2<f32>,
    events: Vec<GameUiEvent>
}

#[derive(Debug, Clone)]
struct AnatomyNotification
{
    pub lifetime: f32,
    pub kind: ChangedPart
}

pub struct UiEntities
{
    pub camera: Entity,
    pub player: Entity
}

pub struct Ui
{
    items_info: Arc<ItemsInfo>,
    assets: Arc<Mutex<Assets>>,
    fonts: Rc<FontsContainer>,
    sliced_textures: Rc<HashMap<String, SlicedTexture>>,
    anatomy_locations: UiAnatomyLocations,
    anatomy_locations_small: UiAnatomyLocations,
    user_receiver: Rc<RefCell<UiReceiver>>,
    ui_entities: UiEntities,
    controller: UiController,
    dragging_window: Option<(UiIdWindow, Vector2<f32>)>,
    mouse_position: Vector2<f32>,
    console_contents: Option<String>,
    loading: Option<f32>,
    is_paused: bool,
    is_fade: bool,
    player_dead: bool,
    windows: Vec<Window>,
    stamina: BarDisplay,
    cooldown: BarDisplay,
    notifications: Vec<NotificationInfo>,
    seen_notifications: HashMap<Entity, f32>,
    anatomy_notifications: HashMap<Entity, (f32, Vec<AnatomyNotification>)>,
    popup_unique_id: u8,
    popup: Option<UiItemPopup>
}

impl Ui
{
    pub fn new(
        items_info: Arc<ItemsInfo>,
        info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        ui_entities: UiEntities,
        sliced_textures: Rc<HashMap<String, SlicedTexture>>,
        anatomy_locations: &mut dyn FnMut(&mut ObjectCreateInfo, &str) -> UiAnatomyLocations,
        user_receiver: Rc<RefCell<UiReceiver>>
    ) -> Rc<RefCell<Self>>
    {
        let controller = Controller::new(&info.partial);

        let this = Self{
            items_info,
            assets: info.partial.assets.clone(),
            fonts: info.partial.builder_wrapper.fonts().clone(),
            sliced_textures,
            anatomy_locations: anatomy_locations(info, "anatomy_areas"),
            anatomy_locations_small: anatomy_locations(info, "anatomy_areas_small"),
            user_receiver,
            ui_entities,
            controller,
            dragging_window: None,
            mouse_position: Vector2::zeros(),
            console_contents: None,
            loading: Some(0.0),
            is_paused: false,
            is_fade: false,
            player_dead: false,
            windows: Vec::new(),
            stamina: BarDisplay::default(),
            cooldown: BarDisplay::default(),
            notifications: Vec::new(),
            seen_notifications: HashMap::new(),
            anatomy_notifications: HashMap::new(),
            popup_unique_id: 0,
            popup: None
        };

        let this = Rc::new(RefCell::new(this));

        {
            let ui = this.clone();
            entities.on_anatomy(Box::new(move |OnChangeInfo{entities, entity, ..}|
            {
                if entities.player_exists(entity)
                {
                    return;
                }

                let mut anatomy = some_or_return!(entities.anatomy_mut_no_change(entity));
                anatomy.for_accessed_parts(|part|
                {
                    let default_lifetime = 2.0;

                    let part = AnatomyNotification{
                        lifetime: 0.2,
                        kind: part
                    };

                    ui.borrow_mut().anatomy_notifications.entry(entity)
                        .and_modify(|(lifetime, parts)|
                        {
                            *lifetime = default_lifetime;
                            parts.push(part.clone());
                        }).or_insert_with(|| (default_lifetime, vec![part]));
                });

                if anatomy.is_dead()
                {
                    ui.borrow_mut().anatomy_notifications.remove(&entity);
                }
            }));
        }

        {
            let ui = this.clone();
            entities.on_enemy(Box::new(move |OnChangeInfo{entities, entity, ..}|
            {
                if some_or_return!(entities.enemy(entity)).seen_fraction().is_some()
                {
                    ui.borrow_mut().seen_notifications.insert(entity, 1.0);
                }
            }));
        }

        if DebugConfig::is_enabled(DebugTool::SaveAnatomyColors)
        {
            let width = 640;
            RgbImage::from_fn(width, 200, |x, _y|
            {
                let fraction = x as f32 / (width - 1) as f32;

                let color: [f32; 4] = single_health_color(Some(fraction)).into();
                let color: [u8; 3] = color.into_iter().map(|x| (x.clamp(0.0, 1.0) * 255.0) as u8)
                    .take(3)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();

                color.into()
            }).save("anatomy_colors.png").unwrap();
        }

        this
    }

    pub fn is_input_captured(&self) -> bool
    {
        self.is_loading() || self.console_contents.is_some()
    }

    pub fn is_loading(&self) -> bool
    {
        self.loading.is_some()
    }

    pub fn set_loading(&mut self, value: Option<f32>)
    {
        self.loading = value;
    }

    pub fn set_paused(&mut self, value: bool)
    {
        self.is_paused = value;
    }

    pub fn player_dead(&mut self)
    {
        self.player_dead = true;
    }

    pub fn set_mouse_position(&mut self, position: Vector2<f32>)
    {
        self.mouse_position = position;

        self.controller.set_mouse_position(position);
    }

    pub fn get_console(&self) -> &Option<String>
    {
        &self.console_contents
    }

    pub fn set_console(&mut self, contents: Option<String>)
    {
        self.console_contents = contents;
    }

    fn find_window(&self, id: &UiIdWindow) -> Option<usize>
    {
        self.windows.iter().position(|x|
        {
            x.kind.as_id() == *id
        })
    }

    fn remove_window(&mut self, id: &UiIdWindow) -> bool
    {
        if let Some(index) = self.find_window(id)
        {
            self.windows.remove(index);

            true
        } else
        {
            false
        }
    }

    fn create_window(&mut self, kind: WindowKind)
    {
        self.remove_window(&kind.as_id());

        self.windows.push(Window{
            position: self.mouse_position,
            kind
        });
    }

    pub fn close_inventory(&mut self, owner: Entity) -> bool
    {
        if let Some(UiItemPopup{owner: entity, ..}) = &self.popup
        {
            if *entity == owner
            {
                self.popup = None;
            }
        }

        self.remove_window(&UiIdWindow::Inventory(owner))
    }

    pub fn inventory_changed(&mut self, owner: Entity)
    {
        if let Some(index) = self.find_window(&UiIdWindow::Inventory(owner))
        {
            if let WindowKind::Inventory(inventory) = &mut self.windows[index].kind
            {
                inventory.needs_update = true;
            }
        }
    }

    pub fn open_inventory(&mut self, entity: Entity, on_click: InventoryOnClick)
    {
        self.create_window(WindowKind::Inventory(UiInventory{
            sorter: InventorySorter::default(),
            entity,
            on_click,
            list: UiList::new(),
            buttons: Vec::new(),
            needs_update: true
        }));
    }

    pub fn open_item_info(&mut self, item: Item)
    {
        self.create_window(WindowKind::ItemInfo(item));
    }

    pub fn create_popup(
        &mut self,
        owner: Entity,
        item: InventoryItem,
        actions: Vec<GameUiEvent>
    )
    {
        self.popup_unique_id = self.popup_unique_id.wrapping_add(1);
        self.popup = Some(UiItemPopup{owner, item, position: self.mouse_position, events: actions});
    }

    pub fn show_notification(
        &mut self,
        notification: NotificationInfo
    )
    {
        if let Some(found_notification) = self.notifications.iter_mut().find(|this|
        {
            this.owner == notification.owner && this.kind == notification.kind
        })
        {
            *found_notification = notification;
        } else
        {
            self.notifications.push(notification);
        }
    }

    pub fn set_stamina(&mut self, value: f32)
    {
        self.stamina = BarDisplay{
            lifetime: 1.0,
            value
        };
    }

    pub fn set_cooldown(&mut self, value: f32)
    {
        self.cooldown = BarDisplay{
            lifetime: 1.0,
            value
        };
    }

    pub fn set_fade(&mut self, fade: bool)
    {
        self.is_fade = fade;
    }

    fn update_popup(
        &mut self,
        controls: &mut UiControls<UiId>,
        popup_taken: bool
    )
    {
        if let Some(UiItemPopup{position, events, ..}) = &self.popup
        {
            let id = |part|
            {
                UiId::Popup(self.popup_unique_id, part)
            };

            let popup_body = {
                let mut animation = Animation::normal();
                animation.scaling.as_mut().unwrap().start_scaling = Vector2::new(0.1, 1.0);

                self.controller.update(id(PopupPart::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{a: 1.0, ..BACKGROUND_COLOR})),
                    animation,
                    position: UiPosition::Absolute{position: *position, align: UiPositionAlign{
                        horizontal: AlignHorizontal::Left,
                        vertical: AlignVertical::Top
                    }},
                    children_layout: UiLayout::Vertical,
                    ..Default::default()
                })
            };

            let selected_index = popup_body.mouse_position_inside().map(|position|
            {
                (position.y * events.len() as f32) as usize
            });

            let pressed = events.iter().enumerate().fold(false, |acc, (index, action)|
            {
                let id = |part|
                {
                    id(PopupPart::Button(index as u32, part))
                };

                let body = popup_body.update(id(PopupButtonPart::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{a: 1.0, ..BACKGROUND_COLOR})),
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Rest(1.0)
                    },
                    animation: Animation{
                        position: Some(PositionAnimation::ease_out(10.0)),
                        ..Default::default()
                    },
                    ..Default::default()
                });

                add_padding_horizontal(body, UiSize::Pixels(ITEM_PADDING).into());

                body.update(id(PopupButtonPart::Text), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(SMALL_TEXT_SIZE, action.name().to_owned())),
                    mix: Some(MixColorLch::color(ACCENT_COLOR)),
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(body, UiSize::Rest(1.0).into());

                body.update(id(PopupButtonPart::Separator), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(ACCENT_COLOR)),
                    width: UiSize::Pixels(SEPARATOR_SIZE).into(),
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                if selected_index == Some(index)
                {
                    let offset = popup_body.try_width().unwrap_or(0.0) * 0.2;
                    body.element().position = UiPosition::Next(Vector2::new(offset, 0.0));

                    if controls.take_click_down()
                    {
                        self.user_receiver.borrow_mut().push(UiEvent::Game(action.clone()));
                        return true;
                    }
                }

                acc
            });

            if pressed | (!popup_taken && (controls.is_click_taken() || controls.is_click_down()))
            {
                self.popup = None;
            }
        }
    }

    fn position_of(entities: &ClientEntities, camera: Entity, owner: Entity) -> Option<Vector2<f32>>
    {
        let owner_transform = some_or_return!(entities.transform(owner));
        let camera_transform = some_or_return!(entities.transform(camera));

        let z_of = |pos|
        {
            TilePos::from(pos).to_global().z
        };

        if z_of(owner_transform.position) > z_of(camera_transform.position)
        {
            return None;
        }

        let owner_position = owner_transform.position.xy() - camera_transform.position.xy();
        let position_absolute = owner_position - Vector2::new(0.0, owner_transform.scale.y * 0.5);

        Some(position_absolute / camera_transform.scale.xy().max())
    }

    pub fn update(&mut self, entities: &ClientEntities, controls: &mut UiControls<UiId>, dt: f32)
    {
        if let Some(progress) = self.loading
        {
            self.controller.update(UiId::Loading(LoadingPart::Cover), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                width: 1.0.into(),
                height: 1.0.into(),
                animation: Animation{
                    mix: Some(MixAnimation{
                        decay: MixDecay::all(1.0),
                        close_mix: Some(Lcha{a: 0.0, ..BACKGROUND_COLOR}),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            });

            let body = self.controller.update(UiId::Loading(LoadingPart::Body), UiElement{
                position: UiPosition::Absolute{position: Vector2::zeros(), align: Default::default()},
                children_layout: UiLayout::Vertical,
                animation: Animation{
                    scaling: Some(ScalingAnimation{
                        close_mode: Scaling::EaseOut{decay: 20.0},
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            });

            body.update(UiId::Loading(LoadingPart::Text), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(BIG_TEXT_SIZE, "LOADING")),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                ..UiElement::fit_content()
            });

            let bar = body.update(UiId::Loading(LoadingPart::BarBody), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(ACCENT_COLOR_FADED)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Pixels(30.0).into(),
                children_layout: UiLayout::Horizontal,
                ..Default::default()
            });

            bar.update(UiId::Loading(LoadingPart::Bar), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                width: UiSize::Rest(progress).into(),
                height: UiSize::Rest(1.0).into(),
                ..Default::default()
            });

            add_padding_horizontal(bar, UiSize::Rest(1.0 - progress).into());

            return;
        }

        if self.player_dead
        {
            self.controller.update(UiId::DeathScreen(DeathScreenPart::Panel), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(Lcha{a: 0.8, ..BLACK_COLOR})),
                width: 1.0.into(),
                height: 1.0.into(),
                animation: Animation{
                    mix: Some(MixAnimation{
                        start_mix: Some(Lcha{a: 0.0, ..BLACK_COLOR}),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            });

            let body = self.controller.update(UiId::DeathScreen(DeathScreenPart::Body), UiElement{
                position: UiPosition::Absolute{position: Vector2::zeros(), align: Default::default()},
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            let kills = entities.player(self.ui_entities.player).map(|player|
            {
                player.kills
            }).unwrap_or_else(||
            {
                eprintln!("the player didnt have a player component, returning 0 kills");

                0
            });

            body.update(UiId::DeathScreen(DeathScreenPart::Text), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(MEDIUM_TEXT_SIZE, "stephy is dead :(")),
                mix: Some(MixColorLch::color(WHITE_COLOR)),
                ..UiElement::fit_content()
            });

            add_padding_vertical(body, UiSize::Pixels(TINY_PADDING).into());

            body.update(UiId::DeathScreen(DeathScreenPart::InfoText), UiElement{
                texture: UiTexture::Text(TextInfo{
                    font_size: MEDIUM_TEXT_SIZE,
                    text: TextBlocks(vec![
                        TextInfoBlock{color: ACCENT_COLOR.into(), text: "killed ".into()},
                        TextInfoBlock{color: SPECIAL_COLOR.into(), text: kills.to_string().into()},
                        TextInfoBlock{color: ACCENT_COLOR.into(), text: " enemies".into()}
                    ]),
                    ..Default::default()
                }),
                ..UiElement::fit_content()
            });

            add_padding_vertical(body, UiSize::Pixels(SMALL_PADDING).into());

            let button = body.update(UiId::DeathScreen(DeathScreenPart::Button), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                animation: Animation::button(),
                ..Default::default()
            });

            add_padding_horizontal(button, UiSize::Pixels(TINY_PADDING).into());

            let text = button.update(UiId::DeathScreen(DeathScreenPart::ButtonText), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(BIG_TEXT_SIZE, "RESTART")),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                animation: Animation{
                    mix: Some(MixAnimation::default()),
                    ..Default::default()
                },
                ..UiElement::fit_content()
            });

            add_padding_horizontal(button, UiSize::Pixels(TINY_PADDING).into());

            if button.is_mouse_inside()
            {
                button.element().mix = Some(MixColorLch::color(ACCENT_COLOR));
                text.element().mix = Some(MixColorLch::color(BACKGROUND_COLOR));

                if controls.take_click_down()
                {
                    self.user_receiver.borrow_mut().push(UiEvent::Restart);
                }
            }

            return;
        }

        if self.is_fade
        {
            self.controller.update(UiId::Fade, UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(BLACK_COLOR)),
                width: 1.0.into(),
                height: 1.0.into(),
                position: UiPosition::Inherit,
                animation: Animation{
                    mix: Some(MixAnimation{
                        start_mix: Some(Lcha{a: 0.0, ..BLACK_COLOR}),
                        close_mix: Some(Lcha{a: 0.0, ..BLACK_COLOR}),
                        decay: MixDecay::all(20.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            });
        }

        let ui_screen_width = {
            let size = self.controller.screen_size();

            size.x / size.max()
        };

        if let Some(anatomy) = entities.anatomy(self.ui_entities.player)
        {
            let health_outer = self.controller.update(UiId::Health(HealthPart::OuterPanel), UiElement{
                width: ui_screen_width.into(),
                height: UiSize::Rest(1.0).into(),
                position: UiPosition::Inherit,
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            add_padding_vertical(health_outer, UiSize::Rest(1.0).into());

            let health_inner = health_outer.update(UiId::Health(HealthPart::InnerPanel), UiElement{
                width: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Horizontal,
                ..Default::default()
            });

            add_padding_horizontal(health_inner, UiSize::Rest(1.0).into());

            let panel_vertical = health_inner.update(UiId::Health(HealthPart::PanelVertical), UiElement{
                texture: UiTexture::Custom("ui/health_panel.png".into()),
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                children_layout: UiLayout::Vertical,
                ..UiElement::fit_content()
            });

            add_padding_vertical(panel_vertical, UiSize::Rest(1.0).into());

            let panel = panel_vertical.update(UiId::Health(HealthPart::Panel), UiElement{
                children_layout: UiLayout::Horizontal,
                width: UiSize::Rest(1.0).into(),
                ..Default::default()
            });

            add_padding_horizontal(panel, UiSize::Rest(1.0).into());

            let body = panel.update(UiId::Health(HealthPart::Body), UiElement::default());

            {
                let texture = UiTexture::Custom("ui/anatomy_outline.png".into());

                let texture_size = self.controller.texture_size(&texture);
                let offset = texture_size - self.controller.texture_size(&UiTexture::CustomId(self.anatomy_locations.full));

                body.update(UiId::Health(HealthPart::Outline), UiElement{
                    texture,
                    mix: Some(MixColorLch::color(WHITE_COLOR)),
                    position: UiPosition::Offset(UiId::Health(HealthPart::Body), Vector2::new(-offset.x * 0.5, 0.0)),
                    ..UiElement::fit_content()
                });
            }

            self.anatomy_locations.locations.iter().for_each(|(part_id, location)|
            {
                let highlighted = false;

                let color = health_color(&anatomy, *part_id);
                let health_color = if highlighted
                {
                    color.with_added_lightness(20.0).with_added_chroma(-30.0)
                } else
                {
                    color
                };

                let size: UiElementSize<_> = UiSize::FitContent(1.0).into();
                body.update(UiId::Health(HealthPart::Anatomy(*part_id)), UiElement{
                    texture: UiTexture::CustomId(location.id),
                    mix: Some(MixColorLch::color(health_color)),
                    position: UiPosition::Inherit,
                    width: size.clone(),
                    height: size,
                    animation: Animation{
                        mix: Some(MixAnimation{
                            decay: MixDecay{l: 50.0, c: 50.0, ..MixDecay::all(20.0)},
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            });

            add_padding_horizontal(panel, UiSize::Pixels(TINY_PADDING).into());

            add_padding_vertical(panel_vertical, UiSize::Pixels(SMALL_PADDING * 0.5).into());
        }

        let position_of = {
            let camera = self.ui_entities.camera;
            move |owner|
            {
                Self::position_of(entities, camera, owner)
            }
        };

        self.notifications.retain_mut(|notification|
        {
            notification.lifetime -= dt;

            let id = |part|
            {
                UiId::Notification(notification.owner, notification.kind.clone(), part)
            };

            let position = some_or_value!(position_of(notification.owner), false);

            let is_active = notification.lifetime > 0.0;

            let animation = Animation{
                mix: Some(MixAnimation{
                    start_mix: Some(Lcha{a: 0.0, ..BACKGROUND_COLOR}),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let body = self.controller.update(id(NotificationPart::Body), UiElement{
                texture: UiTexture::Sliced(self.sliced_textures["rounded"]),
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                position: UiPosition::Absolute{position, align: UiPositionAlign{
                    horizontal: AlignHorizontal::Middle,
                    vertical: AlignVertical::Bottom
                }},
                animation: animation.clone(),
                ..Default::default()
            });

            let is_fully_closed = body.is_mix_near().unwrap_or(false) && notification.is_closed && !is_active;

            add_padding_horizontal(body, UiSize::Pixels(NOTIFICATION_PADDING).into());

            match &notification.kind
            {
                NotificationKindInfo::Text{icon, text} =>
                {
                    let transparency = if let NotificationIcon::Door(NotificationDoor::Close(x)) = icon
                    {
                        *x
                    } else
                    {
                        false
                    };

                    let icon = match icon
                    {
                        NotificationIcon::GoUp => "ui/up_icon.png",
                        NotificationIcon::GoDown => "ui/down_icon.png",
                        NotificationIcon::Door(NotificationDoor::Open) => "ui/door_open_icon.png",
                        NotificationIcon::Door(NotificationDoor::Close(_)) => "ui/door_close_icon.png"
                    };

                    let aspect = {
                        let size = self.assets.lock().texture_by_name(icon).lock().size();

                        size.x / size.y
                    };

                    let a = if transparency { 0.25 } else { 1.0 };

                    let size = 20.0;
                    let icon = body.update(id(NotificationPart::Icon), UiElement{
                        texture: UiTexture::Custom(icon.into()),
                        width: UiSize::Pixels(size * aspect).into(),
                        height: UiSize::Pixels(size).into(),
                        mix: Some(MixColorLch::color(Lcha{a, ..ACCENT_COLOR})),
                        animation: animation.clone(),
                        ..Default::default()
                    });

                    add_padding_horizontal(body, UiSize::Pixels(NOTIFICATION_PADDING * 0.5).into());

                    let text = body.update(id(NotificationPart::Text), UiElement{
                        texture: UiTexture::Text(TextInfo::new_simple(SMALL_TEXT_SIZE, text.clone())),
                        mix: Some(MixColorLch::color(Lcha{a, ..ACCENT_COLOR})),
                        animation,
                        ..UiElement::fit_content()
                    });

                    if !is_active
                    {
                        icon.element().mix = Some(MixColorLch::color(Lcha{a: 0.0, ..ACCENT_COLOR}));
                        text.element().mix = Some(MixColorLch::color(Lcha{a: 0.0, ..ACCENT_COLOR}));
                    }
                }
            }

            add_padding_horizontal(body, UiSize::Pixels(NOTIFICATION_PADDING).into());

            if !is_active
            {
                body.element().mix = Some(MixColorLch::color(Lcha{a: 0.0, ..BACKGROUND_COLOR}));
                notification.is_closed = true;
            }

            !is_fully_closed
        });

        self.anatomy_notifications.retain(|entity, (lifetime, parts)|
        {
            *lifetime -= dt;

            let position = some_or_value!(position_of(*entity), false);

            let body = self.controller.update(UiId::AnatomyNotification(*entity, AnatomyNotificationPart::Body), UiElement{
                position: UiPosition::Absolute{position, align: UiPositionAlign{
                    horizontal: AlignHorizontal::Middle,
                    vertical: AlignVertical::Bottom
                }},
                animation: Animation{
                    position: None,
                    ..Animation::normal()
                },
                ..Default::default()
            });

            let alpha = {
                let other_position = some_or_value!(entities.transform(*entity), false).position;
                let player_position = some_or_value!(entities.transform(self.ui_entities.player), false).position;

                let distance = player_position.metric_distance(&other_position);

                lerp(0.9, 0.1, (distance / (TILE_SIZE * 4.0)).min(1.0))
            };

            let anatomy = some_or_value!(entities.anatomy(*entity), false);
            self.anatomy_locations_small.locations.iter().for_each(|(part_id, location)|
            {
                let selected = parts.iter().any(|x|
                {
                    x.kind == *part_id
                });

                let color = health_color(&anatomy, *part_id);
                let health_color = if selected
                {
                    color.with_added_lightness(50.0).with_added_chroma(-50.0)
                } else
                {
                    color
                };

                let lightness_decay = if selected { 100.0 } else { 30.0 };

                body.update(UiId::AnatomyNotification(*entity, AnatomyNotificationPart::Part(*part_id)), UiElement{
                    texture: UiTexture::CustomId(location.id),
                    mix: Some(MixColorLch::color(Lcha{a: health_color.a * alpha, ..health_color})),
                    position: UiPosition::Inherit,
                    animation: Animation{
                        mix: Some(MixAnimation{
                            decay: MixDecay{l: lightness_decay, c: lightness_decay, ..MixDecay::all(20.0)},
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    ..UiElement::fit_content()
                });
            });

            parts.retain_mut(|part|
            {
                part.lifetime -= dt;

                part.lifetime > 0.0
            });

            *lifetime > 0.0
        });

        self.seen_notifications.retain(|&entity, lifetime|
        {
            let id = |part|
            {
                UiId::SeenNotification(entity, part)
            };

            let enemy = some_or_value!(entities.enemy(entity), false);
            let fraction = enemy.seen_fraction();
            let is_attacking = enemy.is_attacking();

            let position = some_or_value!(position_of(entity), false);

            let body_position = UiPosition::Absolute{position, align: UiPositionAlign{
                horizontal: AlignHorizontal::Middle,
                vertical: AlignVertical::Bottom
            }};

            let body = self.controller.update(id(SeenNotificationPart::Body), UiElement{
                position: body_position.clone(),
                children_layout: UiLayout::Vertical,
                animation: Animation{
                    position: None,
                    scaling: Some(ScalingAnimation{
                        close_mode: Scaling::EaseIn(EaseInInfo::new(0.2)),
                        close_scaling: Vector2::new(0.8, 0.0),
                        ..Animation::normal().scaling.unwrap()
                    }),
                    ..Animation::normal()
                },
                ..Default::default()
            });

            let is_detected = fraction.is_none() && is_attacking;

            if fraction.is_none()
            {
                *lifetime -= dt;
            } else
            {
                *lifetime = 1.0;
            }

            let faded_id = id(SeenNotificationPart::Back);

            if !is_detected
            {
                body.update(faded_id.clone(), UiElement{
                    texture: UiTexture::Custom("ui/seen_faded.png".into()),
                    position: UiPosition::Inherit,
                    ..UiElement::fit_content()
                });
            }

            if let Some(fraction) = fraction
            {
                let clip = body.update(id(SeenNotificationPart::Clip), UiElement{
                    position: UiPosition::Inherit,
                    width: UiSize::Rest(1.0).into(),
                    height: UiSize::CopyElement(UiDirection::Vertical, 1.0, faded_id.clone()).into(),
                    children_layout: UiLayout::Vertical,
                    ..Default::default()
                });

                add_padding_vertical(clip, UiSize::Rest(1.0 - fraction).into());

                let clip_body = clip.update(id(SeenNotificationPart::ClipBody), UiElement{
                    width: UiSize::CopyElement(UiDirection::Horizontal, 1.0, faded_id).into(),
                    height: UiSize::Rest(fraction).into(),
                    scissor: true,
                    ..Default::default()
                });

                clip_body.update(id(SeenNotificationPart::Fill), UiElement{
                    texture: UiTexture::Custom("ui/seen.png".into()),
                    position: body_position,
                    ..UiElement::fit_content()
                });
            } else
            {
                if is_detected
                {
                    body.update(id(SeenNotificationPart::Fill), UiElement{
                        texture: UiTexture::Custom("ui/seen_done.png".into()),
                        position: UiPosition::Inherit,
                        animation: Animation{
                            scaling: Some(ScalingAnimation{
                                start_scaling: Vector2::repeat(2.0),
                                start_mode: Scaling::EaseOut{decay: 20.0},
                                close_mode: Scaling::Ignore,
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        ..UiElement::fit_content()
                    });
                }
            }

            *lifetime > 0.0
        });

        let popup_taken = self.controller.input_of(&UiId::Popup(self.popup_unique_id, PopupPart::Body))
            .is_mouse_inside();

        let takes_input = self.windows.iter().rposition(|window|
        {
            self.controller.input_of(&window.id()).is_mouse_inside()
        });

        self.windows.iter_mut().enumerate().for_each(|(index, x)|
        {
            let window_taken = takes_input.map(|taken_index|
            {
                index < taken_index
            }).unwrap_or(false);

            let mut info = UpdateInfo{
                entities,
                items_info: &self.items_info,
                sliced_textures: &self.sliced_textures,
                fonts: &self.fonts,
                anatomy_locations: &self.anatomy_locations,
                popup: &self.popup,
                dragging_window: &mut self.dragging_window,
                dragging_currently: false,
                mouse_position: self.mouse_position,
                mouse_taken: window_taken || popup_taken,
                controls,
                user_receiver: &mut self.user_receiver.borrow_mut(),
                dt
            };

            x.update(&mut self.controller, &mut info);

            if let Some((id, start)) = info.dragging_window
            {
                if (*id == x.kind.as_id()) && !info.dragging_currently
                {
                    x.position += self.mouse_position - *start;
                    *info.dragging_window = None;
                }
            }
        });

        self.update_popup(controls, popup_taken);

        let bars_body_outer = self.controller.update(UiId::BarsBody, UiElement{
            width: ui_screen_width.into(),
            height: UiSize::Rest(1.0).into(),
            position: UiPosition::Inherit,
            ..Default::default()
        });

        let bars_body = bars_body_outer.update(UiId::BarsBodyInner, UiElement{
            height: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        add_padding_horizontal(bars_body_outer, UiSize::Rest(1.0).into());
        add_padding_vertical(bars_body, UiSize::Rest(1.0).into());

        let render_bar_display = |kind, bar: &mut BarDisplay, color: Lcha|
        {
            if bar.lifetime > 0.0
            {
                let width = 300.0;
                let outside_offset = Vector2::new(-width / self.controller.screen_size().max(), 0.0);

                let body = bars_body.update(UiId::BarDisplay(kind, BarDisplayPart::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                    width: UiSize::Pixels(width).into(),
                    children_layout: UiLayout::Vertical,
                    animation: Animation{
                        position: Some(PositionAnimation{
                            offsets: Some(PositionOffsets{
                                start: outside_offset,
                                end: outside_offset
                            }),
                            start_mode: Connection::EaseOut{decay: 10.0, limit: None},
                            close_mode: Connection::EaseIn(EaseInInfo::new(0.8)),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                });

                let text_id = UiId::BarDisplay(kind, BarDisplayPart::Text);

                let bar_body = body.update(UiId::BarDisplay(kind, BarDisplayPart::Bar), UiElement{
                    width: UiSize::Rest(1.0).into(),
                    height: UiSize::CopyElement(UiDirection::Vertical, 1.0, text_id.clone()).into(),
                    ..Default::default()
                });

                bar_body.update(UiId::BarDisplay(kind, BarDisplayPart::BarFill), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(color)),
                    width: UiSize::Rest(bar.value).into(),
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                add_padding_horizontal(bar_body, UiSize::Rest(1.0 - bar.value).into());

                body.update(text_id, UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(30, kind.name())),
                    mix: Some(MixColorLch::color(WHITE_COLOR)),
                    position: UiPosition::Inherit,
                    ..UiElement::fit_content()
                });

                bar.lifetime -= dt;
            }
        };

        render_bar_display(BarDisplayKind::Cooldown, &mut self.cooldown, Lcha{h: 4.0, ..ACCENT_COLOR});

        if let Some(anatomy) = entities.anatomy(self.ui_entities.player)
        {
            let oxygen = anatomy.oxygen().current;
            let color = if oxygen <= WINDED_OXYGEN
            {
                let fraction = (oxygen / WINDED_OXYGEN).powi(3);
                Lcha{l: lerp(50.0, ACCENT_COLOR.l, fraction), h: lerp(0.713, 1.5, fraction), ..ACCENT_COLOR}
            } else
            {
                Lcha{h: 1.5, ..ACCENT_COLOR}
            };

            render_bar_display(BarDisplayKind::Stamina, &mut self.stamina, color);
        }

        if self.is_paused
        {
            self.controller.update(UiId::Paused(PausedPart::Cover), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(Lcha{a: 0.5, ..BLACK_COLOR})),
                width: 1.0.into(),
                height: 1.0.into(),
                position: UiPosition::Inherit,
                animation: Animation{
                    mix: Some(MixAnimation{
                        decay: MixDecay::all(40.0),
                        start_mix: Some(Lcha{a: 0.0, ..BLACK_COLOR}),
                        close_mix: Some(Lcha{a: 0.0, ..BLACK_COLOR}),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            });

            self.controller.update(UiId::Paused(PausedPart::Text), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(BIG_TEXT_SIZE, "PAUSED")),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                position: UiPosition::Absolute{position: Vector2::zeros(), align: Default::default()},
                animation: Animation{
                    scaling: Some(ScalingAnimation{
                        close_mode: Scaling::EaseOut{decay: 20.0},
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..UiElement::fit_content()
            });
        }

        if let Some(text) = self.console_contents.as_mut()
        {
            text_input_handle(controls, text);

            let body = self.controller.update(UiId::Console(ConsolePart::Body), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(Lcha{a: 0.5, ..BACKGROUND_COLOR})),
                animation: Animation::normal(),
                position: UiPosition::Absolute{position: Vector2::zeros(), align: Default::default()},
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::Absolute(0.9)),
                    ..Default::default()
                },
                height: UiElementSize{
                    minimum_size: Some(UiMinimumSize::Absolute(0.1)),
                    ..Default::default()
                },
                ..Default::default()
            });

            body.update(UiId::Console(ConsolePart::Text), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(15, text.clone())),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                animation: Animation::typing_text(),
                ..UiElement::fit_content()
            });
        }

        if takes_input.is_some() || popup_taken
        {
            controls.take_click_down();
        }
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut UpdateBuffersInfo,
        dt: f32
    )
    {
        self.controller.create_renders(create_info, dt);
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.controller.update_buffers(info);
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
        self.controller.draw(info);
    }
}
