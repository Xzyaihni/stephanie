use std::{
    rc::Rc,
    cell::RefCell,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{Transform, camera::Camera};

use crate::{
    client::{
        ui_element::*,
        game_state::{EntityCreator, WindowWhich, InventoryWhich, UserEvent, UiReceiver}
    },
    common::{
        lerp,
        some_or_return,
        render_info::*,
        lazy_transform::*,
        watcher::*,
        collider::*,
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


pub struct UiScroll
{
    bar: Entity,
    size: f32,
    global_scroll: Rc<RefCell<f32>>,
    target_scroll: f32,
    scroll: f32
}

impl UiScroll
{
    pub fn new(
        creator: &mut EntityCreator,
        background: Entity
    ) -> Self
    {
        let target_scroll = 0.0;
        let global_scroll = Rc::new(RefCell::new(target_scroll));

        let drag = {
            let global_scroll = global_scroll.clone();

            let scale = creator.entities.target_ref(background).unwrap().scale.xy();

            UiElement{
                kind: UiElementType::Drag{
                    state: Default::default(),
                    on_change: Box::new(move |pos|
                    {
                        global_scroll.replace(1.0 - (pos.y + 0.5));
                    })
                },
                keep_aspect: Some(KeepAspect{
                    scale,
                    position: Vector2::x(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        };

        creator.entities.set_ui_element(background, Some(drag));
        creator.entities.set_lazy_mix(background, Some(LazyMix::ui()));

        let bar = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(background, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/light.png".to_owned()}.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            bar,
            size: 1.0,
            global_scroll,
            target_scroll,
            scroll: target_scroll
        }
    }

    pub fn update(&mut self, entities: &mut ClientEntities, dt: f32)
    {
        let half_size = self.size / 2.0;

        let fit_into = |value: f32, low, high|
        {
            let span = high - low;

            if span <= 0.0
            {
                0.0
            } else
            {
                (value.clamp(low, high) - low) / span
            }
        };

        let current_scroll = *self.global_scroll.borrow();

        self.target_scroll = fit_into(current_scroll, half_size, 1.0 - half_size);

        self.scroll = self.scroll.ease_out(self.target_scroll, 15.0, dt);

        self.update_position(entities);
    }

    pub fn update_size(&mut self, entities: &mut ClientEntities, size: f32)
    {
        if let Some(mut lazy) = entities.lazy_transform_mut(self.bar)
        {
            self.size = size;
            lazy.target().scale.y = self.size;
        }

        self.update_position(entities);
    }

    fn update_position(&mut self, entities: &mut ClientEntities)
    {
        if let Some(mut lazy) = entities.lazy_transform_mut(self.bar)
        {
            let span = 1.0 - self.size;
            let position = if span <= 0.0
            {
                0.0
            } else
            {
                ((self.amount() - 0.5) * span).clamp(-0.5, 0.5)
            };

            lazy.target().position.y = position;
        }
    }

    pub fn amount(&self) -> f32
    {
        self.scroll
    }
}

pub struct ListItem
{
    frame: Entity,
    item: Entity
}

pub struct UiList
{
    panel: Entity,
    scroll: UiScroll,
    height: f32,
    amount: usize,
    amount_changed: bool,
    scissor: Scissor,
    current_start: Rc<RefCell<usize>>,
    items: Vec<String>,
    frames: Vec<ListItem>
}

impl UiList
{
    pub fn new(
        creator: &mut EntityCreator,
        background: Entity,
        width: f32,
        on_change: Rc<RefCell<dyn FnMut(Entity, usize)>>
    ) -> Self
    {
        let panel = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Panel,
                    keep_aspect: Some(KeepAspect{
                        scale: Vector2::new(1.0 - width, 1.0),
                        position: Vector2::zeros(),
                        mode: AspectMode::FillRestX,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                parent: Some(Parent::new(background, true)),
                ..Default::default()
            },
            RenderInfo{
                z_level: ZLevel::UiLow,
                ..Default::default()
            }
        );

        let scroll = {
            let scale = Vector3::new(1.0 - width, 1.0, 1.0);

            creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(background, true)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{name: "ui/light.png".to_owned()}.into()),
                    z_level: ZLevel::UiMiddle,
                    ..Default::default()
                }
            )
        };

        let max_fit = 7;
        let height = 1.0 / max_fit as f32;

        let scroll = UiScroll::new(creator, scroll);

        let current_start = Rc::new(RefCell::new(0));

        let frames = Self::create_items(
            creator,
            on_change,
            current_start.clone(),
            panel,
            max_fit
        );

        let mut this = Self{
            panel,
            scroll,
            height,
            amount: 0,
            amount_changed: true,
            frames,
            scissor: Default::default(),
            current_start,
            items: Vec::new()
        };

        this.update_frame_scissors(creator);

        this
    }

    fn create_items(
        creator: &mut EntityCreator,
        on_change: Rc<RefCell<dyn FnMut(Entity, usize)>>,
        current_start: Rc<RefCell<usize>>,
        parent: Entity,
        max_fit: u32
    ) -> Vec<ListItem>
    {
        let height = 1.0 / max_fit as f32;

        (0..=max_fit as usize).map(|index|
        {
            let on_change = on_change.clone();
            let current_start = current_start.clone();
            let id = creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale: Vector3::new(1.0, height * 0.9, 1.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    lazy_mix: Some(LazyMix::ui()),
                    parent: Some(Parent::new(parent, false)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "ui/lighter.png".to_owned()
                    }.into()),
                    z_level: ZLevel::UiHigh,
                    ..Default::default()
                }
            );

            creator.entities.set_ui_element(id, Some(UiElement{
                kind: UiElementType::Button{
                    on_click: Box::new(move ||
                    {
                        let index = index + *current_start.borrow();
                        (on_change.borrow_mut())(id, index);
                    })
                },
                predicate: UiElementPredicate::Inside(parent),
                ..Default::default()
            }));

            let text_id = creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo::default().into()),
                    parent: Some(Parent::new(id, true)),
                    ..Default::default()
                },
                RenderInfo{
                    z_level: ZLevel::UiHigher,
                    ..Default::default()
                }
            );

            ListItem{frame: id, item: text_id}
        }).collect()
    }

    pub fn set_items(
        &mut self,
        creator: &mut EntityCreator,
        items: Vec<String>
    )
    {
        self.items = items;
        self.amount = self.items.len();

        self.update_amount(creator);
    }

    fn update_amount(&mut self, creator: &mut EntityCreator)
    {
        self.amount_changed = true;

        let size = (1.0 / self.screens_fit()).clamp(0.0, 1.0);

        self.scroll.update_size(creator.entities, size);

        self.frames.iter().enumerate().for_each(|(index, item)|
        {
            if let Some(mut parent) = creator.entities.parent_mut(item.frame)
            {
                parent.visible = index < self.amount;
            }
        });

        self.update_items(creator);
    }

    fn screens_fit(&self) -> f32
    {
        self.amount as f32 * self.height
    }

    fn start_item(&self) -> f32
    {
        let last_start = self.amount as f32 - (1.0 / self.height);
        self.scroll.amount() * last_start.max(0.0)
    }

    fn update_items(
        &mut self,
        creator: &mut EntityCreator
    )
    {
        let start_item = self.start_item() as usize;

        let start_changed = *self.current_start.borrow() != start_item;

        self.current_start.replace(start_item);

        if start_changed || self.amount_changed
        {
            self.frames.iter().take(self.amount).enumerate().for_each(|(index, item)|
            {
                let item_index = index + start_item;

                if let Some(text) = self.items.get(item_index)
                {
                    let object = RenderObjectKind::Text{
                        text: text.clone(),
                        font_size: 60,
                        font: FontStyle::Sans
                    }.into();

                    creator.entities.set_deferred_render_object(item.item, object);
                }
            });

            self.amount_changed = false;
        }

        self.update_item_positions(creator.entities);
    }

    fn update_item_positions(&mut self, entities: &ClientEntities)
    {
        let start = self.start_item();

        let over_height = 1.0 / (1.0 / self.height - 1.0);

        let y = -start * over_height;
        let y_modulo = y % over_height;

        self.frames.iter().enumerate().for_each(|(index, item)|
        {
            let mut transform = entities.lazy_transform_mut(item.frame).unwrap();
            let transform = transform.target();

            transform.position.y = Ui::ui_position(
                transform.scale,
                Vector3::new(0.0, y_modulo + index as f32 * over_height, 0.0)
            ).y;
        });
    }

    pub fn update_scissors(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera
    )
    {
        self.scissor = {
            let transform = creator.entities.transform(self.panel).unwrap();

            let pos = camera.screen_position(transform.position.xy());
            let pos = pos + Vector2::repeat(0.5);

            let size = camera.screen_size(transform.scale.xy());
            let pos = pos - size / 2.0;

            Scissor{
                offset: [0.0, pos.y],
                extent: [1.0, size.y]
            }
        };

        self.update_frame_scissors(creator);
    }

    fn update_frame_scissors(&mut self, creator: &mut EntityCreator)
    {
        self.frames.iter().for_each(|item|
        {
            creator.entities.set_deferred_render_scissor(item.frame, self.scissor.clone());
            creator.entities.set_deferred_render_scissor(item.item, self.scissor.clone());
        });
    }

    pub fn update_after(
        &mut self,
        _creator: &mut EntityCreator,
        _camera: &Camera
    )
    {
    }

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        self.scroll.update(creator.entities, dt);
        self.update_items(creator);
        self.update_scissors(creator, camera);
    }
}

