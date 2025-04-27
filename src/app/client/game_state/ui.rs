use std::{
    rc::{Weak, Rc},
    cell::RefCell,
    sync::Arc,
    collections::{HashMap, VecDeque}
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{
    MouseButton,
    Transform,
    FontsContainer,
    TextInfo,
    camera::Camera,
    game_object::*
};

use crate::{
    LONGEST_FRAME,
    client::{
        RenderCreateInfo,
        ControlState,
        game_state::{
            KeyMapping,
            UiAnatomyLocations,
            UiControls,
            GameState,
            UiEvent,
            GameUiEvent,
            UiReceiver
        }
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

const INVENTORY_WIDTH: f32 = 0.1;
const SCROLLBAR_WIDTH: f32 = 0.02;
const SCROLLBAR_HEIGHT: f32 = SCROLLBAR_WIDTH * 6.0;

const NOTIFICATION_HEIGHT: f32 = 0.0375;
const NOTIFICATION_WIDTH: f32 = NOTIFICATION_HEIGHT * 4.0;

const ANIMATION_SCALE: Vector3<f32> = Vector3::new(4.0, 0.0, 1.0);

const TOOLTIP_LIFETIME: f32 = 0.1;
const CLOSED_LIFETIME: f32 = 1.0;

const BACKGROUND_COLOR: [f32; 4] = [0.923, 0.998, 1.0, 1.0];
const ACCENT_COLOR: [f32; 4] = [1.0, 0.393, 0.901, 1.0];
const HIGHLIGHTED_COLOR: [f32; 4] = [1.0, 0.659, 0.848, 1.0];

const SCROLLBAR_COLOR: [f32; 4] = [1.0, 0.893, 0.987, 1.0];


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
    WindowTitlebutton(UiIdWindow, UiIdTitlebutton),
    WindowBody(UiIdWindow),
    InventoryList(UiIdWindow),
    Scrollbar(UiIdWindow),
    ScrollbarBar(UiIdWindow)
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

pub type InventoryOnClick = Box<dyn FnMut(Entity, InventoryItem) -> UiEvent>;

pub enum WindowCreateInfo
{
    ActionsList{popup_position: Vector2<f32>, responses: Vec<GameUiEvent>},
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

        fn constrain<'a, F>(f: F) -> F
        where
            F: FnOnce(&'a mut UiParentElement, String) -> &'a mut UiParentElement
        {
            f
        }

        let with_titlebar = constrain(move |parent: &mut UiParentElement, title|
        {
            let titlebar = parent.update(UiId::WindowTitlebar(id), UiElement::default());

            add_padding_horizontal(titlebar, TITLE_PADDING.into());
            titlebar.update(UiId::WindowTitlebarName(id), UiElement{
                texture: UiTexture::Text{text: title, font_size: 30, font: FontStyle::Sans, align: None},
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
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
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                width: size.clone(),
                height: size,
                animation: Animation::button(),
                ..Default::default()
            });

            if close_button.is_mouse_inside()
            {
                close_button.element().mix.as_mut().unwrap().color = HIGHLIGHTED_COLOR;

                if info.controls.take_is_down(&KeyMapping::Mouse(MouseButton::Left))
                {
                    info.user_receiver.push(UiEvent::Action(Rc::new(move |game_state|
                    {
                        game_state.ui.borrow_mut().remove_window(id);
                    })));
                }
            }

            parent.update(UiId::WindowBody(id), UiElement{
                width: UiSize::ParentScale(1.0).into(),
                ..Default::default()
            })
        });

        match self
        {
            Self::Inventory{entity, on_click} =>
            {
                let name = info.entities.named(*entity).as_deref().cloned()
                    .unwrap_or_else(|| "unnamed".to_owned());

                let body = with_titlebar(parent, name);
                body.element().children_layout = UiLayout::Horizontal;

                let put_minimum_size_back = ();
                let inventory_list = body.update(UiId::InventoryList(id), UiElement{
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::Absolute(INVENTORY_WIDTH)),
                        size: UiSize::Rest(1.0)
                    },
                    height: UiSize::ParentScale(1.0).into(),
                    ..Default::default()
                });

                let scrollbar = body.update(UiId::Scrollbar(id), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColor::color(SCROLLBAR_COLOR)),
                    width: SCROLLBAR_WIDTH.into(),
                    height: SCROLLBAR_HEIGHT.into(),
                    ..Default::default()
                });

                let make_this_adjust = ();
                let bar_height = 0.2;
                scrollbar.update(UiId::ScrollbarBar(id), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColor::color(ACCENT_COLOR)),
                    width: UiSize::ParentScale(1.0).into(),
                    height: UiSize::ParentScale(bar_height).into(),
                    ..Default::default()
                });
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

pub struct UpdateInfo<'a, 'b, 'c>
{
    pub entities: &'a ClientEntities,
    pub controls: &'b mut UiControls,
    pub user_receiver: &'c mut UiReceiver
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

    fn remove_window(&mut self, id: UiIdWindow) -> bool
    {
        if let Some(index) = self.windows.iter().position(|x|
        {
            match (&x.kind, id)
            {
                (WindowKind::Inventory{entity, ..}, UiIdWindow::Inventory(other_entity)) if *entity == other_entity => true,
                _ => false
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

    pub fn close_inventory(&mut self, owner: Entity) -> bool
    {
        self.remove_window(UiIdWindow::Inventory(owner))
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

    pub fn update(&mut self, entities: &ClientEntities, controls: &mut UiControls)
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
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                animation: Animation::typing_text(),
                position: UiPosition::Absolute(Vector2::zeros()),
                ..UiElement::fit_content()
            });
        }

        self.windows.iter_mut().for_each(|x|
        {
            x.update(&mut self.controller, UpdateInfo{
                entities: entities,
                controls: controls,
                user_receiver: &mut self.user_receiver.borrow_mut()
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
