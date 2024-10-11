use std::{
    rc::{Weak, Rc},
    cell::RefCell,
    sync::Arc,
    collections::VecDeque
};

use nalgebra::{Vector2, Vector3};

use strum::EnumIs;

use yanyaengine::{Transform, camera::Camera};

use crate::{
    client::{
        ui_element::*,
        game_state::{GameState, EntityCreator, UserEvent, UiReceiver}
    },
    common::{
        lerp,
        some_or_return,
        render_info::*,
        lazy_transform::*,
        watcher::*,
        collider::*,
        physics::*,
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


const MAX_WINDOWS: usize = 5;

const WINDOW_HEIGHT: f32 = 0.1;
const WINDOW_WIDTH: f32 = WINDOW_HEIGHT * 1.5;
const WINDOW_SIZE: Vector3<f32> = Vector3::new(WINDOW_WIDTH, WINDOW_HEIGHT, WINDOW_HEIGHT);
const WINDOW_ASPECT: f32 = WINDOW_WIDTH / WINDOW_HEIGHT;

const NOTIFICATION_HEIGHT: f32 = 0.0375;
const ANIMATION_SCALE: Vector3<f32> = Vector3::new(4.0, 0.0, 1.0);

pub type WindowType = Weak<RefCell<UiSpecializedWindow>>;

#[derive(Debug, Clone)]
pub enum WindowError
{
    RemoveNonExistent
}

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

            UiElement{
                kind: UiElementType::Drag{
                    state: Default::default(),
                    on_change: Box::new(move |_, pos|
                    {
                        global_scroll.replace(1.0 - (pos.y + 0.5));
                    })
                },
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

    pub fn update(&mut self, entities: &ClientEntities, dt: f32)
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

    pub fn update_size(&mut self, entities: &ClientEntities, size: f32)
    {
        if let Some(mut lazy) = entities.lazy_transform_mut(self.bar)
        {
            self.size = size;
            lazy.target().scale.y = self.size;
        }

        self.update_position(entities);
    }

    fn update_position(&mut self, entities: &ClientEntities)
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
        let scale = Vector3::new(width, 1.0, 1.0);
        let panel = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        position: Ui::ui_position(scale, Vector3::zeros()),
                        scale,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(background, true)),
                ..Default::default()
            },
            None
        );

        let scroll = {
            let scale = Vector3::new(1.0 - width, 1.0, 1.0);

            creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            position: Ui::ui_position(scale, Vector3::x()),
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
                    on_click: Box::new(move |_|
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
        creator: &EntityCreator,
        items: Vec<String>
    )
    {
        self.items = items;
        self.amount = self.items.len();

        self.update_amount(creator);
    }

    fn update_amount(&mut self, creator: &EntityCreator)
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
        creator: &EntityCreator
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
                        font: FontStyle::Sans,
                        align: TextAlign{
                            horizontal: HorizontalAlign::Left,
                            vertical: VerticalAlign::Middle
                        }
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
            let set_position = |target: &mut Transform|
            {
                target.position.y = Ui::ui_position(
                    target.scale,
                    Vector3::new(0.0, y_modulo + index as f32 * over_height, 0.0)
                ).y;
            };

            let mut transform = entities.lazy_transform_mut(item.frame).unwrap();
            set_position(transform.target());
        });
    }

    pub fn update_scissors(
        &mut self,
        creator: &EntityCreator,
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

    fn update_frame_scissors(&mut self, creator: &EntityCreator)
    {
        self.frames.iter().for_each(|item|
        {
            creator.entities.set_deferred_render_scissor(item.frame, self.scissor.clone());
            creator.entities.set_deferred_render_scissor(item.item, self.scissor.clone());
        });
    }

    pub fn update_after(
        &mut self,
        _creator: &EntityCreator,
        _camera: &Camera
    )
    {
    }

    pub fn update(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        self.scroll.update(creator.entities, dt);
        self.update_items(creator);
        self.update_scissors(creator, camera);
    }
}

struct CustomButton
{
    texture: &'static str,
    on_click: Rc<dyn Fn(&mut GameState)>
}