struct UiWindow
{
    body: Entity,
    name: Entity,
    panel: Entity,
    close_button_x: f32,
    is_open: bool,
    resized_update: bool
}

impl UiWindow
{
    pub fn new<Close>(
        creator: &mut EntityCreator,
        anchor: Entity,
        name: String,
        mut on_close: Close
    ) -> Self
    where
        Close: FnMut() + 'static
    {
        let body = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 15.0},
                    connection: Connection::Limit{limit: 1.0},
                    transform: Transform{
                        scale: Vector3::zeros(),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Panel,
                    ..Default::default()
                }),
                parent: Some(Parent::new(anchor, false)),
                watchers: Some(Default::default()),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::UiLow,
                visible: false,
                ..Default::default()
            }
        );

        let panel_size = 0.15;
        let scale = Vector3::new(1.0, panel_size, 1.0);

        let top_panel = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position: Ui::ui_position(scale, Vector3::zeros()),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::UiMiddle,
                ..Default::default()
            }
        );

        let scale = Vector3::new(1.0, 1.0 - panel_size, 1.0);

        let panel = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position: Ui::ui_position(scale, Vector3::y()),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: None,
                z_level: ZLevel::UiMiddle,
                ..Default::default()
            }
        );

        let close_button_x = panel_size;
        let scale = Vector3::new(1.0 - close_button_x, 1.0, 1.0);
        let name = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(top_panel, true)),
                ui_element: Some(UiElement{
                    kind: UiElementType::Panel,
                    keep_aspect: Some(KeepAspect{
                        scale: Vector2::new(close_button_x, 1.0),
                        position: Vector2::zeros(),
                        mode: AspectMode::FillRestX,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text: name,
                    font_size: 80,
                    font: FontStyle::Bold
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        let scale = Vector3::new(close_button_x, 1.0, 1.0);
        creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(top_panel, true)),
                ui_element: Some(UiElement{
                    kind: UiElementType::Button{
                        on_click: Box::new(move ||
                        {
                            on_close();
                        })
                    },
                    keep_aspect: Some(KeepAspect{
                        scale: scale.xy(),
                        position: Vector2::x(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/close_button.png".to_owned()}.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            body,
            name,
            panel,
            close_button_x,
            is_open: false,
            resized_update: true
        }
    }

    pub fn open(&mut self, entities: &mut ClientEntities)
    {
        if self.is_open
        {
            return;
        }

        self.is_open = true;

        let inventory = self.body;

        entities.set_collider(inventory, Some(ColliderInfo{
            kind: ColliderType::Aabb,
            layer: ColliderLayer::Ui,
            move_z: false,
            target_non_lazy: true,
            ..Default::default()
        }.into()));

        open_ui(entities, inventory, Vector3::repeat(0.2));

        self.resized_update = true;
    }

    pub fn close(&mut self, entities: &mut ClientEntities)
    {
        if !self.is_open
        {
            return;
        }

        self.is_open = false;

        let inventory = self.body;

        entities.set_collider(inventory, None);

        close_ui(entities, inventory);
    }

    pub fn update_name(
        &mut self,
        creator: &mut EntityCreator,
        name: String
    )
    {
        let object = RenderObjectKind::Text{
            text: name,
            font_size: 80,
            font: FontStyle::Bold
        }.into();

        creator.entities.set_deferred_render_object(self.name, object);
    }

    pub fn update_after(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera
    )
    {
        if self.resized_update
        {
            self.resized_update = false;

            update_resize_ui(creator.entities, camera.size(), self.body);
        }
    }
}

pub struct InventoryActions<Close, Change>
{
    pub on_close: Close,
    pub on_change: Change
}

pub struct UiInventory
{
    sorter: InventorySorter,
    items_info: Arc<ItemsInfo>,
    items: Rc<RefCell<Vec<InventoryItem>>>,
    inventory: Entity,
    window: UiWindow,
    list: UiList
}

impl UiInventory
{
    pub fn new<Close, Change>(
        creator: &mut EntityCreator,
        items_info: Arc<ItemsInfo>,
        anchor: Entity,
        actions: InventoryActions<Close, Change>
    ) -> Self
    where
        Close: FnMut() + 'static,
        Change: FnMut(Entity, InventoryItem) + 'static
    {
        let InventoryActions{
            on_close,
            mut on_change
        } = actions;

        let window = UiWindow::new(creator, anchor, String::new(), on_close);

        let items = Rc::new(RefCell::new(Vec::new()));

        let on_change = {
            let items = items.clone();
            Rc::new(RefCell::new(move |entity, index|
            {
                let item = items.borrow()[index];

                on_change(entity, item);
            }))
        };

        Self{
            sorter: InventorySorter::default(),
            items_info,
            items,
            inventory: window.body,
            list: UiList::new(creator, window.panel, 1.0 - window.close_button_x, on_change),
            window
        }
    }

    pub fn open_inventory(&mut self, entities: &mut ClientEntities)
    {
        self.window.open(entities);
    }

    pub fn close_inventory(&mut self, entities: &mut ClientEntities)
    {
        self.window.close(entities);
    }

    pub fn body(&self) -> Entity
    {
        self.inventory
    }

    pub fn update_name(
        &mut self,
        creator: &mut EntityCreator,
        name: String
    )
    {
        self.window.update_name(creator, name);
    }

    pub fn update_inventory(
        &mut self,
        creator: &mut EntityCreator,
        entity: Entity
    )
    {
        let inventory = some_or_return!(creator.entities.inventory(entity));
        let mut items: Vec<_> = inventory.items_ids().collect();
        items.sort_by(|a, b|
        {
            self.sorter.order(&self.items_info, a.1, b.1)
        });

        let names = items.iter().map(|x|
        {
            self.items_info.get(x.1.id).name.clone()
        }).collect();

        let new_items = items.into_iter().map(|(index, _)| index).collect();

        drop(inventory);
        self.list.set_items(creator, names);

        self.items.replace(new_items);
    }

    pub fn full_update(
        &mut self,
        creator: &mut EntityCreator,
        entity: Entity
    )
    {
        let name = creator.entities.named(entity).map(|x| x.clone()).unwrap_or_else(||
        {
            "unnamed".to_owned()
        });

        self.update_name(creator, name);
        self.update_inventory(creator, entity);
    }

    pub fn update_after(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera
    )
    {
        self.list.update_after(creator, camera);

        self.window.update_after(creator, camera);
    }

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        if let Some(render) = creator.entities.render(self.inventory)
        {
            if !render.visible
            {
                return;
            }
        }

        self.list.update(creator, camera, dt);
    }
}

struct UiItemInfo
{
    items_info: Arc<ItemsInfo>,
    window: UiWindow,
    text_panel: Entity
}

impl UiItemInfo
{
    pub fn new<Close>(
        creator: &mut EntityCreator,
        items_info: Arc<ItemsInfo>,
        anchor: Entity,
        on_close: Close
    ) -> Self
    where
        Close: FnMut() + 'static,
    {
        let window = UiWindow::new(creator, anchor, String::new(), on_close);

        let padding = 0.05;

        let text_panel = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale: Vector3::new(1.0 - padding, 1.0, 1.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(window.panel, true)),
                ..Default::default()
            },
            RenderInfo{
                object: None,
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            items_info,
            window,
            text_panel
        }
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
    }

    pub fn set_item(&mut self, creator: &mut EntityCreator, item: Item)
    {
        let info = self.items_info.get(item.id);

        let title = format!("info about - {}", info.name);

        self.window.update_name(creator, title);

        let description = format!(
            "{} weighs around {} kg\nand is around {} meters in size!\nbla bla bla",
            info.name,
            info.mass,
            info.scale
        );

        creator.entities.set_deferred_render_object(self.text_panel, RenderObjectKind::Text{
            text: description,
            font_size: 40,
            font: FontStyle::Bold
        }.into());
    }

    pub fn open(&mut self, entities: &mut ClientEntities)
    {
        self.window.open(entities);
    }

    #[allow(dead_code)]
    pub fn close(&mut self, entities: &mut ClientEntities)
    {
        self.window.close(entities);
    }

    pub fn update_after(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera
    )
    {
        self.window.update_after(creator, camera);
    }
}

fn update_resize_ui(entities: &ClientEntities, size: Vector2<f32>, entity: Entity)
{
    let width_smaller = size.x < size.y;

    let min_size = if width_smaller { size.x } else { size.y };

    if let Some(mut lazy) = entities.lazy_transform_mut(entity)
    {
        let scale = {
            let scale = lazy.target().scale;

            0.5 - if width_smaller { scale.x } else { scale.y } / 2.0
        };

        lazy.set_connection_limit(min_size * scale);
    }
}

fn open_ui(entities: &ClientEntities, entity: Entity, scale: Vector3<f32>)
{
    entities.target(entity).unwrap().scale = scale
        .component_mul(&Vector3::new(4.0, 0.1, 1.0));

    entities.end_sync(entity, |mut current, end|
    {
        current.scale = end.scale;
    });

    *entities.visible_target(entity).unwrap() = true;

    let mut lazy = entities.lazy_transform_mut(entity).unwrap();
    lazy.target().scale = scale;
}

pub fn close_ui(entities: &ClientEntities, entity: Entity)
{
    let current_scale;
    {
        let mut lazy = some_or_return!(entities.lazy_transform_mut(entity));
        current_scale = lazy.target_ref().scale;
        lazy.target().scale = Vector3::zeros();
    }

    let watchers = entities.watchers_mut(entity);
    if let Some(mut watchers) = watchers
    {
        let near = 0.2 * current_scale.min();

        let watcher = Watcher{
            kind: WatcherType::ScaleDistance{from: Vector3::zeros(), near},
            action: WatcherAction::SetVisible(false),
            ..Default::default()
        };

        watchers.push(watcher);
    }
}

fn on_close(
    user_receiver: &Rc<RefCell<UiReceiver>>,
    which: WindowWhich
) -> impl FnMut()
{
    let receiver = user_receiver.clone();

    move ||
    {
        receiver.borrow_mut().push(UserEvent::Close(which));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationId(usize);

pub struct BarNotification
{
    body: Entity,
    bar: Entity
}

impl BarNotification
{
    pub fn new(
        creator: &mut EntityCreator,
        anchor: Entity,
        name: String
    ) -> Self
    {
        let body = creator.entities.push(
            true,
            EntityInfo{
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                    z_level: ZLevel::UiLow,
                    visible: false,
                    ..Default::default()
                }),
                follow_position: Some(FollowPosition::new(anchor, Connection::Rigid)),
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 20.0},
                    transform: Transform{
                        scale: Vector3::zeros(),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                watchers: Some(Default::default()),
                ..Default::default()
            }
        );

        let bar = creator.entities.push(
            true,
            EntityInfo{
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                    z_level: ZLevel::UiMiddle,
                    ..Default::default()
                }),
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            }
        );

        creator.entities.push(
            true,
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Text{
                        text: name,
                        font_size: 30,
                        font: FontStyle::Bold
                    }.into()),
                    z_level: ZLevel::UiHigh,
                    ..Default::default()
                }),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            }
        );

        Self{
            body,
            bar
        }
    }

    pub fn set_amount(
        &mut self,
        entities: &ClientEntities,
        amount: f32
    )
    {
        let amount = amount.clamp(0.0, 1.0);

        let mut target = some_or_return!(entities.target(self.bar));

        target.position.x = -0.5 + amount / 2.0;
        target.scale.x = amount;
    }
}

