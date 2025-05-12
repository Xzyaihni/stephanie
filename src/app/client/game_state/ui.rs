use std::{
    rc::Rc,
    cell::RefCell,
    sync::Arc
};

use nalgebra::Vector2;

use yanyaengine::{
    FontsContainer,
    TextureId,
    game_object::*
};

use crate::{
    client::{
        RenderCreateInfo,
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
        render_info::*,
        anatomy::*,
        EaseOut,
        Item,
        InventoryItem,
        InventorySorter,
        Entity,
        ItemsInfo,
        entity::ClientEntities
    }
};

use element::*;
use controller::*;

pub mod element;
mod controller;


const TITLE_PADDING: f32 = 0.02;
const ITEM_PADDING: f32 = 10.0;

const BUTTON_SIZE: f32 = 40.0;
const SCROLLBAR_HEIGHT: f32 = BUTTON_SIZE * 5.0;

const SEPARATOR_SIZE: f32 = 3.0;

const NOTIFICATION_HEIGHT: f32 = 0.0375;
const NOTIFICATION_WIDTH: f32 = NOTIFICATION_HEIGHT * 4.0;

const TOOLTIP_LIFETIME: f32 = 0.1;

const BACKGROUND_COLOR: [f32; 4] = [0.923, 0.998, 1.0, 1.0];
const ACCENT_COLOR: [f32; 4] = [1.0, 0.393, 0.901, 1.0];
const HIGHLIGHTED_COLOR: [f32; 4] = [1.0, 0.659, 0.848, 1.0];


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UiId
{
    Screen,
    Padding(u32),
    Console(ConsolePart),
    Popup(u8, PopupPart),
    Window(UiIdWindow, WindowPart)
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ConsolePart
{
    Body,
    Text
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PopupPart
{
    Body,
    Button(u32, PopupButtonPart)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PopupButtonPart
{
    Body,
    Text,
    Separator
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum WindowPart
{
    Panel,
    Body,
    Separator,
    Title(TitlePart),
    Inventory(InventoryPart)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum InventoryPart
{
    Body,
    List(UiListPart),
    Item(u32, ItemPart),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ItemPart
{
    Body,
    Icon(IconPart),
    Name
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum IconPart
{
    Body,
    Picture
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TitlePart
{
    Body,
    Text,
    Button(UiIdTitleButton)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UiListPart
{
    Body,
    Moving,
    Separator,
    Scrollbar,
    BarPad,
    Bar
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UiIdWindow
{
    Inventory(Entity)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UiIdTitleButton
{
    Anatomy,
    Stats,
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
    if info.mouse_taken
    {
        return;
    }

    if button.is_mouse_inside()
    {
        button.element().mix.as_mut().unwrap().color = HIGHLIGHTED_COLOR;

        if info.controls.take_click_down()
        {
            f(info);
        }
    }
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
        mut update_item: impl FnMut(&mut UpdateInfo, UiParentElement, &T, u32, bool)
    ) -> Option<usize>
    {
        assert!(parent.element().children_layout.is_horizontal());

        self.position = self.position.ease_out(self.target_position, 10.0, info.dt);

        let body_id = id(UiListPart::Body);
        let body = parent.update(body_id.clone(), UiElement{
            width: UiSize::Rest(1.0).into(),
            height: UiSize::Rest(1.0).into(),
            scissor: true,
            ..Default::default()
        });

        let body_height = body.try_height()?;

        let items_total = self.items.len() as f32 * item_height;
        let items_fit = (body_height / item_height).ceil() as usize + 1;

        let height = 1.0;

        let bottom_scroll = (items_total - body_height).max(0.0);
        let offset = bottom_scroll * self.position;

        let starting_item = (offset / item_height) as usize;

        let relative_offset = offset % item_height;
        let moving_offset = (height - body_height) / 2.0 - relative_offset;

        let moving_part = body.update(id(UiListPart::Moving), UiElement{
            position: UiPosition::Offset(body_id, Vector2::new(0.0, moving_offset)),
            children_layout: UiLayout::Vertical,
            width: UiSize::Rest(1.0).into(),
            height: height.into(),
            ..Default::default()
        });

        let bar_height = (body_height / items_total).min(1.0);

        if bar_height < 1.0
        {
            parent.update(id(UiListPart::Separator), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColor::color(ACCENT_COLOR)),
                width: UiSize::Pixels(SEPARATOR_SIZE).into(),
                height: UiSize::Rest(1.0).into(),
                animation: Animation::separator_tall(),
                ..Default::default()
            });

            let scrollbar_id = id(UiListPart::Scrollbar);
            let scrollbar = parent.update(scrollbar_id.clone(), UiElement{
                width: UiSize::Pixels(BUTTON_SIZE).into(),
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            if let Some(position) = scrollbar.mouse_position_mapped()
            {
                let position = position.y;

                if scrollbar.is_mouse_inside() && !info.mouse_taken
                {
                    info.controls.poll_action_held();
                }

                if info.controls.observe_action_held()
                {
                    self.target_position = if bar_height > 0.99
                    {
                        0.0
                    } else
                    {
                        let half_bar_height = bar_height / 2.0;
                        (position.clamp(half_bar_height, 1.0 - half_bar_height) - half_bar_height) / (1.0 - bar_height)
                    };
                }
            }

            scrollbar.update(id(UiListPart::BarPad), UiElement{
                height: UiSize::CopyElement(UiDirection::Vertical, (1.0 - bar_height) * self.position, scrollbar_id.clone()).into(),
                ..Default::default()
            });

            let bar = scrollbar.update(id(UiListPart::Bar), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColor::color(ACCENT_COLOR)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::CopyElement(UiDirection::Vertical, bar_height, scrollbar_id).into(),
                animation: Animation::scrollbar_bar(),
                ..Default::default()
            });

            if bar.is_mouse_inside() && !info.mouse_taken
            {
                bar.element().mix.as_mut().unwrap().color = HIGHLIGHTED_COLOR;
            }
        }

        let selected_index = if info.mouse_taken
        {
            None
        } else
        {
            body.mouse_position_inside().and_then(|position|
            {
                let item_height = item_height / body_height;
                let index = starting_item + ((position.y + (relative_offset / body_height)) / item_height) as usize;

                (index < self.items.len()).then(|| index)
            })
        };

        self.items.iter().enumerate().skip(starting_item).take(items_fit).for_each(|(index, name)|
        {
            let is_selected = selected_index.map(|x| x == index).unwrap_or(false);
            update_item(info, moving_part, name, index as u32, is_selected);
        });

        selected_index
    }
}

struct UiInventoryItem
{
    item: InventoryItem,
    name: String,
    aspect: Vector2<f32>,
    texture: Option<TextureId>
}

struct UiTitleButton
{
    id: UiIdTitleButton,
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
            self.sorter.order(&info.items_info, a.1, b.1)
        });

        items.into_iter().map(|(index, x)|
        {
            let item = info.items_info.get(x.id);
            UiInventoryItem{
                item: index,
                name: item.name.clone(),
                aspect: item.aspect,
                texture: item.texture.clone()
            }
        }).collect()
    }

    fn update_items(&mut self, info: &UpdateInfo)
    {
        if self.needs_update
        {
            self.buttons = Vec::new();
            if info.entities.anatomy(self.entity).is_some()
            {
                self.buttons.push(UiTitleButton{
                    id: UiIdTitleButton::Anatomy,
                    texture: "ui/anatomy_button.png".to_owned(),
                    action: Rc::new(|game_state| todo!())
                });
            }

            if info.entities.player(self.entity).is_some()
            {
                self.buttons.push(UiTitleButton{
                    id: UiIdTitleButton::Stats,
                    texture: "ui/stats_button.png".to_owned(),
                    action: Rc::new(|game_state| todo!())
                });
            }

            self.list.items = self.items(info);
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
        let window_id = self.as_id();

        fn with_titlebar<'a, 'b>(
            window_id: UiIdWindow,
            parent: UiParentElement<'a>,
            info: &'b mut UpdateInfo,
            title: String,
            buttons: &[UiTitleButton]
        ) -> UiParentElement<'a>
        {
            let id = {
                let window_id = window_id.clone();
                move |part|
                {
                    UiId::Window(window_id.clone(), part)
                }
            };

            let titlebar = parent.update(id(WindowPart::Title(TitlePart::Body)), UiElement{
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::FitChildren),
                    size: UiSize::Rest(1.0)
                },
                ..Default::default()
            });

            let mut update_button = |button_id, texture, action|
            {
                let size: UiElementSize<UiId> = UiSize::Pixels(BUTTON_SIZE).into();

                let close_button = titlebar.update(id(WindowPart::Title(TitlePart::Button(button_id))), UiElement{
                    texture: UiTexture::Custom(texture),
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                    animation: Animation::button(),
                    width: size.clone(),
                    height: size,
                    ..Default::default()
                });

                handle_button(info, close_button, move |info|
                {
                    info.user_receiver.push(UiEvent::Action(action));
                });
            };

            buttons.iter().for_each(|button|
            {
                update_button(button.id.clone(), button.texture.clone(), button.action.clone());
            });

            let padding_size = UiElementSize{minimum_size: Some(TITLE_PADDING.into()), size: UiSize::Rest(1.0)};

            add_padding_horizontal(titlebar, padding_size.clone());
            titlebar.update(id(WindowPart::Title(TitlePart::Text)), UiElement{
                texture: UiTexture::Text{text: title, font_size: 25},
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                ..UiElement::fit_content()
            });
            add_padding_horizontal(titlebar, padding_size);

            update_button(UiIdTitleButton::Close, "ui/close_button.png".to_owned(), Rc::new(move |game_state|
            {
                game_state.ui.borrow_mut().remove_window(&window_id);
            }));

            parent.update(id(WindowPart::Separator), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColor::color(ACCENT_COLOR)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Pixels(SEPARATOR_SIZE).into(),
                animation: Animation::separator_wide(),
                ..Default::default()
            });

            parent.update(id(WindowPart::Body), UiElement{
                width: UiElementSize{
                    minimum_size: Some(UiMinimumSize::FitChildren),
                    size: UiSize::Rest(1.0)
                },
                ..Default::default()
            })
        }

        match self
        {
            Self::Inventory(inventory) =>
            {
                let id = {
                    let window_id = window_id.clone();
                    move |part|
                    {
                        UiId::Window(window_id.clone(), WindowPart::Inventory(part))
                    }
                };

                let name = info.entities.named(inventory.entity).as_deref().cloned()
                    .unwrap_or_else(|| "unnamed".to_owned());

                let body = with_titlebar(window_id.clone(), parent, &mut info, name, &inventory.buttons);

                body.element().height = UiSize::Pixels(SCROLLBAR_HEIGHT).into();

                let body = body.update(id(InventoryPart::Body), UiElement{
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Pixels(SCROLLBAR_HEIGHT * 2.0)
                    },
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                inventory.update_items(&info);

                let font_size = 20;
                let item_height = info.fonts.text_height(font_size, body.screen_size().max());

                let selected = inventory.list.update(&mut info, body, |list_part|
                {
                    id(InventoryPart::List(list_part))
                }, item_height, |_info, parent, item, index, is_selected|
                {
                    let id = |part|
                    {
                        id(InventoryPart::Item(index, part))
                    };

                    let mut body_color = ACCENT_COLOR;
                    body_color[3] = if is_selected { 0.3 } else { 0.0 };

                    let body = parent.update(id(ItemPart::Body), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColor::color(body_color)),
                        width: UiSize::Rest(1.0).into(),
                        animation: Animation{
                            mix: Animation::button().mix,
                            ..Default::default()
                        },
                        ..Default::default()
                    });

                    add_padding_horizontal(body, UiSize::Pixels(ITEM_PADDING).into());

                    let icon_size = item_height * 0.9;
                    let icon_id = id(ItemPart::Icon(IconPart::Body));
                    let icon = body.update(icon_id.clone(), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColor::color(ACCENT_COLOR)),
                        width: icon_size.into(),
                        height: icon_size.into(),
                        children_layout: UiLayout::Vertical,
                        ..UiElement::default()
                    });

                    if let Some(texture) = item.texture
                    {
                        icon.update(id(ItemPart::Icon(IconPart::Picture)), UiElement{
                            texture: UiTexture::CustomId(texture),
                            width: UiSize::CopyElement(UiDirection::Horizontal, item.aspect.x, icon_id.clone()).into(),
                            height: UiSize::CopyElement(UiDirection::Vertical, item.aspect.y, icon_id).into(),
                            ..UiElement::default()
                        });
                    }

                    add_padding_horizontal(body, UiSize::Pixels(ITEM_PADDING / 2.0).into());

                    body.update(id(ItemPart::Name), UiElement{
                        texture: UiTexture::Text{text: item.name.clone(), font_size},
                        mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                        ..UiElement::fit_content()
                    });
                });

                if let Some(index) = selected
                {
                    if info.controls.take_click_down()
                    {
                        let event = (inventory.on_click)(inventory.entity, inventory.list.items[index].item);

                        info.user_receiver.push(event);
                    }
                }
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
        let body = ui.update(UiId::Window(self.kind.as_id(), WindowPart::Panel), UiElement{
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
    pub items_info: &'a ItemsInfo,
    pub fonts: &'a FontsContainer,
    pub mouse_taken: bool,
    pub controls: &'b mut UiControls,
    pub user_receiver: &'c mut UiReceiver,
    pub dt: f32
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
    windows: Vec<Window>,
    popup_unique_id: u8,
    popup: Option<(Vector2<f32>, Entity, Vec<GameUiEvent>)>
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
            windows: Vec::new(),
            popup_unique_id: 0,
            popup: None
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

    fn find_window(&self, id: &UiIdWindow) -> Option<usize>
    {
        self.windows.iter().position(|x|
        {
            match (&x.kind, id)
            {
                (WindowKind::Inventory(inventory), UiIdWindow::Inventory(other_entity)) if inventory.entity == *other_entity => true,
                _ => false
            }
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

    pub fn close_inventory(&mut self, owner: Entity) -> bool
    {
        if let Some((_, entity, _)) = self.popup
        {
            if entity == owner
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
        self.windows.push(Window{
            position: self.mouse_position,
            kind: WindowKind::Inventory(UiInventory{
                sorter: InventorySorter::default(),
                entity,
                on_click,
                list: UiList::new(),
                buttons: Vec::new(),
                needs_update: true
            })
        });
    }

    pub fn create_popup(&mut self, owner: Entity, actions: Vec<GameUiEvent>)
    {
        self.popup_unique_id = self.popup_unique_id.wrapping_add(1);
        self.popup = Some((self.mouse_position, owner, actions));
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

    pub fn update(&mut self, entities: &ClientEntities, controls: &mut UiControls, dt: f32)
    {
        let popup_taken = {
            if self.controller.input_of(&UiId::Popup(self.popup_unique_id, PopupPart::Body)).is_mouse_inside()
            {
                true
            } else
            {
                false
            }
        };

        self.windows.iter_mut().for_each(|x|
        {
            x.update(&mut self.controller, UpdateInfo{
                entities: entities,
                items_info: &self.items_info,
                fonts: &self.fonts,
                mouse_taken: popup_taken,
                controls: controls,
                user_receiver: &mut self.user_receiver.borrow_mut(),
                dt
            })
        });

        if let Some((position, _, actions)) = &self.popup
        {
            let popup_body = {
                let mut animation = Animation::normal();
                animation.scaling.as_mut().unwrap().start_scaling = Vector2::new(0.1, 1.0);

                self.controller.update(UiId::Popup(self.popup_unique_id, PopupPart::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColor::color(ACCENT_COLOR)),
                    animation,
                    position: UiPosition::Absolute(*position),
                    children_layout: UiLayout::Vertical,
                    ..Default::default()
                })
            };

            let selected_index = popup_body.mouse_position_inside().map(|position|
            {
                (position.y * actions.len() as f32) as usize
            });

            let pressed = actions.iter().enumerate().fold(false, |acc, (index, action)|
            {
                let id = |part|
                {
                    UiId::Popup(self.popup_unique_id, PopupPart::Button(index as u32, part))
                };

                let body = popup_body.update(id(PopupButtonPart::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColor::color(BACKGROUND_COLOR)),
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Rest(1.0)
                    },
                    animation: Animation{
                        position: Some(10.0),
                        ..Default::default()
                    },
                    ..Default::default()
                });

                add_padding_horizontal(body, UiSize::Pixels(ITEM_PADDING).into());

                body.update(id(PopupButtonPart::Text), UiElement{
                    texture: UiTexture::Text{text: action.name().to_owned(), font_size: 20},
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(body, UiSize::Rest(1.0).into());

                body.update(id(PopupButtonPart::Separator), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColor::color(ACCENT_COLOR)),
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

        if let Some(text) = self.console_contents.clone()
        {
            let body = self.controller.update(UiId::Console(ConsolePart::Body), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColor::color(BACKGROUND_COLOR)),
                animation: Animation::normal(),
                position: UiPosition::Absolute(Vector2::zeros()),
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
                texture: UiTexture::Text{text, font_size: 30},
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color(ACCENT_COLOR)}),
                animation: Animation::typing_text(),
                position: UiPosition::Absolute(Vector2::zeros()),
                ..UiElement::fit_content()
            });
        }
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