// a mut ref to a mut ref to a mut ref to a
struct CommonWindowInfo<'a, 'b>
{
    creator: &'a mut EntityCreator<'b>,
    user_receiver: Rc<RefCell<UiReceiver>>,
    ui: Rc<RefCell<Ui>>,
    id: UiWindowId
}

struct UiWindow
{
    body: Entity,
    name: Entity,
    panel: Entity,
    button_x: f32,
    resized_update: bool
}

impl UiWindow
{
    pub fn new(
        info: &mut CommonWindowInfo,
        name: String,
        spawn_position: Vector2<f32>,
        custom_buttons: Vec<CustomButton>
    ) -> Self
    {
        let body = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 15.0},
                    connection: Connection::Limit{mode: LimitMode::Manhattan(Vector3::repeat(1.0))},
                    transform: Transform{
                        scale: WINDOW_SIZE,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                physical: Some(PhysicalProperties{
                    floating: true,
                    fixed: PhysicalFixed{
                        rotation: true
                    },
                    move_z: false,
                    target_non_lazy: true,
                    ..Default::default()
                }.into()),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Aabb,
                    layer: ColliderLayer::Ui,
                    ..Default::default()
                }.into()),
                watchers: Some(Default::default()),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::UiLow,
                ..Default::default()
            }
        );

        info.creator.entities.set_transform(body, Some(Transform{
            position: Vector3::new(spawn_position.x, spawn_position.y, 0.0),
            scale: WINDOW_SIZE.component_mul(&ANIMATION_SCALE),
            ..Default::default()
        }));

        let panel_size = 0.15;
        let scale = Vector3::new(1.0, panel_size, 1.0);

        let top_panel = info.creator.push(
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

        let panel = info.creator.push(
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

        let button_x = panel_size / WINDOW_ASPECT;

        let scale = Vector3::new(1.0 - button_x * (1 + custom_buttons.len()) as f32, 1.0, 1.0);

        let low = button_x * custom_buttons.len() as f32;
        let high = 1.0 - button_x;
        let name = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position: Vector3::new(
                            (low + high) / 2.0 - 0.5,
                            0.0,
                            0.0
                        ),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(top_panel, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text: name,
                    font_size: 80,
                    font: FontStyle::Bold,
                    align: TextAlign::centered()
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        let scale = Vector3::new(button_x, 1.0, 1.0);

        custom_buttons.into_iter().enumerate().for_each(|(index, custom_button)|
        {
            let urx = info.user_receiver.clone();
            let CustomButton{texture, on_click} = custom_button;

            let x = -0.5 + scale.x / 2.0 + scale.x * index as f32;
            info.creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale,
                            position: Vector3::new(x, 0.0, 0.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(top_panel, true)),
                    ui_element: Some(UiElement{
                        kind: UiElementType::Button{
                            on_click: Box::new(move |_|
                            {
                                urx.borrow_mut().push(UserEvent::UiAction(on_click.clone()));
                            })
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: texture.to_owned()
                    }.into()),
                    z_level: ZLevel::UiHigh,
                    ..Default::default()
                }
            );
        });

        let ui = info.ui.clone();
        let id = info.id;

        info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position: Ui::ui_position(scale, Vector3::x()),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(top_panel, true)),
                ui_element: Some(UiElement{
                    kind: UiElementType::Button{
                        on_click: Box::new(move |entities|
                        {
                            let _ = ui.borrow_mut().remove_window_id(entities, id);
                        })
                    },
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
            button_x,
            resized_update: true
        }
    }

    pub fn update_name(
        &mut self,
        creator: &EntityCreator,
        name: String
    )
    {
        let object = RenderObjectKind::Text{
            text: name,
            font_size: 80,
            font: FontStyle::Bold,
            align: TextAlign::centered()
        }.into();

        creator.entities.set_deferred_render_object(self.name, object);
    }

    pub fn update_after(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        if self.resized_update
        {
            self.resized_update = false;

            update_resize_ui(creator.entities, camera.size(), self.body);

            let f = |entity|
            {
                if let Some(mut ui_element) = creator.entities.ui_element_mut(entity)
                {
                    ui_element.update_aspect(
                        creator.entities,
                        entity,
                        camera.aspect()
                    );
                }
            };

            f(self.body);
            creator.entities.for_every_child(self.body, f);
        }
    }
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
    fn new(
        info: &mut CommonWindowInfo,
        owner: Entity,
        spawn_position: Vector2<f32>,
        mut on_click: Box<dyn FnMut(Entity, InventoryItem)>
    ) -> Self
    {
        let items_info = info.ui.borrow().items_info.clone();

        let mut custom_buttons = Vec::new();

        if info.creator.entities.anatomy_exists(owner)
        {
            custom_buttons.push(CustomButton{
                texture: "ui/anatomy_button.png",
                on_click: Rc::new(move |game_state|
                {
                    game_state.add_window(WindowCreateInfo::Anatomy{
                        spawn_position: game_state.ui_mouse_position(),
                        entity: owner
                    });
                })
            });
        }

        if info.creator.entities.player_exists(owner)
        {
            custom_buttons.push(CustomButton{
                texture: "ui/stats_button.png",
                on_click: Rc::new(move |game_state|
                {
                    game_state.add_window(WindowCreateInfo::Stats{
                        spawn_position: game_state.ui_mouse_position(),
                        entity: owner
                    });
                })
            });
        }

        let window = UiWindow::new(info, String::new(), spawn_position, custom_buttons);

        let items = Rc::new(RefCell::new(Vec::new()));

        let on_change = {
            let items = items.clone();
            Rc::new(RefCell::new(move |entity, index|
            {
                let item = items.borrow()[index];

                on_click(entity, item);
            }))
        };

        let mut this = Self{
            sorter: InventorySorter::default(),
            items_info,
            items,
            inventory: window.body,
            list: UiList::new(&mut info.creator, window.panel, 1.0 - window.button_x, on_change),
            window
        };

        this.full_update(&mut info.creator, owner);

        this
    }

    pub fn body(&self) -> Entity
    {
        self.inventory
    }

    pub fn update_name(
        &mut self,
        creator: &EntityCreator,
        name: String
    )
    {
        self.window.update_name(creator, name);
    }

    pub fn update_inventory(
        &mut self,
        creator: &EntityCreator,
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
        creator: &EntityCreator,
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
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        self.list.update_after(creator, camera);

        self.window.update_after(creator, camera);
    }

    pub fn update(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        self.list.update(creator, camera, dt);
    }
}