pub struct TextNotification
{
    body: Entity
}

pub enum Notification
{
    Bar(BarNotification),
    Text(TextNotification)
}

impl From<BarNotification> for Notification
{
    fn from(x: BarNotification) -> Self
    {
        Self::Bar(x)
    }
}

impl Notification
{
    pub fn set_visibility(&self, entities: &ClientEntities, state: bool)
    {
        let entity = self.body();

        if state
        {
            let width = 0.15;
            let height = width * 0.25;

            open_ui(entities, entity, Vector3::new(width, height, height));
        } else
        {
            close_ui(entities, entity);
        }
    }

    pub fn set_position(&self, entities: &ClientEntities, position: f32)
    {
        if let Some(mut follow_position) = entities.follow_position_mut(self.body())
        {
            follow_position.offset.y = -position;
        }
    }

    fn body(&self) -> Entity
    {
        match self
        {
            Self::Bar(x) => x.body,
            Self::Text(x) => x.body
        }
    }
}

pub struct ActiveNotification
{
    id: NotificationId,
    lifetime: f32
}

pub struct Ui
{
    notifications: Vec<Notification>,
    active_notifications: Vec<ActiveNotification>,
    active_popup: Option<Entity>,
    item_info: UiItemInfo,
    pub player_inventory: UiInventory,
    pub other_inventory: UiInventory
}

