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
        ui_element::*,
        game_state::{UiAnatomyLocations, GameState, UserEvent, UiReceiver}
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

mod controller;


const MAX_WINDOWS: usize = 5;

const WINDOW_HEIGHT: f32 = 0.1;
const WINDOW_WIDTH: f32 = WINDOW_HEIGHT * 1.5;
const WINDOW_SIZE: Vector3<f32> = Vector3::new(WINDOW_WIDTH, WINDOW_HEIGHT, WINDOW_HEIGHT);
const TITLE_PADDING: f32 = WINDOW_HEIGHT * 0.1;

const PANEL_SIZE: f32 = 0.15;

const NOTIFICATION_HEIGHT: f32 = 0.0375;
const NOTIFICATION_WIDTH: f32 = NOTIFICATION_HEIGHT * 4.0;

const ANIMATION_SCALE: Vector3<f32> = Vector3::new(4.0, 0.0, 1.0);

const TOOLTIP_LIFETIME: f32 = 0.1;
const CLOSED_LIFETIME: f32 = 1.0;

const DEFAULT_COLOR: [f32; 3] = [0.165, 0.161, 0.192];


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UiWindowId(usize);

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

pub enum WindowCreateInfo
{
    ActionsList{popup_position: Vector2<f32>, responses: Vec<UserEvent>},
    Anatomy{spawn_position: Vector2<f32>, entity: Entity},
    Stats{spawn_position: Vector2<f32>, entity: Entity},
    ItemInfo{spawn_position: Vector2<f32>, item: Item},
    Inventory{
        spawn_position: Vector2<f32>,
        entity: Entity,
        on_click: Box<dyn FnMut(Entity, InventoryItem) -> UserEvent>
    }
}

pub struct Ui
{
    items_info: Arc<ItemsInfo>,
    fonts: Rc<FontsContainer>,
    mouse: Entity,
    console_contents: Option<String>,
    anatomy_locations: UiAnatomyLocations,
    user_receiver: Rc<RefCell<UiReceiver>>
}

impl Ui
{
    pub fn new(
        items_info: Arc<ItemsInfo>,
        fonts: Rc<FontsContainer>,
        entities: &mut ClientEntities,
        mouse: Entity,
        anatomy_locations: UiAnatomyLocations,
        user_receiver: Rc<RefCell<UiReceiver>>
    ) -> Rc<RefCell<Self>>
    {
        let this = Self{
            items_info,
            fonts,
            mouse,
            console_contents: None,
            anatomy_locations,
            user_receiver
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

    pub fn create_window<'a, 'b>(
        this: &Rc<RefCell<Self>>,
        window: WindowCreateInfo,
        id: UiWindowId
    )
    {
        todo!()
    }

    pub fn set_console(&mut self, contents: Option<String>)
    {
        self.console_contents = contents;
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

    pub fn update(
        &mut self,
        creator: &ClientEntities,
        camera: &Camera,
        dt: f32
    )
    {
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
    }
}