pub struct UiAnatomy
{
    window: UiWindow
}

impl UiAnatomy
{
    fn new(
        window_info: &mut CommonWindowInfo,
        spawn_position: Vector2<f32>,
        entity: Entity
    ) -> Self
    {
        let window = UiWindow::new(window_info, "anatomy".to_owned(), spawn_position, Vec::new());

        let padding = 0.05;

        let description = format!("this will have anatomy later :)");

        window_info.creator.push(
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
                object: Some(RenderObjectKind::Text{
                    text: description,
                    font_size: 40,
                    font: FontStyle::Bold,
                    align: TextAlign::default()
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            window
        }
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
    }

    pub fn update_after(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        self.window.update_after(creator, camera);
    }
}

pub struct UiStats
{
    window: UiWindow
}

impl UiStats
{
    fn new(
        window_info: &mut CommonWindowInfo,
        spawn_position: Vector2<f32>,
        entity: Entity
    ) -> Self
    {
        let window = UiWindow::new(window_info, "stats".to_owned(), spawn_position, Vec::new());

        let padding = 0.05;

        let description = format!("this will have stats later :)");

        window_info.creator.push(
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
                object: Some(RenderObjectKind::Text{
                    text: description,
                    font_size: 40,
                    font: FontStyle::Bold,
                    align: TextAlign::default()
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            window
        }
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
    }

    pub fn update_after(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        self.window.update_after(creator, camera);
    }
}

pub struct UiItemInfo
{
    window: UiWindow
}

impl UiItemInfo
{
    fn new(
        window_info: &mut CommonWindowInfo,
        spawn_position: Vector2<f32>,
        item: Item
    ) -> Self
    {
        let items_info = window_info.ui.borrow().items_info.clone();
        let info = items_info.get(item.id);

        let title = format!("info about - {}", info.name);

        let window = UiWindow::new(window_info, title, spawn_position, Vec::new());

        let padding = 0.05;

        let description = format!(
            "{} weighs around {} kg\nand is about {} meters in size!\nbla bla bla",
            info.name,
            info.mass,
            info.scale
        );

        window_info.creator.push(
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
                object: Some(RenderObjectKind::Text{
                    text: description,
                    font_size: 40,
                    font: FontStyle::Bold,
                    align: TextAlign::default()
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            window
        }
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
    }

    pub fn update_after(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        self.window.update_after(creator, camera);
    }
}

fn update_resize_ui(entities: &ClientEntities, size: Vector2<f32>, entity: Entity)
{
    if let Some(mut lazy) = entities.lazy_transform_mut(entity)
    {
        let limit = (size - lazy.target().scale.xy()) / 2.0;
        lazy.set_connection_limit(LimitMode::Manhattan(Vector3::new(limit.x, limit.y, 0.0)));
    }
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
            action: WatcherAction::Remove,
            ..Default::default()
        };

        watchers.push(watcher);
    }
}

fn create_notification_body(
    info: &mut CommonWindowInfo,
    entity: Entity
) -> Entity
{
    let position = info.creator.entities.transform(entity).map(|x| x.position).unwrap_or_default();
    let scale = Vector3::new(NOTIFICATION_HEIGHT * 4.0, NOTIFICATION_HEIGHT, 1.0);

    let entity = info.creator.push(
        EntityInfo{
            follow_position: Some(FollowPosition::new(
                entity,
                Connection::EaseOut{decay: 14.0, limit: None}
            )),
            lazy_transform: Some(LazyTransformInfo{
                scaling: Scaling::EaseOut{decay: 16.0},
                transform: Transform{
                    position,
                    scale,
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            ui_element: Some(UiElement{
                world_position: true,
                ..Default::default()
            }),
            watchers: Some(Default::default()),
            ..Default::default()
        },
        RenderInfo{
            object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
            z_level: ZLevel::UiLow,
            ..Default::default()
        }
    );

    info.creator.entities.set_transform(entity, Some(Transform{
        position,
        scale: scale.component_mul(&ANIMATION_SCALE),
        ..Default::default()
    }));

    entity
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
    fn new(
        info: &mut CommonWindowInfo,
        owner: Entity,
        name: String,
        amount: f32
    ) -> Self
    {
        let body = create_notification_body(info, owner);

        let bar = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::UiMiddle,
                ..Default::default()
            }
        );

        info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text: name,
                    font_size: 30,
                    font: FontStyle::Bold,
                    align: TextAlign::centered()
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        let mut this = Self{
            body,
            bar
        };

        this.set_amount(info.creator.entities, amount);

        this
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
    body: Entity,
    text: Entity
}

impl TextNotification
{
    fn new(
        info: &mut CommonWindowInfo,
        owner: Entity,
        text: String
    ) -> Self
    {
        let body = create_notification_body(info, owner);

        let text = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text,
                    font_size: 30,
                    font: FontStyle::Bold,
                    align: TextAlign::centered()
                }.into()),
                z_level: ZLevel::UiHigh,
                ..Default::default()
            }
        );

        Self{
            body,
            text
        }
    }

    pub fn set_text(
        &mut self,
        entities: &ClientEntities,
        text: String
    )
    {
        let object = RenderObjectKind::Text{
            text,
            font_size: 80,
            font: FontStyle::Bold,
            align: TextAlign::centered()
        }.into();

        entities.set_deferred_render_object(self.text, object);
    }
}

macro_rules! quick_casts
{
    ($ref_fn:ident, $mut_fn:ident, $variant:ident, $result:ident) =>
    {
        #[allow(dead_code)]
        pub fn $ref_fn(&self) -> Option<&$result>
        {
            if let Self::$variant(x) = self { Some(x) } else { None }
        }

        #[allow(dead_code)]
        pub fn $mut_fn(&mut self) -> Option<&mut $result>
        {
            if let Self::$variant(x) = self { Some(x) } else { None }
        }
    }
}

pub enum NotificationKind
{
    Bar(BarNotification),
    Text(TextNotification)
}

impl From<BarNotification> for NotificationKind
{
    fn from(x: BarNotification) -> Self
    {
        Self::Bar(x)
    }
}

impl From<TextNotification> for NotificationKind
{
    fn from(x: TextNotification) -> Self
    {
        Self::Text(x)
    }
}

impl NotificationKind
{
    quick_casts!{as_bar_ref, as_bar_mut, Bar, BarNotification}
    quick_casts!{as_text_ref, as_text_mut, Text, TextNotification}

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

pub struct Notification
{
    pub lifetime: f32,
    pub kind: NotificationKind
}

pub struct ActionsList
{
    body: Entity
}

impl ActionsList
{
    fn new(
        info: &mut CommonWindowInfo,
        mut popup_position: Vector2<f32>,
        responses: Vec<UserEvent>
    ) -> Self
    {
        let button_size = WINDOW_SIZE.xy().component_mul(&Vector2::new(0.3, 0.1));

        let padding = button_size.y * 0.2;

        let mut scale = Vector2::new(button_size.x, padding * 2.0);
        scale.y += button_size.y * responses.len() as f32;
        scale.y += padding * responses.len().saturating_sub(1) as f32;

        popup_position += scale / 2.0;

        let scale = Vector3::new(scale.x, scale.y, 0.0);

        let body = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 20.0},
                    transform: Transform{
                        scale: scale.component_mul(&ANIMATION_SCALE),
                        position: Vector3::new(popup_position.x, popup_position.y, 0.0),
                        ..Default::default()
                    },
                    unscaled_position: true,
                    inherit_position: false,
                    ..Default::default()
                }.into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::ActiveTooltip,
                    ..Default::default()
                }),
                watchers: Some(Default::default()),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::UiPopupLow,
                ..Default::default()
            }
        );

        info.creator.entities.target(body).unwrap().scale = scale;

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

            let urx = info.ui.borrow().user_receiver.clone();
            let button = info.creator.push(
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
                            on_click: Box::new(move |_|
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

            info.creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo::default().into()),
                    parent: Some(Parent::new(button, true)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Text{
                        text: name,
                        font_size: 80,
                        font: FontStyle::Bold,
                        align: TextAlign::centered()
                    }.into()),
                    z_level: ZLevel::UiPopupHigh,
                    ..Default::default()
                }
            );
        });

        Self{
            body
        }
    }

    pub fn body(&self) -> Entity
    {
        self.body
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UiWindowId(usize);

pub enum NotificationCreateInfo
{
    Bar{name: String, amount: f32},
    Text{text: String}
}

#[derive(EnumIs)]
pub enum WindowCreateInfo
{
    ActionsList{popup_position: Vector2<f32>, responses: Vec<UserEvent>},
    Notification{owner: Entity, lifetime: f32, info: NotificationCreateInfo},
    Anatomy{spawn_position: Vector2<f32>, entity: Entity},
    Stats{spawn_position: Vector2<f32>, entity: Entity},
    ItemInfo{spawn_position: Vector2<f32>, item: Item},
    Inventory{
        spawn_position: Vector2<f32>,
        entity: Entity,
        on_click: Box<dyn FnMut(Entity, InventoryItem) -> UserEvent>
    }
}

pub enum UiSpecializedWindow
{
    ActionsList(ActionsList),
    Notification(Notification),
    Anatomy(UiAnatomy),
    Stats(UiStats),
    ItemInfo(UiItemInfo),
    Inventory(UiInventory)
}

impl UiSpecializedWindow
{
    quick_casts!{as_actions_list, as_actions_list_mut, ActionsList, ActionsList}
    quick_casts!{as_notification, as_notification_mut, Notification, Notification}
    quick_casts!{as_item_info, as_item_info_mut, ItemInfo, UiItemInfo}
    quick_casts!{as_inventory, as_inventory_mut, Inventory, UiInventory}

    fn body(&self) -> Entity
    {
        match self
        {
            Self::ActionsList(x) => x.body(),
            Self::Notification(x) => x.kind.body(),
            Self::Anatomy(x) => x.body(),
            Self::Stats(x) => x.body(),
            Self::ItemInfo(x) => x.body(),
            Self::Inventory(x) => x.body()
        }
    }

    fn update_after(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        match self
        {
            Self::ActionsList(_) => (),
            Self::Notification(_) => (),
            Self::Anatomy(x) => x.update_after(creator, camera),
            Self::Stats(x) => x.update_after(creator, camera),
            Self::ItemInfo(x) => x.update_after(creator, camera),
            Self::Inventory(x) => x.update_after(creator, camera)
        }
    }

    fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        match self
        {
            Self::ActionsList(_) => (),
            Self::Notification(_) => (),
            Self::Anatomy(_) => (),
            Self::Stats(_) => (),
            Self::ItemInfo(_) => (),
            Self::Inventory(x) => x.update(creator, camera, dt)
        }
    }
}