impl Ui
{
    pub fn new(
        creator: &mut EntityCreator,
        items_info: Arc<ItemsInfo>,
        anchor: Entity,
        user_receiver: Rc<RefCell<UiReceiver>>
    ) -> Self
    {
        let urx = user_receiver.clone();

        let player_inventory = UiInventory::new(
            creator,
            items_info.clone(),
            anchor,
            InventoryActions{
                on_close: on_close(&user_receiver, WindowWhich::Inventory(InventoryWhich::Player)),
                on_change: move |anchor, item|
                {
                    urx.borrow_mut().push(UserEvent::Popup{
                        anchor,
                        responses: vec![
                            UserEvent::Wield(item),
                            UserEvent::Drop{which: InventoryWhich::Player, item},
                            UserEvent::Info{which: InventoryWhich::Player, item}
                        ]
                    });
                }
            }
        ); 

        let urx = user_receiver.clone();

        let other_inventory = UiInventory::new(
            creator,
            items_info.clone(),
            anchor,
            InventoryActions{
                on_close: on_close(&user_receiver, WindowWhich::Inventory(InventoryWhich::Other)),
                on_change: move |anchor, item|
                {
                    urx.borrow_mut().push(UserEvent::Popup{
                        anchor,
                        responses: vec![
                            UserEvent::Take(item),
                            UserEvent::Info{which: InventoryWhich::Other, item}
                        ]
                    });
                }
            }
        );

        let item_info = UiItemInfo::new(
            creator,
            items_info,
            anchor,
            on_close(&user_receiver, WindowWhich::ItemInfo)
        );

        Self{
            notifications: Vec::new(),
            active_notifications: Vec::new(),
            active_popup: None,
            item_info,
            player_inventory,
            other_inventory
        }
    }

