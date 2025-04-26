use std::{
    rc::{Weak, Rc},
    cell::RefCell,
    sync::Arc,
    collections::{HashMap, VecDeque}
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{Transform, FontsContainer, TextInfo, camera::Camera, game_object::*};

use crate::{
    LONGEST_FRAME,
    client::{
        RenderCreateInfo,
        ControlState,
        game_state::{KeyMapping, UiAnatomyLocations, GameState, UserEvent, UiReceiver}
    },
    common::{
        lerp,
        some_or_return,
        render_info::*,
        lazy_transform::*,
        watcher::*,
        physics::*,
        anatomy::*,
        ObjectsStore,
        EaseOut,
        LazyMix,
        AnyEntities,
        Item,
        InventoryItem,
        InventorySorter,
        Parent,
        Entity,
        ItemsInfo,
        EntityInfo,
        entity::ClientEntities
    }
};

use element::*;
use controller::*;

pub mod element;
mod controller;


const TITLE_PADDING: f32 = 0.02;

const PANEL_SIZE: f32 = 0.15;

const NOTIFICATION_HEIGHT: f32 = 0.0375;
const NOTIFICATION_WIDTH: f32 = NOTIFICATION_HEIGHT * 4.0;

const ANIMATION_SCALE: Vector3<f32> = Vector3::new(4.0, 0.0, 1.0);

const TOOLTIP_LIFETIME: f32 = 0.1;
const CLOSED_LIFETIME: f32 = 1.0;

const BACKGROUND_COLOR: [f32; 4] = [0.923, 0.998, 1.0, 0.9];
const TEXT_COLOR: [f32; 4] = [1.0, 0.393, 0.901, 1.0];


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiId
{
    Screen,
    Padding(u32),
    ConsoleBody,
    ConsoleText,
    Window(UiIdWindow),
    WindowTitlebar(UiIdWindow),
    WindowTitlebarName(UiIdWindow),
    WindowTitlebutton(UiIdWindow, UiIdTitlebutton)
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiIdWindow
{
    Inventory(Entity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiIdTitlebutton
{
    Close
}

type UiController = Controller<UiId>;
type UiParentElement = TreeElement<UiId>;

pub enum NotificationSeverity
{
    Normal,
    DamageMinor,
    Damage,
    DamageMajor
}

pub enum NotificationKindInfo
{
    Bar{name: String, color: [f32; 3], amount: f32},
    Text{severity: NotificationSeverity, text: String}
}

pub struct NotificationInfo
{
    pub owner: Entity,
    pub lifetime: f32,
    pub kind: NotificationKindInfo
}

#[derive(Debug, Clone)]
pub enum TooltipInfo
{
    Anatomy{entity: Entity, id: HumanPartId}
}

pub type InventoryOnClick = Box<dyn FnMut(Entity, InventoryItem) -> UserEvent>;

pub enum WindowCreateInfo
{
    ActionsList{popup_position: Vector2<f32>, responses: Vec<UserEvent>},
    Anatomy{spawn_position: Vector2<f32>, entity: Entity},
    Stats{spawn_position: Vector2<f32>, entity: Entity},
    ItemInfo{spawn_position: Vector2<f32>, item: Item},
    Inventory{
        spawn_position: Vector2<f32>,
        entity: Entity,
        on_click: InventoryOnClick
    }
}

enum WindowKind
{
    Inventory{entity: Entity, on_click: InventoryOnClick}
}

impl WindowKind
{
    fn update(&mut self, parent: &mut UiParentElement, info: UpdateInfo)
    {
        let id = self.as_id();
        let mut with_titlebar = |title|
        {
            let titlebar = parent.update(UiId::WindowTitlebar(id), UiElement::default());

            add_padding_horizontal(titlebar, TITLE_PADDING.into());
            titlebar.update(UiId::WindowTitlebarName(id), UiElement{
                texture: UiTexture::Text{text: title, font_size: 30, font: FontStyle::Sans, align: None},
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(TEXT_COLOR)}),
                animation: Animation::text(),
                ..UiElement::fit_content()
            });
            add_padding_horizontal(titlebar, TITLE_PADDING.into());

            let size = UiElementSize{
                size: UiSize::FitContent(0.5),
                ..Default::default()
            };

            let close_button = titlebar.update(UiId::WindowTitlebutton(id, UiIdTitlebutton::Close), UiElement{
                texture: UiTexture::Custom("ui/close_button.png".to_owned()),
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(TEXT_COLOR)}),
                width: size.clone(),
                height: size,
                animation: Animation::button(),
                ..Default::default()
            });

            if close_button.is_mouse_inside()
            {
                println!("AHHHHHHHH");
            }
        };

        match self
        {
            Self::Inventory{entity, on_click} =>
            {
                let name = info.entities.named(*entity).as_deref().cloned()
                    .unwrap_or_else(|| "unnamed".to_owned());

                with_titlebar(name);
            }
        }
    }

    fn as_id(&self) -> UiIdWindow
    {
        match self
        {
            Self::Inventory{entity, ..} => UiIdWindow::Inventory(*entity)
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
    fn update(&mut self, ui: &mut UiController, info: UpdateInfo)
    {
        let body = ui.update(UiId::Window(self.kind.as_id()), UiElement{
            texture: UiTexture::Solid,
            mix: Some(MixColor::color(BACKGROUND_COLOR)),
            animation: Animation::normal(),
            position: UiPosition::Absolute(self.position),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        self.kind.update(body, info);
    }
}

pub struct UpdateInfo<'a, 'b>
{
    pub entities: &'a ClientEntities,
    pub controls: &'b mut Vec<(ControlState, KeyMapping)>
}

pub struct Ui
{
    items_info: Arc<ItemsInfo>,
    fonts: Rc<FontsContainer>,
    anatomy_locations: UiAnatomyLocations,
    user_receiver: Rc<RefCell<UiReceiver>>,
    controller: UiController,
    mouse_position: Vector2<f32>,
    console_contents: Option<String>,
    windows: Vec<Window>
}

impl Ui
{
    pub fn new(
        items_info: Arc<ItemsInfo>,
        info: &ObjectCreateInfo,
        entities: &mut ClientEntities,
        anatomy_locations: UiAnatomyLocations,
        user_receiver: Rc<RefCell<UiReceiver>>
    ) -> Rc<RefCell<Self>>
    {
        let controller = Controller::new(&info.partial);

        let this = Self{
            items_info,
            fonts: info.partial.builder_wrapper.fonts().clone(),
            anatomy_locations,
            user_receiver,
            controller,
            mouse_position: Vector2::zeros(),
            console_contents: None,
            windows: Vec::new()
        };

        let this = Rc::new(RefCell::new(this));

        let ui = this.clone();
        entities.on_anatomy(Box::new(move |entities, entity|
        {
            let mut broken = Vec::new();
            some_or_return!(entities.anatomy_mut(entity)).for_broken_parts(|part|
            {
                broken.push(part);
            });

            broken.into_iter().for_each(|part|
            {
                let severity = match part.kind
                {
                    BrokenKind::Skin => NotificationSeverity::DamageMinor,
                    BrokenKind::Muscle => NotificationSeverity::Damage,
                    BrokenKind::Bone => NotificationSeverity::DamageMajor
                };

                let kind = NotificationKindInfo::Text{
                    severity,
                    text: part.to_string()
                };

                let notification = NotificationInfo{owner: entity, lifetime: 1.0, kind};

                ui.borrow_mut().set_notification(notification);
            });
        }));

        this
    }

    pub fn set_mouse_position(&mut self, position: Vector2<f32>)
    {
        self.mouse_position = position;

        self.controller.set_mouse_position(position);
    }

    pub fn set_console(&mut self, contents: Option<String>)
    {
        self.console_contents = contents;
    }

    pub fn close_inventory(&mut self, owner: Entity) -> bool
    {
        if let Some(index) = self.windows.iter().position(|x|
        {
            if let WindowKind::Inventory{entity, ..} = x.kind
            {
                entity == owner
            } else
            {
                false
            }
        })
        {
            self.windows.remove(index);

            true
        } else
        {
            false
        }
    }

    pub fn open_inventory(&mut self, entity: Entity, on_click: InventoryOnClick)
    {
        self.windows.push(Window{
            position: self.mouse_position,
            kind: WindowKind::Inventory{entity, on_click}
        });
    }

    pub fn set_notification(
        &mut self,
        notification: NotificationInfo
    )
    {
    }

    pub fn set_tooltip(
        &mut self,
        tooltip: TooltipInfo
    )
    {
    }

    pub fn update(&mut self, info: UpdateInfo)
    {
        if let Some(text) = self.console_contents.clone()
        {
            let body = self.controller.update(UiId::ConsoleBody, UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColor::color(BACKGROUND_COLOR)),
                animation: Animation::normal(),
                position: UiPosition::Absolute(Vector2::zeros()),
                width: UiSize::ParentScale(0.9).into(),
                height: UiElementSize{
                    minimum_size: Some(UiMinimumSize::Absolute(0.1)),
                    ..Default::default()
                },
                ..Default::default()
            });

            body.update(UiId::ConsoleText, UiElement{
                texture: UiTexture::Text{text, font_size: 30, font: FontStyle::Sans, align: None},
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(TEXT_COLOR)}),
                animation: Animation::typing_text(),
                position: UiPosition::Absolute(Vector2::zeros()),
                ..UiElement::fit_content()
            });
        }

        self.windows.iter_mut().for_each(|x|
        {
            x.update(&mut self.controller, UpdateInfo{
                entities: info.entities,
                controls: info.controls
            })
        });
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut RenderCreateInfo,
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