pub struct Ui
{
    items_info: Arc<ItemsInfo>,
    user_receiver: Rc<RefCell<UiReceiver>>,
    notifications: Vec<UiWindowId>,
    active_popup: Option<UiWindowId>,
    windows_order: VecDeque<UiWindowId>,
    windows: ObjectsStore<Rc<RefCell<UiSpecializedWindow>>>
}

impl Ui
{
    pub fn new(
        items_info: Arc<ItemsInfo>,
        user_receiver: Rc<RefCell<UiReceiver>>
    ) -> Self
    {
        Self{
            items_info,
            user_receiver,
            notifications: Vec::new(),
            active_popup: None,
            windows_order: VecDeque::new(),
            windows: ObjectsStore::new()
        }
    }

    pub fn add_window<'a, 'b>(
        this: Rc<RefCell<Self>>,
        creator: &'a mut EntityCreator<'b>,
        window: WindowCreateInfo
    ) -> WindowType
    {
        let this_cloned = this.clone();

        let is_notification = window.is_notification();
        let is_actions_list = window.is_actions_list();

        let is_normal = !is_notification && !is_actions_list;

        let id = {
            let mut this = this.borrow_mut();

            if is_normal
            {
                let windows_amount = this.windows_order.len();
                debug_assert!(!(windows_amount > MAX_WINDOWS));
                if windows_amount == MAX_WINDOWS
                {
                    let oldest_window = this.windows_order.pop_front().unwrap();
                    this.remove_window_id(creator.entities, oldest_window).unwrap();
                }
            }

            UiWindowId(this.windows.vacant_key())
        };

        let test: &mut EntityCreator<'b> = &mut *creator;
        let window = Self::create_window(this_cloned, test, window, id);
        let weak = Rc::downgrade(&window);

        if is_normal
        {
            this.borrow_mut().windows_order.push_back(id);
        }

        this.borrow_mut().windows.push(window);

        if is_notification
        {
            this.borrow_mut().notifications.push(id);
        } else if is_actions_list
        {
            let mut this = this.borrow_mut();

            this.close_popup(creator.entities);

            this.active_popup = Some(id);
        }

        weak
    }

