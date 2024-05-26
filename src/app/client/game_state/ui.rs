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
        game_state::EntityCreator
    },
    common::{
        ease_out,
        Inventory,
        Parent,
        Entity,
        ItemsInfo,
        EntityInfo,
        RenderObject,
        RenderInfo,
        Scissor,
        lazy_transform::*,
        entity::ClientEntities
    }
};


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
                    on_change: Box::new(move |pos|
                    {
                        global_scroll.replace(1.0 - (pos.y + 0.5));
                    })
                }
            }
        };

        creator.entities.set_ui_element(background, Some(drag));

        let bar = creator.push(
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Panel
                }),
                parent: Some(Parent::new(background)),
                ..Default::default()
            },
            RenderInfo{
                object: Some(RenderObject::Texture{name: "ui/light.png".to_owned()}),
                z_level: 150,
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

        self.scroll = ease_out(self.scroll, self.target_scroll, 0.05, dt);

        self.update_position(entities);
    }

    pub fn update_size(&mut self, entities: &mut ClientEntities, size: f32)
    {
        if let Some(lazy) = entities.lazy_transform_mut(self.bar)
        {
            self.size = size;
            lazy.target().scale.y = self.size;
        }

        self.update_position(entities);
    }

    fn update_position(&mut self, entities: &mut ClientEntities)
    {
        if let Some(lazy) = entities.lazy_transform_mut(self.bar)
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
    pub fn set_scissor(&self, creator: &mut EntityCreator, scissor: Scissor)
    {
        creator.replace_scissor(self.frame, scissor.clone());
        creator.replace_scissor(self.item, scissor);
    }
}

pub struct UiList
{
    panel: Entity,
    scroll: UiScroll,
    height: f32,
    amount: usize,
    amount_changed: bool,
    current_start: usize,
    items: Vec<String>,
    frames: Vec<ListItem>
}

impl UiList
{
    pub fn new(
        creator: &mut EntityCreator,
        background: Entity
    ) -> Self
    {
        let width = 0.92;
        let panel = {
            let size = Vector3::new(width, 1.0, 1.0);

            creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            position: Ui::ui_position(size, Vector3::zeros()),
                            scale: size,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    ui_element: Some(UiElement{
                        kind: UiElementType::Panel
                    }),
                    parent: Some(Parent::new(background)),
                    ..Default::default()
                },
                RenderInfo{
                    z_level: 150,
                    ..Default::default()
                }
            )
        };

        let scroll = {
            let size = Vector3::new(1.0 - width, 1.0, 1.0);

            creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            position: Ui::ui_position(size, Vector3::new(1.0, 0.0, 0.0)),
                            scale: size,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(background)),
                    ..Default::default()
                },
                RenderInfo{
                    object: Some(RenderObject::Texture{name: "ui/light.png".to_owned()}),
                    z_level: 150,
                    ..Default::default()
                }
            )
        };

        let max_fit = 5;
        let height = 1.0 / max_fit as f32;

        let scroll = UiScroll::new(creator, scroll);

        Self{
            panel,
            scroll,
            height,
            amount: 0,
            amount_changed: true,
            current_start: 0,
            items: Vec::new(),
            frames: Self::create_items(creator, panel, max_fit)
        }
    }

    fn create_items(
        creator: &mut EntityCreator,
        parent: Entity,
        max_fit: u32
    ) -> Vec<ListItem>
    {
        (0..=max_fit).map(|_|
        {
            let id = creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo::default().into()),
                    parent: Some(Parent::new(parent)),
                    ..Default::default()
                },
                RenderInfo{
                    visible: false,
                    object: Some(RenderObject::Texture{
                        name: "ui/lighter.png".to_owned()
                    }),
                    z_level: 150,
                    ..Default::default()
                }
            );

            let text_id = creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo::default().into()),
                    parent: Some(Parent::new(id)),
                    ..Default::default()
                },
                RenderInfo{
                    object: None,
                    z_level: 151,
                    ..Default::default()
                }
            );

            ListItem{frame: id, item: text_id}
        }).collect()

    }

    pub fn set_items(
        &mut self,
        creator: &mut EntityCreator,
        items: impl Iterator<Item=String>
    )
    {
        self.items = items.collect();
        self.amount = self.items.len();

        self.update_amount(creator);
    }

    fn update_amount(&mut self, creator: &mut EntityCreator)
    {
        self.amount_changed = true;

        let size = (1.0 / self.screens_fit()).clamp(0.0, 1.0);

        self.scroll.update_size(&mut creator.entities, size);

        self.frames.iter().enumerate().for_each(|(index, item)|
        {
            if let Some(render) = creator.entities.render_mut(item.frame)
            {
                render.visible = index < self.amount;
            }
        });

        self.update_items(creator);
    }

    fn screens_fit(&self) -> f32
    {
        self.amount as f32 * self.height
    }

    fn update_items(
        &mut self,
        creator: &mut EntityCreator
    )
    {
        let last_start = self.amount as f32 - (1.0 / self.height);
        let start = self.scroll.amount() * last_start.max(0.0);

        let over_height = 1.0 / (1.0 / self.height - 1.0);

        let y = -start * over_height;
        let y_modulo = y % over_height;

        let start_item = start as usize;

        let start_changed = self.current_start != start_item;

        self.current_start = start_item;

        self.frames.iter().take(self.amount).enumerate().for_each(|(index, item)|
        {
            if start_changed || self.amount_changed
            {
                let item_index = index + start_item;

                creator.replace_object(
                    item.item,
                    RenderObject::Text{
                        text: self.items[item_index].clone(),
                        font_size: 40
                    }
                );
            }

            let transform = creator.entities.lazy_transform_mut(item.frame).unwrap().target();

            transform.scale.y = self.height * 0.9;

            transform.position.y = Ui::ui_position(
                transform.scale,
                Vector3::new(0.0, y_modulo + index as f32 * over_height, 0.0)
            ).y;
        });

        self.amount_changed = false;
    }

    pub fn update_scissors(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera
    )
    {
        let transform = creator.entities.transform(self.panel).unwrap();

        let pos = camera.screen_position(transform.position.xy());
        let pos = pos + Vector2::repeat(0.5);

        let size = camera.screen_size(transform.scale.xy());
        let pos = pos - size / 2.0;

        let scissor = Scissor{
            offset: [pos.x, pos.y],
            extent: [size.x, size.y]
        };

        self.frames.first().into_iter()
            .chain(self.frames.last())
            .for_each(|item|
            {
                item.set_scissor(creator, scissor.clone());
            });
    }

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        dt: f32
    )
    {
        self.scroll.update(&mut creator.entities, dt);
        self.update_scissors(creator, camera);
        self.update_items(creator);
    }
}

