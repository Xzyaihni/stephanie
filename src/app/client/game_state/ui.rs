use std::sync::Arc;

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::{
    client::{
        ui_element::*,
        game_state::EntityCreator
    },
    common::{
        Inventory,
        Parent,
        Entity,
        ItemsInfo,
        EntityInfo,
        RenderObject,
        RenderInfo,
        lazy_transform::*,
        entity::ClientEntities
    }
};


pub struct UiScroll
{
    background: Entity,
    scroll: f32
}

impl UiScroll
{
    pub fn new(background: Entity) -> Self
    {
        Self{background, scroll: 0.0}
    }

    pub fn amount(&self) -> f32
    {
        self.scroll
    }
}

pub struct UiList
{
    panel: Entity,
    scroll: UiScroll,
    height: f32,
    frames: Vec<Entity>,
    items: Vec<Entity>
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

            let info = EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        position: Ui::ui_position(size, Vector3::zeros()),
                        scale: size,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Button
                }),
                render: Some(creator.to_client(RenderInfo{
                    object: None,
                    z_level: 150
                })),
                parent: Some(Parent::new(background)),
                ..Default::default()
            };

            creator.push(info)
        };

        let scroll = {
            let size = Vector3::new(1.0 - width, 1.0, 1.0);

            let info = EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        position: Ui::ui_position(size, Vector3::new(1.0, 0.0, 0.0)),
                        scale: size,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Button
                }),
                render: Some(creator.to_client(RenderInfo{
                    object: Some(RenderObject::Texture{name: "ui/light.png".to_owned()}),
                    z_level: 150
                })),
                parent: Some(Parent::new(background)),
                ..Default::default()
            };

            creator.push(info)
        };

        let scroll = UiScroll::new(scroll);

        Self{
            panel,
            scroll,
            height: 1.0 / 5.0,
            frames: Vec::new(),
            items: Vec::new()
        }
    }

    pub fn set_items(
        &mut self,
        creator: &mut EntityCreator,
        items: impl Iterator<Item=String>
    )
    {
        self.items.iter().chain(&self.frames).for_each(|x| creator.entities.remove(*x));
        self.items.clear();

        let frames: Vec<_> = items.map(|name|
        {
            let info = EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Button
                }),
                render: Some(creator.to_client(RenderInfo{
                    object: Some(RenderObject::Texture{
                        name: "ui/lighter.png".to_owned()
                    }),
                    z_level: 150
                })),
                parent: Some(Parent::new(self.panel)),
                ..Default::default()
            };

            let id = creator.push(info);

            let info = EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Panel
                }),
                render: Some(creator.to_client(RenderInfo{
                    object: Some(RenderObject::Text{
                        text: name,
                        font_size: 40
                    }),
                    z_level: 151
                })),
                parent: Some(Parent::new(id)),
                ..Default::default()
            };

            let text_id = creator.push(info);

            self.items.push(text_id);

            id
        }).collect();

        self.frames = frames;

        self.update_items(creator);
    }

    fn update_items(
        &mut self,
        creator: &mut EntityCreator
    )
    {
        let start = self.scroll.amount();

        let over_height = 1.0 / (1.0 / self.height - 1.0);

        self.frames.iter().enumerate().for_each(|(index, item)|
        {
            let transform = creator.entities.lazy_transform_mut(*item).unwrap().target();

            transform.scale.y = self.height * 0.9;

            transform.position.y = Ui::ui_position(
                transform.scale,
                Vector3::new(0.0, index as f32 * over_height - start, 0.0)
            ).y;
        });
    }
}

pub struct UiInventory
{
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

            let info = EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(ui_element),
                render: Some(creator.to_client(RenderInfo{
                    object,
                    z_level: *z_level
                })),
                parent: Some(Parent::new(parent)),
                ..Default::default()
            };

            creator.push(info)
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
        let new_render = creator.to_client_object(RenderObject::Text{
            text: name,
            font_size: 40
        });

        if let Some(render) = creator.entities.render_mut(self.name)
        {
            render.object = new_render;
        }
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

    pub fn update(
        &mut self,
        creator: &mut EntityCreator,
        name: String,
        inventory: &Inventory
    )
    {
        self.update_name(creator, name);
        self.update_inventory(creator, inventory);
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
        let anchor = creator.push(EntityInfo{
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
        entities: &mut ClientEntities,
        player_transform: Option<Transform>,
        camera_size: Vector2<f32>
    )
    {
        let ui_transform = entities.lazy_transform_mut(self.anchor)
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

        entities.update_ui(camera_size);
    }

    fn ui_position(scale: Vector3<f32>, position: Vector3<f32>) -> Vector3<f32>
    {
        (position - Vector3::repeat(0.5)).component_mul(&(Vector3::repeat(1.0) - scale))
    }
}