    pub fn remove_window(
        &mut self,
        entities: &ClientEntities,
        window: Rc<RefCell<UiSpecializedWindow>>
    ) -> Result<(), WindowError>
    {
        // why do i have to do this? i dont get it
        let found = self.windows.iter().find(|(_, x)| Rc::ptr_eq(x, &window));
        if let Some((id, _)) = found
        {
            let id = UiWindowId(id);
            self.remove_window_id(entities, id)
        } else
        {
            Err(WindowError::RemoveNonExistent)
        }
    }

    fn remove_window_id(
        &mut self,
        entities: &ClientEntities,
        id: UiWindowId
    ) -> Result<(), WindowError>
    {
        if let Some(window) = self.windows.remove(id.0)
        {
            let window = window.borrow();
            if window.as_notification().is_some()
            {
                self.notifications.retain(|x| *x != id);
            }

            let body = window.body();
            if let Some(index) = self.windows_order.iter().position(|x| *x == id)
            {
                self.windows_order.remove(index);
            }

            close_ui(entities, body);

            Ok(())
        } else
        {
            Err(WindowError::RemoveNonExistent)
        }
    }

    fn create_window<'a, 'b>(
        this: Rc<RefCell<Self>>,
        creator: &'a mut EntityCreator<'b>,
        window: WindowCreateInfo,
        id: UiWindowId
    ) -> Rc<RefCell<UiSpecializedWindow>>
    {
        let user_receiver = this.borrow().user_receiver.clone();

        let mut window_info = CommonWindowInfo{
            creator,
            user_receiver,
            ui: this,
            id
        };

        let window = match window
        {
            WindowCreateInfo::ActionsList{popup_position, responses} =>
            {
                UiSpecializedWindow::ActionsList(ActionsList::new(
                    &mut window_info,
                    popup_position,
                    responses
                ))
            },
            WindowCreateInfo::Notification{owner, lifetime, info} =>
            {
                let kind: NotificationKind = match info
                {
                    NotificationCreateInfo::Bar{name, amount} =>
                    {
                        BarNotification::new(&mut window_info, owner, name, amount).into()
                    },
                    NotificationCreateInfo::Text{text} =>
                    {
                        TextNotification::new(&mut window_info, owner, text).into()
                    }
                };

                let notification = Notification{
                    lifetime,
                    kind
                };

                UiSpecializedWindow::Notification(notification)
            },
            WindowCreateInfo::Anatomy{spawn_position, entity} =>
            {
                UiSpecializedWindow::Anatomy(UiAnatomy::new(
                    &mut window_info,
                    spawn_position,
                    entity
                ))
            },
            WindowCreateInfo::Stats{spawn_position, entity} =>
            {
                UiSpecializedWindow::Stats(UiStats::new(
                    &mut window_info,
                    spawn_position,
                    entity
                ))
            },
            WindowCreateInfo::ItemInfo{spawn_position, item} =>
            {
                UiSpecializedWindow::ItemInfo(UiItemInfo::new(
                    &mut window_info,
                    spawn_position,
                    item
                ))
            },
            WindowCreateInfo::Inventory{spawn_position, entity, mut on_click} =>
            {
                let urx = window_info.user_receiver.clone();
                UiSpecializedWindow::Inventory(UiInventory::new(
                    &mut window_info,
                    entity,
                    spawn_position,
                    Box::new(move |anchor, item|
                    {
                        urx.borrow_mut().push(on_click(anchor, item));
                    })
                ))
            }
        };

        Rc::new(RefCell::new(window))
    }