    pub fn set_inventory_state(
        &mut self,
        entities: &mut ClientEntities,
        which: InventoryWhich,
        state: bool
    )
    {
        if !state
        {
            self.close_popup(entities);
        }

        let inventory_ui = match which
        {
            InventoryWhich::Player => &mut self.player_inventory,
            InventoryWhich::Other => &mut self.other_inventory
        };

        if state
        {
            inventory_ui.open_inventory(entities);
        } else
        {
            inventory_ui.close_inventory(entities);
        }
    }

    pub fn create_popup(
        &mut self,
        mut popup_position: Vector2<f32>,
        creator: &mut EntityCreator,
        user_receiver: Rc<RefCell<UiReceiver>>,
        anchor: Entity,
        responses: Vec<UserEvent>
    )
    {
        let button_size = Vector2::new(0.5, 1.0);
        let padding = button_size.y * 0.2;

        let mut scale = Vector2::new(button_size.x, padding * 2.0);
        scale.y += button_size.y * responses.len() as f32;
        scale.y += padding * responses.len().saturating_sub(1) as f32;

        popup_position += scale / 2.0;

        let scale = Vector3::new(scale.x, scale.y, 0.0);

        let body = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 20.0},
                    transform: Transform{
                        position: Vector3::new(popup_position.x, popup_position.y, 0.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::ActiveTooltip,
                    ..Default::default()
                }),
                parent: Some(Parent::new(anchor, true)),
                watchers: Some(Default::default()),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::UiPopupLow,
                ..Default::default()
            }
        );

        creator.entities.target(body).unwrap().scale = scale;

        let total = responses.len();
        responses.into_iter().enumerate().for_each(|(index, response)|
        {
            let i = index as f32 / (total - 1) as f32;

            let fraction_scale = button_size.y / scale.y;

            let depth = {
                let padding = padding / scale.y;
                let half_scale = fraction_scale / 2.0;

                lerp(-0.5 + half_scale + padding, 0.5 - half_scale - padding, i)
            };

            let position = Vector3::new(0.0, depth, 0.0);

            let name = response.name().to_owned();

            let urx = user_receiver.clone();
            let button = creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            position,
                            scale: Vector3::new(1.0, fraction_scale, 1.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(body, true)),
                    ui_element: Some(UiElement{
                        kind: UiElementType::Button{
                            on_click: Box::new(move ||
                            {
                                urx.borrow_mut().push(response.clone());
                            })
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "ui/lighter.png".to_owned()
                    }.into()),
                    z_level: ZLevel::UiPopupMiddle,
                    ..Default::default()
                }
            );

            creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo::default().into()),
                    parent: Some(Parent::new(button, true)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Text{
                        text: name,
                        font_size: 80,
                        font: FontStyle::Bold
                    }.into()),
                    z_level: ZLevel::UiPopupHigh,
                    ..Default::default()
                }
            );
        });

        self.close_popup(creator.entities);

        self.active_popup = Some(body);
    }

    pub fn close_popup(&mut self, entities: &mut ClientEntities)
    {
        if let Some(previous) = self.active_popup.take()
        {
            close_ui(entities, previous);
        }
    }

    pub fn create_info_window(
        &mut self,
        creator: &mut EntityCreator,
        item: Item
    )
    {
        self.item_info.set_item(creator, item);
        self.item_info.open(creator.entities);
    }

    pub fn close_info_window(&mut self, entities: &mut ClientEntities)
    {
        self.item_info.close(entities);
    }

    pub fn push_notification(&mut self, notification: Notification) -> NotificationId
    {
        let id = self.notifications.len();

        self.notifications.push(notification);

        NotificationId(id)
    }

    pub fn set_bar(&mut self, entities: &ClientEntities, id: NotificationId, amount: f32)
    {
        if let Notification::Bar(x) = &mut self.notifications[id.0]
        {
            x.set_amount(entities, amount);
        }
    }

    pub fn activate_notification(
        &mut self,
        entities: &ClientEntities,
        id: NotificationId,
        lifetime: f32
    )
    {
        if let Some(notification) = self.active_notifications.iter_mut().find(|x| x.id == id)
        {
            notification.lifetime = lifetime;
        } else
        {
            let notification = ActiveNotification{id, lifetime};

            self.notifications[id.0].set_visibility(entities, true);

            self.active_notifications.push(notification);
        }
    }

    pub fn update_after(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera
    )
    {
        self.inventories_mut().for_each(|inventory|
        {
            inventory.update_after(creator, camera);
        });

        self.item_info.update_after(creator, camera);
    }

    pub fn update_resize(
        &self,
        entities: &ClientEntities,
        size: Vector2<f32>
    )
    {
        self.inventories().for_each(|inventory|
        {
            update_resize_ui(entities, size, inventory.body());
        });

        update_resize_ui(entities, size, self.item_info.body());

        if let Some(popup) = self.active_popup
        {
            update_resize_ui(entities, size, popup);
        }
    }

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        self.active_notifications.retain_mut(|ActiveNotification{id, lifetime}|
        {
            *lifetime -= dt;

            let keep = *lifetime > 0.0;

            if !keep
            {
                self.notifications[id.0].set_visibility(creator.entities, false);
            }

            keep
        });

        let distance = 0.04;
        let start = 0.08;

        self.active_notifications.iter().enumerate().for_each(|(index, ActiveNotification{id, ..})|
        {
            let position = start + index as f32 * distance;

            self.notifications[id.0].set_position(creator.entities, position);
        });

        self.inventories_mut().for_each(|inventory|
        {
            inventory.update(creator, camera, dt);
        });
    }

    fn inventories(&self) -> impl Iterator<Item=&UiInventory>
    {
        [&self.player_inventory, &self.other_inventory].into_iter()
    }

    fn inventories_mut(&mut self) -> impl Iterator<Item=&mut UiInventory>
    {
        [&mut self.player_inventory, &mut self.other_inventory].into_iter()
    }

    pub fn ui_position(scale: Vector3<f32>, position: Vector3<f32>) -> Vector3<f32>
    {
        (position - Vector3::repeat(0.5)).component_mul(&(Vector3::repeat(1.0) - scale))
    }
}
