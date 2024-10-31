use std::{
    rc::{Weak, Rc},
    cell::RefCell,
    sync::Arc,
    collections::{HashMap, VecDeque}
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{Transform, FontsContainer, TextInfo, camera::Camera};

use crate::{
    LONGEST_FRAME,
    client::{
        ui_element::*,
        game_state::{UiAnatomyLocations, GameState, EntityCreator, UserEvent, UiReceiver}
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

pub type WindowType = Weak<RefCell<UiSpecializedWindow>>;

#[derive(Debug, Clone)]
pub enum WindowError
{
    RemoveNonExistent
}

pub struct UiScroll
{
    background: Entity,
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
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        Self{
            background,
            bar,
            size: 1.0,
            global_scroll,
            target_scroll,
            scroll: target_scroll
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.background);
        f(self.bar);
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

impl ListItem
{
    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.frame);
        f(self.item);
    }
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
                    z_level: ZLevel::Ui,
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

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        self.scroll.in_render_order(&mut f);
        self.frames.iter().for_each(|x| x.in_render_order(&mut f));
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
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            );

            creator.entities.set_ui_element(id, Some(UiElement{
                kind: UiElementType::Button(ButtonEvents{
                    on_click: Box::new(move |_|
                    {
                        let index = index + *current_start.borrow();
                        (on_change.borrow_mut())(id, index);
                    }),
                    ..Default::default()
                }),
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
                    z_level: ZLevel::Ui,
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
                        font_size: 20,
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

struct UiWindowInfo
{
    pub name: String,
    pub spawn_position: Vector2<f32>,
    pub custom_buttons: Vec<CustomButton>,
    pub size: Vector2<f32>
}

impl Default for UiWindowInfo
{
    fn default() -> Self
    {
        Self{
            name: "undefined".to_owned(),
            spawn_position: Vector2::zeros(),
            custom_buttons: Vec::new(),
            size: WINDOW_SIZE.xy()
        }
    }
}

struct UiWindow
{
    body: Entity,
    top_panel: Entity,
    panel: Entity,
    name_entity: Entity,
    buttons: Vec<Entity>,
    button_width: f32
}

impl UiWindow
{
    pub fn new(
        info: &mut CommonWindowInfo,
        window_info: UiWindowInfo
    ) -> Self
    {
        let UiWindowInfo{name, spawn_position, custom_buttons, size} = window_info;
        let mut size = Vector3::new(size.x, size.y, 1.0);

        let font_size = 30;
        let align = TextAlign::centered();
        let style = FontStyle::Bold;

        let text_width = info.ui.borrow().fonts.calculate_bounds(TextInfo{
            text: &name,
            font: style,
            align,
            font_size
        }).x;

        let minimum_width = {
            let panel_size = PANEL_SIZE * size.y;

            text_width + TITLE_PADDING + (custom_buttons.len() + 1) as f32 * panel_size
        };
        if minimum_width > size.x
        {
            size.x = minimum_width;
        }

        let panel_size = Self::panel_size(size.y);

        let body = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 15.0},
                    connection: Connection::Limit{mode: LimitMode::Manhattan(Vector3::repeat(1.0))},
                    transform: Transform{
                        scale: size,
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
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        info.creator.entities.set_transform(body, Some(Transform{
            position: Vector3::new(spawn_position.x, spawn_position.y, 0.0),
            scale: size.component_mul(&ANIMATION_SCALE),
            ..Default::default()
        }));

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
                z_level: ZLevel::Ui,
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
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        let button_width = Self::button_width(size.xy());

        let scale = Vector3::new(1.0 - button_width * (1 + custom_buttons.len()) as f32, 1.0, 1.0);

        let low = button_width * custom_buttons.len() as f32;
        let high = 1.0 - button_width;
        let name_entity = info.creator.push(
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
                    font_size,
                    font: style,
                    align
                }.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        let scale = Vector3::new(button_width, 1.0, 1.0);

        let mut buttons = custom_buttons.into_iter().enumerate().map(|(index, custom_button)|
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
                        kind: UiElementType::Button(ButtonEvents{
                            on_click: Box::new(move |_|
                            {
                                urx.borrow_mut().push(UserEvent::UiAction(on_click.clone()));
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: texture.to_owned()
                    }.into()),
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            )
        }).collect::<Vec<_>>();

        let ui = info.ui.clone();
        let id = info.id;

        let close_button = info.creator.push(
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
                    kind: UiElementType::Button(ButtonEvents{
                        on_click: Box::new(move |entities|
                        {
                            let _ = ui.borrow_mut().remove_window_id(entities, id);
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/close_button.png".to_owned()}.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        buttons.push(close_button);

        Self{
            body,
            top_panel,
            panel,
            name_entity,
            buttons,
            button_width
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.body);
        f(self.top_panel);
        f(self.name_entity);
        self.buttons.iter().copied().for_each(f);
    }

    fn panel_size(height: f32) -> f32
    {
        PANEL_SIZE * (WINDOW_SIZE.y / height)
    }

    fn button_width(size: Vector2<f32>) -> f32
    {
        let aspect = size.x / size.y;

        Self::panel_size(size.y) / aspect
    }
}

pub struct UiInventory
{
    sorter: InventorySorter,
    items_info: Arc<ItemsInfo>,
    items: Rc<RefCell<Vec<InventoryItem>>>,
    inventory: Entity,
    list: UiList,
    window: UiWindow
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

        let name = info.creator.entities.named(owner).map(|x| x.clone()).unwrap_or_else(||
        {
            "unnamed".to_owned()
        });

        let window_info = UiWindowInfo{
            spawn_position,
            custom_buttons,
            name,
            ..Default::default()
        };

        let window = UiWindow::new(info, window_info);

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
            list: UiList::new(&mut info.creator, window.panel, 1.0 - window.button_width, on_change),
            window
        };

        this.full_update(&mut info.creator, owner);

        this
    }

    pub fn body(&self) -> Entity
    {
        self.inventory
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        self.window.in_render_order(&mut f);
        self.list.in_render_order(f);
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
        self.update_inventory(creator, entity);
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
    window: UiWindow,
    anatomy_entities: Vec<Entity>
}

impl UiAnatomy
{
    fn new(
        common_info: &mut CommonWindowInfo,
        spawn_position: Vector2<f32>,
        entity: Entity
    ) -> Self
    {
        let ui = common_info.ui.borrow();
        let anatomy_locations = &ui.anatomy_locations;

        let window_info = UiWindowInfo{
            name: "anatomy".to_owned(),
            spawn_position,
            size: Vector2::new(WINDOW_WIDTH, WINDOW_WIDTH / anatomy_locations.aspect),
            ..Default::default()
        };

        drop(ui);

        let window = UiWindow::new(common_info, window_info);

        let ui = &common_info.ui;
        let ui_ref = ui.borrow();
        let anatomy_locations = &ui_ref.anatomy_locations;

        let anatomy_entities = HumanPartId::iter().map(|id|
        {
            (id, &anatomy_locations.locations[&id])
        }).map(|(id, location)|
        {
            let ui = ui.clone();
            let mut lazy_mix = LazyMix::ui_color([0.4; 3]);
            lazy_mix.target.keep_transparency = true;

            common_info.creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale: Vector3::repeat(0.95),
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    lazy_mix: Some(lazy_mix),
                    ui_element: Some(UiElement{
                        kind: UiElementType::Button(ButtonEvents{
                            on_hover: Box::new(move |entities, _position|
                            {
                                ui.borrow_mut().update_tooltip(
                                    entities,
                                    TooltipCreateInfo::Anatomy{entity, id}
                                );
                            }),
                            ..Default::default()
                        }),
                        shape: UiElementShape::Mask(location.mask.clone()),
                        ..Default::default()
                    }),
                    parent: Some(Parent::new(window.panel, true)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::TextureId{id: location.id}.into()),
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            )
        }).collect::<Vec<_>>();

        Self{
            window,
            anatomy_entities
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        self.window.in_render_order(&mut f);
        self.anatomy_entities.iter().copied().for_each(f);
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
    }
}

pub struct UiStats
{
    window: UiWindow,
    temp: Entity
}

impl UiStats
{
    fn new(
        common_info: &mut CommonWindowInfo,
        spawn_position: Vector2<f32>,
        entity: Entity
    ) -> Self
    {
        let window_info = UiWindowInfo{
            name: "stats".to_owned(),
            spawn_position,
            ..Default::default()
        };

        let window = UiWindow::new(common_info, window_info);

        let padding = 0.05;

        let description = format!("this will have stats later :)");

        let temp = common_info.creator.push(
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
                    font_size: 15,
                    font: FontStyle::Bold,
                    align: TextAlign::default()
                }.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        Self{
            window,
            temp
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        self.window.in_render_order(&mut f);
        f(self.temp);
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
    }
}

pub struct UiItemInfo
{
    window: UiWindow,
    description_entity: Entity
}

impl UiItemInfo
{
    fn new(
        common_info: &mut CommonWindowInfo,
        spawn_position: Vector2<f32>,
        item: Item
    ) -> Self
    {
        let items_info = common_info.ui.borrow().items_info.clone();
        let info = items_info.get(item.id);

        let title = format!("info about - {}", info.name);

        let window_info = UiWindowInfo{
            name: title,
            spawn_position,
            ..Default::default()
        };

        let window = UiWindow::new(common_info, window_info);

        let padding = 0.05;

        let description = format!(
            "{} weighs around {} kg\nand is about {} meters in size!\nbla bla bla",
            info.name,
            info.mass,
            info.scale
        );

        let description_entity = common_info.creator.push(
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
                    font_size: 15,
                    font: FontStyle::Bold,
                    align: TextAlign::default()
                }.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        Self{
            window,
            description_entity
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        self.window.in_render_order(&mut f);
        f(self.description_entity);
    }

    pub fn body(&self) -> Entity
    {
        self.window.body
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

fn close_ui(entities: &ClientEntities, entity: Entity)
{
    let current_scale;
    {
        current_scale = some_or_return!(entities.transform(entity)).scale;

        let mut lazy = some_or_return!(entities.lazy_transform_mut(entity));
        lazy.target().scale = Vector3::zeros();
    }

    entities.for_every_child(entity, |entity|
    {
        if let Some(mut render) = entities.render_mut(entity)
        {
            let _ = render.set_text_dynamic_scale(Some(current_scale.xy()));
        }
    });

    let watchers = entities.watchers_mut(entity);
    if let Some(mut watchers) = watchers
    {
        let watcher = Watcher{
            kind: WatcherType::Lifetime(CLOSED_LIFETIME.into()),
            action: WatcherAction::Remove,
            ..Default::default()
        };

        watchers.push(watcher);
    }
}

fn create_notification_body(
    info: &mut CommonWindowInfo,
    entity: Entity,
    color: [f32; 3]
) -> Entity
{
    let position = info.creator.entities.transform(entity).map(|x| x.position).unwrap_or_default();
    let scale = Vector3::new(NOTIFICATION_WIDTH, NOTIFICATION_HEIGHT, 1.0);

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
            mix: Some(MixColor{color, amount: 1.0, keep_transparency: true}),
            z_level: ZLevel::Ui,
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

struct UiBarInfo
{
    pub color: [f32; 3],
    pub font_size: u32,
    pub smoothing: bool,
    pub z_level: ZLevel
}

impl Default for UiBarInfo
{
    fn default() -> Self
    {
        Self{
            color: DEFAULT_COLOR,
            font_size: 50,
            smoothing: false,
            z_level: ZLevel::Ui
        }
    }
}

#[derive(Debug, Clone)]
struct UiBar
{
    body: Entity,
    bar: Entity,
    text_entity: Entity,
    smoothing: bool
}

impl UiBar
{
    pub fn with_body(
        creator: &mut EntityCreator,
        body: Entity,
        name: String,
        info: UiBarInfo
    ) -> Self
    {
        let bar_z_level = info.z_level;
        let bar = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: if info.smoothing
                    {
                        Scaling::EaseOut{decay: 16.0}
                    } else
                    {
                        Scaling::Instant
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                mix: Some(MixColor{color: info.color, amount: 1.0, keep_transparency: true}),
                z_level: bar_z_level,
                ..Default::default()
            }
        );

        let text_entity = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text: name,
                    font_size: info.font_size,
                    font: FontStyle::Bold,
                    align: TextAlign::centered()
                }.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        Self{
            body,
            bar,
            text_entity,
            smoothing: info.smoothing
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.body);
        f(self.bar);
        f(self.text_entity);
    }

    pub fn set_amount(
        &self,
        entities: &ClientEntities,
        amount: f32
    )
    {
        let amount = amount.clamp(0.0, 1.0);

        some_or_return!(entities.target(self.bar)).scale.x = amount;

        self.update_scale(entities, amount);
    }

    fn update_scale(&self, entities: &ClientEntities, current: f32)
    {
        let mut target = some_or_return!(entities.target(self.bar));

        target.position.x = -0.5 + current / 2.0;
    }

    pub fn update(&self, entities: &ClientEntities)
    {
        if self.smoothing
        {
            let amount = {
                let global_scale = &mut some_or_return!(entities.transform_mut(self.bar)).scale.x;

                let parent_transform = some_or_return!(entities.parent_transform(self.bar));

                if *global_scale > parent_transform.scale.x
                {
                    *global_scale = parent_transform.scale.x;
                }

                let lazy = some_or_return!(entities.lazy_transform(self.bar));
                let global_scale_target = lazy.target_global(Some(&parent_transform)).scale.x;

                *global_scale * lazy.target_local.scale.x / global_scale_target
            };

            self.update_scale(entities, amount);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity
{
    Normal,
    DamageMinor,
    Damage,
    DamageMajor
}

impl NotificationSeverity
{
    pub fn color(self) -> [f32; 3]
    {
        match self
        {
            Self::Normal => DEFAULT_COLOR,
            Self::DamageMinor => [1.0, 0.727, 0.349], // wysi
            Self::Damage => [0.995, 0.367, 0.367],
            Self::DamageMajor => [0.765, 0.0, 0.423]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationId(usize);

pub struct BarNotification
{
    body: Entity,
    bar: UiBar
}

impl BarNotification
{
    fn new(
        info: &mut CommonWindowInfo,
        owner: Entity,
        name: String,
        color: [f32; 3],
        amount: f32
    ) -> Self
    {
        let body = create_notification_body(info, owner, color);

        let bar = UiBar::with_body(
            info.creator,
            body,
            name,
            UiBarInfo{color, z_level: ZLevel::Ui, ..Default::default()}
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
        self.bar.set_amount(entities, amount);
    }

    fn in_render_order(&self, f: impl FnMut(Entity))
    {
        self.bar.in_render_order(f);
    }

    pub fn update(&self, entities: &ClientEntities)
    {
        self.bar.update(entities);
    }
}

pub struct TextNotification
{
    body: Entity,
    text_entity: Entity,
    text: String
}

impl TextNotification
{
    fn new(
        info: &mut CommonWindowInfo,
        owner: Entity,
        severity: NotificationSeverity,
        text: String
    ) -> Self
    {
        let body = create_notification_body(info, owner, severity.color());

        let font_size = 35;
        let style = FontStyle::Bold;
        let align = TextAlign::centered();

        let fonts = &*info.ui.borrow().fonts;
        let size = fonts.calculate_bounds(TextInfo{
            text: &text,
            font: style,
            align,
            font_size
        });

        let width = size.x + NOTIFICATION_WIDTH * 0.1;
        info.creator.entities.target(body).unwrap().scale.x = width;

        let text_entity =info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(body, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text: text.clone(),
                    font_size,
                    font: style,
                    align
                }.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        Self{
            body,
            text_entity,
            text
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.body);
        f(self.text_entity);
    }

    pub fn text(&self) -> &str
    {
        &self.text
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

    fn in_render_order(&self, f: impl FnMut(Entity))
    {
        match self
        {
            Self::Bar(x) => x.in_render_order(f),
            Self::Text(x) => x.in_render_order(f)
        }
    }

    pub fn update(&mut self, entities: &ClientEntities)
    {
        match self
        {
            Self::Text(_) => (),
            Self::Bar(x) => x.update(entities)
        }
    }
}

pub struct Notification
{
    pub lifetime: f32,
    pub kind: NotificationKind
}

impl Notification
{
    fn in_render_order(&self, f: impl FnMut(Entity))
    {
        self.kind.in_render_order(f);
    }
}

pub struct AnatomyTooltip
{
    current: HumanPartId,
    body: Entity,
    top_panel: Entity,
    name_entity: Entity,
    bars: Vec<UiBar>
}

impl AnatomyTooltip
{
    fn new(
        info: &mut CommonWindowInfo,
        size: Vector2<f32>,
        previous_size: Option<Vector2<f32>>,
        mouse: Entity,
        entity: Entity,
        id: HumanPartId
    ) -> Self
    {
        let padding = 0.2;

        let fit = 3;

        let bars = if let HumanPartId::Eye(_) = id
        {
            vec!["eye"]
        } else
        {
            vec!["skin", "muscle", "bone"]
        };

        /*
            w is WINDOW_SIZE.y
            a is PANEL_SIZE
            p is size.y
            b is bar_size in world size
            f is fit
            d is padding

            p = bf + bd(f - 1) + wa
        */

        let panel_size = WINDOW_SIZE.y * PANEL_SIZE; // wa
        let diff = size.y - panel_size; // p - wa

        let bottom = |f: f32| f * (1.0 + padding) - padding;

        let bar_size = diff / bottom(fit as f32);

        let new_height = bar_size * bottom(bars.len() as f32) + panel_size;

        let bar_size = bar_size / new_height;

        let size = Vector2::new(size.x, new_height);

        let size3 = Vector3::new(size.x, size.y, 1.0);
        let body = info.creator.push(
            EntityInfo{
                follow_position: Some(FollowPosition{
                    parent: mouse,
                    connection: Connection::Rigid,
                    offset: Tooltip::position_offset(size),
                }),
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::EaseOut{decay: 15.0},
                    transform: Transform{
                        scale: size3,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/solid.png".to_owned()}.into()),
                mix: Some(MixColor{color: [0.2, 0.2, 0.3], amount: 1.0, keep_transparency: false}),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        let animation_strength = if let Some(size) = previous_size
        {
            Vector3::new(size.x * 0.9, size.y * 0.8, 1.0)
        } else
        {
            ANIMATION_SCALE
        };

        let mouse_position = info.creator.entities.transform(mouse).unwrap().position;
        let position = info.creator.entities.follow_position(body).unwrap().target_end(0.0, mouse_position);

        info.creator.entities.set_transform(body, Some(Transform{
            scale: size3.component_mul(&animation_strength),
            position,
            ..Default::default()
        }));

        let scale = Vector3::new(1.0, PANEL_SIZE * (WINDOW_SIZE.y / size.y), 1.0);
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
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        let scale = Vector3::new(1.0, 1.0 - scale.y, 1.0);
        let bars_panel = info.creator.push(
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
            None
        );

        let bar_size = bar_size / scale.y;

        let name_entity = info.creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                parent: Some(Parent::new(top_panel, true)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Text{
                    text: id.to_string(),
                    font_size: 20,
                    font: FontStyle::Bold,
                    align: TextAlign::centered()
                }.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        let bars = bars.iter().enumerate().map(|(index, name)|
        {
            let offset = (bar_size + padding * bar_size) * index as f32;

            let scale = Vector3::new(1.0, bar_size, 1.0);
            let body = info.creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale,
                            position: Vector3::new(0.0, -0.5 + (bar_size / 2.0) + offset, 0.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(bars_panel, true)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{name: "ui/lighter.png".to_owned()}.into()),
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            );

            UiBar::with_body(
                info.creator,
                body,
                (*name).to_owned(),
                UiBarInfo{
                    color: [0.03, 0.05, 0.1],
                    font_size: 20,
                    smoothing: true,
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            )
        }).rev().collect::<Vec<_>>();

        let this = Self{
            current: id,
            body,
            top_panel,
            name_entity,
            bars
        };

        this.update_tooltip(info.creator.entities, entity, id);

        this
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.body);
        f(self.top_panel);
        f(self.name_entity);
        self.bars.iter().for_each(|x| x.in_render_order(&mut f));
    }

    pub fn update_tooltip(
        &self,
        entities: &ClientEntities,
        entity: Entity,
        id: HumanPartId
    )
    {
        let anatomy = entities.anatomy(entity);
        let part = anatomy.as_ref().and_then(|x| x.get_human(id).unwrap());

        let hp_of = |part: Option<Health>| -> f32
        {
            part.map(|x| x.fraction()).unwrap_or_default()
        };

        self.bars.iter().enumerate().for_each(|(index, bar)|
        {
            let part = match index
            {
                0 => part.map(|x| *x.bone),
                1 => part.and_then(|x| *x.muscle),
                2 => part.and_then(|x| *x.skin),
                _ => unreachable!()
            };

            let hp = hp_of(part);

            bar.set_amount(entities, hp);
        });
    }

    pub fn update(&self, entities: &ClientEntities)
    {
        self.bars.iter().for_each(|bar| bar.update(entities));
    }

    pub fn current(&self) -> HumanPartId
    {
        self.current
    }

    pub fn body(&self) -> Entity
    {
        self.body
    }
}

pub enum TooltipKind
{
    Anatomy(AnatomyTooltip)
}

pub struct Tooltip
{
    lifetime: f32,
    kind: TooltipKind
}

impl Tooltip
{
    fn new(
        common_info: &mut CommonWindowInfo,
        mouse: Entity,
        previous_size: Option<Vector2<f32>>,
        info: TooltipCreateInfo
    ) -> Self
    {
        let size = WINDOW_SIZE.xy().component_mul(&Vector2::new(0.6, 0.5));

        let kind = match info
        {
            TooltipCreateInfo::Anatomy{entity, id} =>
            {
                TooltipKind::Anatomy(AnatomyTooltip::new(common_info, size, previous_size, mouse, entity, id))
            }
        };

        Self{
            lifetime: TOOLTIP_LIFETIME,
            kind
        }
    }

    fn in_render_order(&self, f: impl FnMut(Entity))
    {
        match &self.kind
        {
            TooltipKind::Anatomy(x) => x.in_render_order(f)
        }
    }

    pub fn size(&self, entities: &ClientEntities) -> Vector2<f32>
    {
        entities.transform(self.body()).unwrap().scale.xy()
    }

    pub fn matching_tooltip(&self, tooltip: &TooltipCreateInfo) -> bool
    {
        #[allow(unreachable_patterns)]
        match (&self.kind, tooltip)
        {
            (TooltipKind::Anatomy(x), TooltipCreateInfo::Anatomy{id, ..}) => x.current() == *id,
            _ => false
        }
    }

    pub fn update_tooltip(
        &mut self,
        entities: &ClientEntities,
        tooltip: TooltipCreateInfo
    )
    {
        debug_assert!(self.matching_tooltip(&tooltip));

        self.lifetime = TOOLTIP_LIFETIME;

        #[allow(unreachable_patterns)]
        match (&mut self.kind, tooltip)
        {
            (TooltipKind::Anatomy(x), TooltipCreateInfo::Anatomy{entity, id}) =>
            {
                x.update_tooltip(entities, entity, id);
            },
            _ => ()
        }
    }

    fn position_offset(size: Vector2<f32>) -> Vector3<f32>
    {
        let half_size = size / 2.0;
        Vector3::new(half_size.x, -half_size.y, 0.0)
    }

    pub fn update_lifetime(&mut self, dt: f32) -> bool
    {
        let needs_deletion = self.lifetime <= 0.0;
        self.lifetime -= dt;

        needs_deletion
    }

    pub fn body(&self) -> Entity
    {
        match &self.kind
        {
            TooltipKind::Anatomy(x) => x.body()
        }
    }

    pub fn update(&self, entities: &ClientEntities)
    {
        match &self.kind
        {
            TooltipKind::Anatomy(x) => x.update(entities)
        }
    }
}

struct ActionResponse
{
    button: Entity,
    text: Entity
}

impl ActionResponse
{
    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.button);
        f(self.text);
    }
}

pub struct ActionsList
{
    body: Entity,
    responses: Vec<ActionResponse>
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
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObjectKind::Texture{name: "ui/background.png".to_owned()}.into()),
                z_level: ZLevel::Ui,
                ..Default::default()
            }
        );

        info.creator.entities.target(body).unwrap().scale = scale;

        let total = responses.len();
        let responses = responses.into_iter().enumerate().map(|(index, response)|
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
                        kind: UiElementType::Button(ButtonEvents{
                            on_click: Box::new(move |_|
                            {
                                urx.borrow_mut().push(response.clone());
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "ui/lighter.png".to_owned()
                    }.into()),
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            );

            let text = info.creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo::default().into()),
                    parent: Some(Parent::new(button, true)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObjectKind::Text{
                        text: name,
                        font_size: 20,
                        font: FontStyle::Bold,
                        align: TextAlign::centered()
                    }.into()),
                    z_level: ZLevel::Ui,
                    ..Default::default()
                }
            );

            ActionResponse{button, text}
        }).collect::<Vec<_>>();

        Self{
            body,
            responses
        }
    }

    fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        f(self.body);
        self.responses.iter().for_each(|x| x.in_render_order(&mut f));
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
    Bar{name: String, color: [f32; 3], amount: f32},
    Text{severity: NotificationSeverity, text: String}
}

pub enum WindowCreateInfo
{
    ActionsList{popup_position: Vector2<f32>, responses: Vec<UserEvent>},
    Notification{owner: Entity, lifetime: f32, info: NotificationCreateInfo},
    Tooltip{closing_animation: bool, previous_size: Option<Vector2<f32>>, info: TooltipCreateInfo},
    Anatomy{spawn_position: Vector2<f32>, entity: Entity},
    Stats{spawn_position: Vector2<f32>, entity: Entity},
    ItemInfo{spawn_position: Vector2<f32>, item: Item},
    Inventory{
        spawn_position: Vector2<f32>,
        entity: Entity,
        on_click: Box<dyn FnMut(Entity, InventoryItem) -> UserEvent>
    }
}

#[derive(Debug, Clone)]
pub enum TooltipCreateInfo
{
    Anatomy{entity: Entity, id: HumanPartId}
}

pub enum UiSpecializedWindow
{
    ActionsList(ActionsList),
    Notification(Notification),
    Tooltip(Tooltip),
    Anatomy(UiAnatomy),
    Stats(UiStats),
    ItemInfo(UiItemInfo),
    Inventory(UiInventory)
}

impl UiSpecializedWindow
{
    quick_casts!{as_actions_list, as_actions_list_mut, ActionsList, ActionsList}
    quick_casts!{as_notification, as_notification_mut, Notification, Notification}
    quick_casts!{as_tooltip, as_tooltip_mut, Tooltip, Tooltip}
    quick_casts!{as_item_info, as_item_info_mut, ItemInfo, UiItemInfo}
    quick_casts!{as_inventory, as_inventory_mut, Inventory, UiInventory}

    fn body(&self) -> Entity
    {
        match self
        {
            Self::ActionsList(x) => x.body(),
            Self::Notification(x) => x.kind.body(),
            Self::Tooltip(x) => x.body(),
            Self::Anatomy(x) => x.body(),
            Self::Stats(x) => x.body(),
            Self::ItemInfo(x) => x.body(),
            Self::Inventory(x) => x.body()
        }
    }

    fn in_render_order(&self, f: impl FnMut(Entity))
    {
        match self
        {
            Self::ActionsList(x) => x.in_render_order(f),
            Self::Notification(x) => x.in_render_order(f),
            Self::Tooltip(x) => x.in_render_order(f),
            Self::Anatomy(x) => x.in_render_order(f),
            Self::Stats(x) => x.in_render_order(f),
            Self::ItemInfo(x) => x.in_render_order(f),
            Self::Inventory(x) => x.in_render_order(f)
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
            Self::Notification(x) => x.kind.update(creator.entities),
            Self::Tooltip(x) => x.update(creator.entities),
            Self::Anatomy(_) => (),
            Self::Stats(_) => (),
            Self::ItemInfo(_) => (),
            Self::Inventory(x) => x.update(creator, camera, dt)
        }
    }
}

struct ClosingWindow
{
    window: Rc<RefCell<UiSpecializedWindow>>,
    lifetime: f32
}

pub struct Ui
{
    items_info: Arc<ItemsInfo>,
    fonts: Rc<FontsContainer>,
    mouse: Entity,
    console: Entity,
    anatomy_locations: UiAnatomyLocations,
    user_receiver: Rc<RefCell<UiReceiver>>,
    notifications: HashMap<Entity, Vec<UiWindowId>>,
    active_popup: Option<UiWindowId>,
    active_tooltip: Option<UiWindowId>,
    windows_order: VecDeque<UiWindowId>,
    closing_list: Vec<ClosingWindow>,
    windows: ObjectsStore<Rc<RefCell<UiSpecializedWindow>>>
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
        let console = entities.push_eager(true, EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                scaling: Scaling::Ignore,
                rotation: Rotation::Ignore,
                transform: Transform{
                    scale: Vector3::new(1.0, 0.2, 1.0),
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            render: Some(RenderInfo{
                z_level: ZLevel::Ui,
                visibility_check: false,
                visible: false,
                ..Default::default()
            }),
            ..Default::default()
        });

        let this = Self{
            items_info,
            fonts,
            mouse,
            console,
            anatomy_locations,
            user_receiver,
            notifications: HashMap::new(),
            active_popup: None,
            active_tooltip: None,
            windows_order: VecDeque::new(),
            closing_list: Vec::new(),
            windows: ObjectsStore::new()
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

                let info = NotificationCreateInfo::Text{
                    severity,
                    text: part.to_string()
                };

                let window = WindowCreateInfo::Notification{owner: entity, lifetime: 1.0, info};

                let mut creator = EntityCreator{entities};
                Ui::add_window(ui.clone(), &mut creator, window);
            });
        }));

        this
    }

    pub fn console(&self) -> Entity
    {
        self.console
    }

    pub fn add_window<'a, 'b>(
        this: Rc<RefCell<Self>>,
        creator: &'a mut EntityCreator<'b>,
        window: WindowCreateInfo
    ) -> WindowType
    {
        let this_cloned = this.clone();

        let is_normal = match window
        {
            WindowCreateInfo::Notification{..}
            | WindowCreateInfo::Tooltip{..}
            | WindowCreateInfo::ActionsList{..} => false,
            _ => true
        };

        let post_action: Box<dyn Fn(&mut Self, &mut EntityCreator, _)> = match window
        {
            WindowCreateInfo::Notification{owner, ..} => Box::new(move |this, _creator, id|
            {
                this.notifications.entry(owner).or_insert(Vec::new());
                this.notifications.get_mut(&owner).unwrap().push(id);
            }),
            WindowCreateInfo::ActionsList{..} => Box::new(|this, creator, id|
            {
                this.close_popup(creator.entities);

                this.active_popup = Some(id);
            }),
            WindowCreateInfo::Tooltip{closing_animation, ..} => Box::new(move |this, creator, id|
            {
                if let Some(previous) = this.active_tooltip
                {
                    let remover = if closing_animation
                    {
                        Self::remove_window_id
                    } else
                    {
                        Self::remove_window_id_instant
                    };

                    let _ = remover(this, creator.entities, previous);
                }

                this.active_tooltip = Some(id);
            }),
            _ => Box::new(|_, _, _| {})
        };

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

        let mut this = this.borrow_mut();
        post_action(&mut this, creator, id);

        weak
    }

    pub fn find_window_with_body(&self, needle: Entity) -> Option<Weak<RefCell<UiSpecializedWindow>>>
    {
        self.windows.iter().find_map(|(_, window)|
        {
            (window.borrow().body() == needle).then(|| Rc::downgrade(window))
        })
    }

    pub fn remove_window(
        &mut self,
        entities: &ClientEntities,
        window: Rc<RefCell<UiSpecializedWindow>>
    ) -> Result<(), WindowError>
    {
        self.remove_window_with(entities, window, Self::remove_window_id)
    }

    pub fn remove_window_instant(
        &mut self,
        entities: &ClientEntities,
        window: Rc<RefCell<UiSpecializedWindow>>
    ) -> Result<(), WindowError>
    {
        self.remove_window_with(entities, window, Self::remove_window_id_instant)
    }

    fn remove_window_with(
        &mut self,
        entities: &ClientEntities,
        window: Rc<RefCell<UiSpecializedWindow>>,
        remover: fn(&mut Self, &ClientEntities, UiWindowId) -> Result<(), WindowError>
    ) -> Result<(), WindowError>
    {
        // why do i have to do this? i dont get it
        let found = self.windows.iter().find(|(_, x)| Rc::ptr_eq(x, &window));
        if let Some((id, _)) = found
        {
            let id = UiWindowId(id);
            remover(self, entities, id)
        } else
        {
            Err(WindowError::RemoveNonExistent)
        }
    }

    fn remove_window_id_instant(
        &mut self,
        entities: &ClientEntities,
        id: UiWindowId
    ) -> Result<(), WindowError>
    {
        self.remove_window_id_with(id, |body| entities.remove_deferred(body))?;

        Ok(())
    }

    fn remove_window_id(
        &mut self,
        entities: &ClientEntities,
        id: UiWindowId
    ) -> Result<(), WindowError>
    {
        let window = self.remove_window_id_with(id, |body| close_ui(entities, body))?;

        self.closing_list.push(ClosingWindow{window, lifetime: CLOSED_LIFETIME - LONGEST_FRAME as f32});

        Ok(())
    }

    fn remove_window_id_with(
        &mut self,
        id: UiWindowId,
        remover: impl FnOnce(Entity)
    ) -> Result<Rc<RefCell<UiSpecializedWindow>>, WindowError>
    {
        if let Some(window) = self.windows.remove(id.0)
        {
            {
                let window = window.borrow();

                match &*window
                {
                    UiSpecializedWindow::Notification(_) =>
                    {
                        self.notifications.retain(|_entity, notifications|
                        {
                            notifications.retain(|x| *x != id);

                            !notifications.is_empty()
                        });
                    },
                    UiSpecializedWindow::ActionsList(_) =>
                    {
                        assert_eq!(self.active_popup.take(), Some(id));
                    },
                    UiSpecializedWindow::Tooltip(_) =>
                    {
                        assert_eq!(self.active_tooltip.take(), Some(id));
                    },
                    UiSpecializedWindow::Anatomy(_) => (),
                    UiSpecializedWindow::Stats(_) => (),
                    UiSpecializedWindow::ItemInfo(_) => (),
                    UiSpecializedWindow::Inventory(_) => ()
                }

                let body = window.body();
                if let Some(index) = self.windows_order.iter().position(|x| *x == id)
                {
                    self.windows_order.remove(index);
                }

                remover(body);
            }

            Ok(window)
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

        let mouse = this.borrow().mouse;
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
                    NotificationCreateInfo::Bar{name, color, amount} =>
                    {
                        BarNotification::new(&mut window_info, owner, name, color, amount).into()
                    },
                    NotificationCreateInfo::Text{severity, text} =>
                    {
                        TextNotification::new(&mut window_info, owner, severity, text).into()
                    }
                };

                let notification = Notification{
                    lifetime,
                    kind
                };

                UiSpecializedWindow::Notification(notification)
            },
            WindowCreateInfo::Tooltip{closing_animation: _, previous_size, info} =>
            {
                UiSpecializedWindow::Tooltip(Tooltip::new(&mut window_info, mouse, previous_size, info))
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
        if let Some(previous) = self.active_popup
        {
            let _ = self.remove_window_id(entities, previous);
        }
    }

    pub fn update_tooltip(
        &mut self,
        entities: &ClientEntities,
        tooltip: TooltipCreateInfo
    )
    {
        let previous_size = if let Some(window_id) = self.active_tooltip.as_mut()
        {
            let mut tooltip_window = self.windows[window_id.0].borrow_mut();
            let tooltip_window = tooltip_window.as_tooltip_mut().unwrap();

            if tooltip_window.matching_tooltip(&tooltip)
            {
                tooltip_window.update_tooltip(entities, tooltip);

                return;
            } else
            {
                Some(tooltip_window.size(entities))
            }
        } else
        {
            None
        };

        let create = Rc::new(move |game_state: &mut GameState|
        {
            let tooltip = WindowCreateInfo::Tooltip{
                closing_animation: previous_size.is_none(),
                previous_size,
                info: tooltip.clone()
            };

            game_state.add_window(tooltip);
        });

        self.user_receiver.borrow_mut().push(UserEvent::UiAction(create));
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

    pub fn in_render_order(&self, mut f: impl FnMut(Entity))
    {
        self.closing_list.iter().for_each(|window| window.window.borrow().in_render_order(&mut f));

        let mut for_id = |id: &UiWindowId|
        {
            self.windows[id.0].borrow().in_render_order(&mut f);
        };

        self.notifications.iter().flat_map(|(_entity, notifications)| notifications).for_each(&mut for_id);
        self.windows_order.iter().for_each(&mut for_id);
        self.active_popup.iter().for_each(&mut for_id);
        self.active_tooltip.iter().for_each(&mut for_id);

        f(self.console);
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

        let mut to_remove = Vec::new();
        self.notifications.iter().for_each(|(_entity, notifications)|
        {
            notifications.iter().enumerate().for_each(|(index, id)|
            {
                let position = start + index as f32 * distance;

                let mut window = self.windows[id.0].borrow_mut();

                let notification = window.as_notification_mut().unwrap();

                notification.kind.set_position(creator.entities, position);

                notification.lifetime -= dt;

                if notification.lifetime <= 0.0
                {
                    to_remove.push(*id);
                }
            });
        });

        to_remove.into_iter().for_each(|id|
        {
            self.remove_window_id(creator.entities, id).unwrap();
        });

        if let Some(id) = self.active_tooltip
        {
            let mut window = self.windows[id.0].borrow_mut();
            let needs_deletion = window.as_tooltip_mut().unwrap().update_lifetime(dt);
            drop(window);

            if needs_deletion
            {
                let _ = self.remove_window_id(creator.entities, id);
            }
        }

        self.windows.iter_mut().for_each(|(_, window)|
        {
            window.borrow_mut().update(creator, camera, dt);
        });

        self.closing_list.retain_mut(|window|
        {
            window.lifetime -= dt;

            if window.lifetime <= 0.0
            {
                return false;
            }

            true
        });
    }

    pub fn ui_position(scale: Vector3<f32>, position: Vector3<f32>) -> Vector3<f32>
    {
        (position - Vector3::repeat(0.5)).component_mul(&(Vector3::repeat(1.0) - scale))
    }
}