    pub fn close_popup(&mut self, entities: &ClientEntities)
    {
        if let Some(previous) = self.active_popup.take()
        {
            let _ = self.remove_window_id(entities, previous);
        }
    }

    pub fn update_after(
        &mut self,
        creator: &EntityCreator,
        camera: &Camera
    )
    {
        self.windows.iter_mut().for_each(|(_, window)|
        {
            window.borrow_mut().update_after(creator, camera);
        });
    }

    pub fn update_resize(
        &self,
        entities: &ClientEntities,
        size: Vector2<f32>
    )
    {
        self.windows.iter().for_each(|(_, window)|
        {
            update_resize_ui(entities, size, window.borrow().body());
        });
    }

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        let distance = 0.04;
        let start = 0.08;

        let to_remove: Vec<_> = self.notifications.iter_mut().enumerate().filter_map(|(index, id)|
        {
            let position = start + index as f32 * distance;

            let mut window = self.windows[id.0].borrow_mut();

            let notification = window.as_notification_mut().unwrap();
            
            notification.kind.set_position(creator.entities, position);

            notification.lifetime -= dt;

            if notification.lifetime <= 0.0
            {
                return Some((index, *id));
            }

            None
        }).collect();

        to_remove.into_iter().for_each(|(index, id)|
        {
            self.notifications.swap_remove(index);
            self.remove_window_id(creator.entities, id).unwrap();
        });

        self.windows.iter_mut().for_each(|(_, window)|
        {
            window.borrow_mut().update(creator, camera, dt);
        });
    }

    pub fn ui_position(scale: Vector3<f32>, position: Vector3<f32>) -> Vector3<f32>
    {
        (position - Vector3::repeat(0.5)).component_mul(&(Vector3::repeat(1.0) - scale))
    }
}
