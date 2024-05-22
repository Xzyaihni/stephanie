use std::sync::Arc;

use nalgebra::{Vector2, Vector3};

use yanyaengine::{Transform, game_object::*};

use crate::{
    client::ui_element::*,
    common::{
        Inventory,
        Parent,
        Entity,
        ServerToClient,
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
    background: Entity
}

impl UiScroll
{
    pub fn new(background: Entity) -> Self
    {
        Self{background}
    }
}

pub struct UiList
{
    panel: Entity,
    scroll: UiScroll,
    items: Vec<Entity>
}

impl UiList
{
    pub fn new(panel: Entity) -> Self
    {
        return Self{panel, scroll: UiScroll::new(panel), items: Vec::new()};
        let panel = todo!();
        let scroll = todo!();

        let scroll = UiScroll::new(scroll);

        Self{
            panel,
            scroll,
            items: Vec::new()
        }
    }

    pub fn set_items(
        &mut self,
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        items: impl Iterator<Item=String>
    )
    {
        self.items.iter().for_each(|x| entities.remove(*x));
        self.items.clear();

        let items: Vec<_> = items.map(|name|
        {
            let info = EntityInfo{
                lazy_transform: Some(LazyTransformInfo::default().into()),
                ui_element: Some(UiElement{
                    kind: UiElementType::Button
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObject::Text{
                        text: name,
                        font_size: 40
                    }),
                    z_level: 150
                }.server_to_client(Some(Default::default()), object_info)),
                parent: Some(Parent::new(self.panel)),
                ..Default::default()
            };

            entities.push(info)
        }).collect();

        self.items = items;

        self.update_items();
    }

    fn update_items(&mut self)
    {
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
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        items_info: Arc<ItemsInfo>,
        anchor: Entity,
        z_level: &mut i32
    ) -> Self
    {
        let mut add_ui = |parent, position, scale, ui_element, object|
        {
            *z_level += 1;

            entities.push(EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale,
                        position,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                ui_element: Some(ui_element),
                render: Some(RenderInfo{
                    object,
                    z_level: *z_level
                }.server_to_client(Some(Default::default()), object_info)),
                parent: Some(Parent::new(parent)),
                ..Default::default()
            })
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

        let top_panel = add_ui(
            inventory,
            Vector3::new(0.0, -(1.0 / 2.0 - panel_size / 2.0), 0.0),
            Vector3::new(1.0, panel_size, 1.0),
            UiElement{
                kind: UiElementType::Panel
            },
            Some(RenderObject::Texture{name: "ui/background.png".to_owned()})
        );

        let bottom_size = 1.0 - panel_size;

        let inventory_panel = add_ui(
            inventory,
            Vector3::new(0.0, 1.0 / 2.0 - bottom_size / 2.0, 0.0),
            Vector3::new(1.0, bottom_size, 1.0),
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
            list: UiList::new(inventory_panel)
        }
    }

    pub fn update_name(
        &mut self,
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        name: String
    )
    {
        let new_render = RenderObject::Text{
            text: name,
            font_size: 40
        }.into_client(Default::default(), object_info);

        if let Some(render) = entities.render_mut(self.name)
        {
            render.object = new_render;
        }
    }

    pub fn update_inventory(
        &mut self,
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        inventory: &Inventory
    )
    {
        let names = inventory.items().iter().map(|x|
        {
            self.items_info.get(x.id).name.clone()
        });

        self.list.set_items(object_info, entities, names);
    }

    pub fn update(
        &mut self,
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        name: String,
        inventory: &Inventory
    )
    {
        self.update_name(object_info, entities, name);
        self.update_inventory(object_info, entities, inventory);
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
        object_info: &mut ObjectCreateInfo,
        entities: &mut ClientEntities,
        items_info: Arc<ItemsInfo>,
        _aspect: f32
    ) -> Self
    {
        let anchor = entities.push(EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                connection: Connection::Limit{limit: 1.0},
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        let mut z_level = 100;

        let player_inventory = UiInventory::new(
            object_info,
            entities,
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
}