pub struct UiInventory
{
    inventory: Entity,
    items_info: Arc<ItemsInfo>,
    name: Entity,
    list: UiList
}

impl UiInventory
{
    pub fn new(
        creator: &mut EntityCreator,
        items_info: Arc<ItemsInfo>,
        anchor: Entity,
        z_level: &mut i32
    ) -> Self
    {
        let mut add_ui = |parent, position, scale, ui_element, object|
        {
            *z_level += 1;

            creator.push(
                EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale,
                            position,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    ui_element: Some(ui_element),
                    parent: Some(Parent::new(parent)),
                    ..Default::default()
                },
                RenderInfo{
                    object,
                    z_level: *z_level,
                    ..Default::default()
                }
            )
        };

        let inventory = add_ui(
            anchor,
            Vector3::zeros(),
            Vector3::new(0.4, 0.4, 1.0),
            UiElement{
                kind: UiElementType::Panel
            },
            Some(RenderObject::Texture{name: "ui/background.png".to_owned()})
        );

        let panel_size = 0.2;
        let size = Vector3::new(1.0, panel_size, 1.0);

        let top_panel = add_ui(
            inventory,
            Ui::ui_position(size, Vector3::zeros()),
            size,
            UiElement{
                kind: UiElementType::Panel
            },
            Some(RenderObject::Texture{name: "ui/background.png".to_owned()})
        );

        let size = Vector3::new(1.0, 1.0 - panel_size, 1.0);

        let inventory_panel = add_ui(
            inventory,
            Ui::ui_position(size, Vector3::new(0.0, 1.0, 0.0)),
            size,
            UiElement{
                kind: UiElementType::Panel
            },
            None
        );

        let name = add_ui(
            top_panel,
            Vector3::zeros(),
            Vector3::repeat(1.0),
            UiElement{
                kind: UiElementType::Panel
            },
            None
        );

        *z_level += 100;

        Self{
            inventory,
            items_info,
            name,
            list: UiList::new(creator, inventory_panel)
        }
    }

    pub fn update_name(
        &mut self,
        creator: &mut EntityCreator,
        name: String
    )
    {
        creator.replace_object(
            self.name,
            RenderObject::Text{
                text: name,
                font_size: 80
            }
        );
    }

    pub fn update_inventory(
        &mut self,
        creator: &mut EntityCreator,
        inventory: &Inventory
    )
    {
        let names = inventory.items().iter().map(|x|
        {
            self.items_info.get(x.id).name.clone()
        });

        self.list.set_items(creator, names);
    }

    pub fn full_update(
        &mut self,
        creator: &mut EntityCreator,
        name: String,
        inventory: &Inventory
    )
    {
        self.update_name(creator, name);
        self.update_inventory(creator, inventory);
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

pub struct Ui
{
    anchor: Entity,
    pub player_inventory: UiInventory
}

impl Ui
{
    pub fn new(
        creator: &mut EntityCreator,
        items_info: Arc<ItemsInfo>,
        _aspect: f32
    ) -> Self
    {
        let anchor = creator.entities.push(EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                connection: Connection::Limit{limit: 1.0},
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        let mut z_level = 100;

        let player_inventory = UiInventory::new(
            creator,
            items_info,
            anchor,
            &mut z_level
        ); 

        Self{
            anchor,
            player_inventory
        }
    }

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        camera: &Camera,
        player_transform: Option<Transform>,
        dt: f32
    )
    {
        let camera_size = camera.size();

        let ui_transform = creator.entities.lazy_transform_mut(self.anchor)
            .unwrap();

        let min_size = camera_size.x.min(camera_size.y);
        ui_transform.set_connection_limit(min_size * 0.3);

        let ui_target = ui_transform.target();

        let ui_scale = &mut ui_target.scale;

        ui_scale.x = camera_size.x;
        ui_scale.y = camera_size.y;

        if let Some(player_transform) = player_transform
        {
            ui_target.position = player_transform.position;
        }

        self.player_inventory.update(creator, camera, dt);
    }

    fn ui_position(scale: Vector3<f32>, position: Vector3<f32>) -> Vector3<f32>
    {
        (position - Vector3::repeat(0.5)).component_mul(&(Vector3::repeat(1.0) - scale))
    }
}
