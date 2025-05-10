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

const BUTTON_SIZE: f32 = 40.0;
const SCROLLBAR_HEIGHT: f32 = BUTTON_SIZE * 5.0;

const SEPARATOR_SIZE: f32 = 3.0;

const NOTIFICATION_HEIGHT: f32 = 0.0375;
const NOTIFICATION_WIDTH: f32 = NOTIFICATION_HEIGHT * 4.0;

const TOOLTIP_LIFETIME: f32 = 0.1;

const BACKGROUND_COLOR: [f32; 4] = [0.923, 0.998, 1.0, 1.0];
const ACCENT_COLOR: [f32; 4] = [1.0, 0.393, 0.901, 1.0];
const HIGHLIGHTED_COLOR: [f32; 4] = [1.0, 0.659, 0.848, 1.0];


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
    WindowBodyNested(UiIdWindow),
    InventoryList(UiIdWindow, UiListPart),
    InventoryItem(UiIdWindow, u32),
    InventoryItemName(UiIdWindow, u32),
    SeparatorWide(UiIdWindow)
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiListPart
{
    Body,
    Moving,
    Separator,
    Scrollbar,
    Bar
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
type UiParentElement<'a> = TreeInserter<'a, UiId>;

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

fn handle_button(
    info: &mut UpdateInfo,
    button: UiParentElement,
    f: impl FnOnce(&mut UpdateInfo)
)
{
    if button.is_mouse_inside()
    {
        button.element().mix.as_mut().unwrap().color = HIGHLIGHTED_COLOR;

        if info.controls.take_is_down(&KeyMapping::Mouse(MouseButton::Left))
        {
            f(info);
        }
    }
}

pub struct UiList
{
    position: f32,
    items: Vec<String>
}

impl UiList
{
    fn new() -> Self
    {
        Self{
            position: 0.0,
            items: Vec::new()
        }
    }

    fn update(
        &mut self,
        info: &mut UpdateInfo,
        parent: UiParentElement,
        id: impl Fn(UiListPart) -> UiId,
        mut update_item: impl FnMut(&mut UpdateInfo, UiParentElement, &str, u32)
    ) -> Option<usize>
    {
        assert!(parent.element().children_layout.is_horizontal());

        let body_id = id(UiListPart::Body);
        let body = parent.update(body_id, UiElement{
            width: UiSize::Rest(1.0).into(),
            height: UiSize::Rest(1.0).into(),
            scissor: true,
            ..Default::default()
        });

        let height = 1.0;
        let offset = 0.0;

        let moving_part = body.update(id(UiListPart::Moving), UiElement{
            texture: UiTexture::Solid,
            mix: Some(MixColor::color([1.0, 0.0, 0.0, 1.0])),
            position: UiPosition::Offset(body_id, Vector2::new(0.0, offset)),
            children_layout: UiLayout::Vertical,
            width: UiSize::Rest(1.0).into(),
            height: height.into(),
            ..Default::default()
        });

        parent.update(id(UiListPart::Separator), UiElement{
            texture: UiTexture::Solid,
            mix: Some(MixColor::color(ACCENT_COLOR)),
            width: UiSize::Pixels(SEPARATOR_SIZE).into(),
            height: UiSize::Rest(1.0).into(),
            animation: Animation::separator_tall(),
            ..Default::default()
        });

        let scrollbar_id = id(UiListPart::Scrollbar);
        let scrollbar = parent.update(scrollbar_id, UiElement{
            width: UiSize::Pixels(BUTTON_SIZE).into(),
            height: UiSize::Rest(1.0).into(),
            animation: Animation::scrollbar(),
            ..Default::default()
        });

        let make_this_adjust = ();
        let bar_height = 0.2;
        scrollbar.update(id(UiListPart::Bar), UiElement{
            texture: UiTexture::Solid,
            mix: Some(MixColor::color(ACCENT_COLOR)),
            width: UiSize::Rest(1.0).into(),
            height: UiSize::CopyElement(UiDirection::Vertical, bar_height, scrollbar_id).into(),
            animation: Animation::scrollbar_bar(),
            ..Default::default()
        });

        self.items.iter().enumerate().for_each(|(index, name)|
        {
            update_item(info, moving_part, name, index as u32);
        });

        None
    }
}

pub struct UiInventory
{
    items: Vec<InventoryItem>,
    sorter: InventorySorter,
    entity: Entity,
    on_click: InventoryOnClick,
    list: UiList,
    needs_update: bool
}

impl UiInventory
{
    fn items(&self, info: &UpdateInfo) -> (Vec<String>, Vec<InventoryItem>)
    {
        let inventory = some_or_return!(info.entities.inventory(self.entity));

        let mut items: Vec<_> = inventory.items_ids().collect();
        items.sort_by(|a, b|
        {
            self.sorter.order(&info.items_info, a.1, b.1)
        });

        items.into_iter().map(|(index, x)|
        {
            (info.items_info.get(x.id).name.clone(), index)
        }).unzip()
    }

    fn update_items(&mut self, info: &UpdateInfo)
    {
        if self.needs_update
        {
            let (names, items) = self.items(info);

            self.list.items = names;
            self.items = items;
            self.needs_update = false;
        }
    }
}

enum WindowKind
{
    Inventory(UiInventory)
}

impl WindowKind
{
    fn update(&mut self, parent: UiParentElement, mut info: UpdateInfo)
    {
        let id = self.as_id();

        fn constrain<'a, 'b, F>(f: F) -> F
        where
            F: FnOnce(UiParentElement<'a>, &'b mut UpdateInfo, String) -> UiParentElement<'a>
        {
            f
        }

        let with_titlebar = constrain(|parent, info, title|
        {
            let titlebar = parent.update(UiId::WindowTitlebar(id), UiElement{
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::FitChildren),
                    size: UiSize::Rest(1.0)
                },
                ..Default::default()
            });

            add_padding_horizontal(titlebar, TITLE_PADDING.into());
            titlebar.update(UiId::WindowTitlebarName(id), UiElement{
                texture: UiTexture::Text{text: title, font_size: 25},
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                animation: Animation::text(),
                ..UiElement::fit_content()
            });
            add_padding_horizontal(titlebar, TITLE_PADDING.into());

            let size: UiElementSize<UiId> = UiSize::Pixels(BUTTON_SIZE).into();

            let close_button = titlebar.update(UiId::WindowTitlebutton(id, UiIdTitlebutton::Close), UiElement{
                texture: UiTexture::Custom("ui/close_button.png".to_owned()),
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                width: size.clone(),
                height: size,
                animation: Animation::button(),
                ..Default::default()
            });

            handle_button(info, close_button, |info|
            {
                info.user_receiver.push(UiEvent::Action(Rc::new(move |game_state|
                {
                    game_state.ui.borrow_mut().remove_window(id);
                })));
            });

            parent.update(UiId::SeparatorWide(id), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColor::color(ACCENT_COLOR)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Pixels(SEPARATOR_SIZE).into(),
                animation: Animation::separator_wide(),
                ..Default::default()
            });

            parent.update(UiId::WindowBody(id), UiElement{
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::FitChildren),
                    size: UiSize::Rest(1.0)
                },
                ..Default::default()
            })
        });

        match self
        {
            Self::Inventory(inventory) =>
            {
                let name = info.entities.named(inventory.entity).as_deref().cloned()
                    .unwrap_or_else(|| "unnamed".to_owned());

                let body = with_titlebar(parent, &mut info, name);
                body.element().height = UiSize::Pixels(SCROLLBAR_HEIGHT).into();

                let body = body.update(UiId::WindowBodyNested(id), UiElement{
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Pixels(SCROLLBAR_HEIGHT * 2.0)
                    },
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                inventory.update_items(&info);

                inventory.list.update(&mut info, body, |list_part|
                {
                    UiId::InventoryList(id, list_part)
                }, |info, parent, name, index|
                {
                    let body = parent.update(UiId::InventoryItem(id, index), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColor::color([0.0, 1.0, 0.0, 1.0])),
                        width: UiSize::Rest(1.0).into(),
                        ..Default::default()
                    });

                    body.update(UiId::InventoryItemName(id, index), UiElement{
                        texture: UiTexture::Text{text: name.to_owned(), font_size: 20},
                        mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                        ..UiElement::fit_content()
                    });
                });
            }
        }
    }

    fn as_id(&self) -> UiIdWindow
    {
        match self
        {
            Self::Inventory(inventory) => UiIdWindow::Inventory(inventory.entity)
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

pub struct UpdateInfo<'a, 'b, 'c, 'd>
{
    pub entities: &'a ClientEntities,
    pub items_info: &'b ItemsInfo,
    pub controls: &'c mut UiControls,
    pub user_receiver: &'d mut UiReceiver
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

    fn find_window(&self, id: UiIdWindow) -> Option<usize>
    {
        self.windows.iter().position(|x|
        {
            match (&x.kind, id)
            {
                (WindowKind::Inventory(inventory), UiIdWindow::Inventory(other_entity)) if inventory.entity == other_entity => true,
                _ => false
            }
        })
    }

    fn remove_window(&mut self, id: UiIdWindow) -> bool
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

    pub fn close_inventory(&mut self, owner: Entity) -> bool
    {
        self.remove_window(UiIdWindow::Inventory(owner))
    }

    pub fn inventory_changed(&mut self, owner: Entity)
    {
        if let Some(index) = self.find_window(UiIdWindow::Inventory(owner))
        {
            if let WindowKind::Inventory(inventory) = &mut self.windows[index].kind
            {
                inventory.needs_update = true;
            }
        }
    }

    pub fn open_inventory(&mut self, entity: Entity, on_click: InventoryOnClick)
    {
        self.windows.push(Window{
            position: self.mouse_position,
            kind: WindowKind::Inventory(UiInventory{
                items: Vec::new(),
                sorter: InventorySorter::default(),
                entity,
                on_click,
                list: UiList::new(),
                needs_update: true
            })
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
                width: 0.9.into(),
                height: UiElementSize{
                    minimum_size: Some(UiMinimumSize::Absolute(0.1)),
                    ..Default::default()
                },
                ..Default::default()
            });

            body.update(UiId::ConsoleText, UiElement{
                texture: UiTexture::Text{text, font_size: 30},
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
                items_info: &self.items_info,
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
