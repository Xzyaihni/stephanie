use std::{
    f32,
    sync::Arc
};

use parking_lot::Mutex;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{Assets, Transform, TextureId};

use crate::common::{
    some_or_return,
    define_layers,
    render_info::*,
    lazy_transform::*,
    AnyEntities,
    Entity,
    EntityInfo,
    CharacterId,
    CharactersInfo,
    ItemsInfo,
    InventoryItem,
    ItemInfo,
    Parent,
    Anatomy,
    entity::ClientEntities
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteState
{
    Normal,
    Lying
}

fn true_fn() -> bool
{
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Stateful<T>
{
    #[serde(skip, default="true_fn")]
    changed: bool,
    value: T
}

impl<T> From<T> for Stateful<T>
{
    fn from(value: T) -> Self
    {
        Self{
            changed: true,
            value
        }
    }
}

impl<T> Stateful<T>
{
    pub fn set_state(&mut self, value: T)
    where
        T: PartialEq
    {
        if self.value != value
        {
            self.value = value;
            self.changed = true;
        }
    }

    pub fn value(&self) -> &T
    {
        &self.value
    }

    pub fn dirty(&mut self)
    {
        self.changed = true;
    }

    pub fn changed(&mut self) -> bool
    {
        let state = self.changed;

        self.changed = false;

        state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Faction
{
    Player,
    Zob
}

impl Faction
{
    pub fn aggressive(&self, other: &Self) -> bool
    {
        define_layers!{
            self, other,
            (Player, Player, false),
            (Zob, Zob, false),
            (Player, Zob, true)
        }
    }
}

pub const HELD_DISTANCE: f32 = 0.1;

#[derive(Clone, Copy)]
pub struct CombinedInfo<'a>
{
    pub entities: &'a ClientEntities,
    pub assets: &'a Arc<Mutex<Assets>>,
    pub items_info: &'a ItemsInfo,
    pub characters_info: &'a CharactersInfo
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterInfo
{
    this: Entity,
    holding: Entity,
    holding_right: Entity
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
    pub id: CharacterId,
    pub faction: Faction,
    pub holding: Option<InventoryItem>,
    info: Option<AfterInfo>,
    sprite_state: Stateful<SpriteState>
}

impl Character
{
    pub fn new(
        id: CharacterId,
        faction: Faction
    ) -> Self
    {
        Self{
            id,
            faction,
            info: None,
            holding: None,
            sprite_state: SpriteState::Normal.into()
        }
    }

    pub fn initialize(
        &mut self,
        entity: Entity,
        mut inserter: impl FnMut(EntityInfo) -> Entity
    )
    {
        let held_item = |flip|
        {
            EntityInfo{
                render: Some(RenderInfo{
                    object: Some(RenderObject::Texture{
                        name: "placeholder.png".to_owned()
                    }),
                    flip: if flip { Uvs::FlipHorizontal } else { Uvs::Normal },
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Arms,
                    ..Default::default()
                }),
                parent: Some(Parent::new(entity, {let reminder = ""; true})),
                lazy_transform: Some(LazyTransformInfo{
                    origin_rotation: -f32::consts::FRAC_PI_2,
                    transform: Transform{
                        rotation: f32::consts::FRAC_PI_2,
                        position: Vector3::new(1.0, {let reminder = ""; if flip {0.0} else {0.5}}, 0.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                watchers: Some(Default::default()),
                ..Default::default()
            }
        };

        let info = AfterInfo{
            this: entity,
            holding: inserter(held_item(true)),
            holding_right: inserter(held_item(false))
        };

        self.info = Some(info);
    }

    pub fn with_previous(&mut self, previous: Self)
    {
        self.sprite_state.set_state(*previous.sprite_state.value());
        self.sprite_state.dirty();
    }

    fn update_held(
        &mut self,
        combined_info: CombinedInfo
    )
    {
        return;
        let entities = &combined_info.entities;

        let info = some_or_return!(self.info.as_ref());

        let holding_entity = info.holding;
        let holding_right = info.holding_right;

        let mut parent = some_or_return!(entities.parent_mut(holding_entity));
        let mut parent_right = some_or_return!(entities.parent_mut(holding_right));

        parent.visible = true;
        drop(parent);

        let get_texture = |texture|
        {
            combined_info.assets.lock().texture(texture).clone()
        };

        if let Some(item) = self.holding.and_then(|holding| self.item_info(combined_info, holding))
        {
            parent_right.visible = false;

            let texture = get_texture(item.texture);

            let mut lazy_transform = entities.lazy_transform_mut(holding_entity).unwrap();
            let target = lazy_transform.target();

            target.scale = item.scale3();
            target.position = self.item_position(target.scale);

            let mut render = entities.render_mut(holding_entity).unwrap();
            render.set_texture(texture);
        } else
        {
            let holding_left = holding_entity;

            parent_right.visible = true;

            let character_info = combined_info.characters_info.get(self.id);

            let texture = get_texture(character_info.hand);

            let set_for = |entity, y|
            {
                let mut lazy = entities.lazy_transform_mut(entity).unwrap();
                let target = lazy.target();

                target.scale = Vector3::repeat(0.3);

                target.position = self.item_position(target.scale);
                target.position.y = y;

                let mut render = entities.render_mut(entity).unwrap();

                render.set_texture(texture.clone());
            };

            set_for(holding_left, -0.3);
            set_for(holding_right, 0.3);
        }

        drop(parent_right);

        let lazy_for = |entity|
        {
            let lazy_transform = entities.lazy_transform(entity).unwrap();

            let parent_transform = entities.parent_transform(entity);
            let new_target = lazy_transform.target_global(parent_transform.as_ref());

            let mut transform = entities.transform_mut(entity).unwrap();
            transform.scale = new_target.scale;
            transform.position = new_target.position;
        };

        lazy_for(holding_entity);
        lazy_for(holding_right);
    }

    fn item_info<'a>(
        &'a self,
        combined_info: CombinedInfo<'a>,
        id: InventoryItem
    ) -> Option<&'a ItemInfo>
    {
        self.info.as_ref().and_then(move |info|
        {
            let inventory = combined_info.entities.inventory(info.this).unwrap();
            inventory.get(id).map(|x| combined_info.items_info.get(x.id))
        })
    }

    fn held_item_position(
        &self,
        combined_info: CombinedInfo
    ) -> Option<Vector3<f32>>
    {
        let item = self.item_info(combined_info, self.holding?)?;
        let scale = item.scale3();

        Some(self.item_position(scale))
    }

    fn item_position(&self, scale: Vector3<f32>) -> Vector3<f32>
    {
        let offset = scale.y / 2.0 + 0.5 + HELD_DISTANCE;

        Vector3::new(offset, 0.0, 0.0)
    }

    pub fn update_sprite_common(
        &mut self,
        characters_info: &CharactersInfo,
        transform: &mut Transform
    ) -> bool
    {
        if !self.sprite_state.changed()
        {
            return false;
        }

        let info = characters_info.get(self.id);
        match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                transform.scale = Vector3::repeat(info.scale);
            },
            SpriteState::Lying =>
            {
                transform.scale = Vector3::repeat(info.scale * 1.3);
            }
        }

        true
    }

    pub fn update_sprite(
        &mut self,
        combined_info: CombinedInfo,
        entity: Entity,
        set_sprite: impl FnOnce(TextureId)
    ) -> bool
    {
        let entities = &combined_info.entities;

        {
            let reminder = "";
            if let Some(info) = self.info.as_ref()
            {
                if let Some(mut parent) = entities.parent_mut(info.holding)
                {
                    if let Some(mut parent_right) = entities.parent_mut(info.holding_right)
                    {
                        assert!(entities.transform(parent.entity()).is_some());
                        parent.visible = false;
                        parent_right.visible = false;
                    }
                }
            }
        }

        let mut render = entities.render_mut(entity).unwrap();
        let mut target = entities.target(entity).unwrap();

        if !self.update_sprite_common(combined_info.characters_info, &mut target)
        {
            return false;
        }

        let info = combined_info.characters_info.get(self.id);
        let texture = match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                render.z_level = ZLevel::Head;

                info.normal
            },
            SpriteState::Lying =>
            {
                render.z_level = ZLevel::Feet;

                info.lying
            }
        };

        drop(render);
        drop(target);

        self.update_held(combined_info);

        set_sprite(texture);

        true
    }

    pub fn anatomy_changed(&mut self, anatomy: &Anatomy)
    {
        let can_move = anatomy.speed().is_some();

        let state = if can_move
        {
            SpriteState::Normal
        } else
        {
            SpriteState::Lying
        };

        self.set_sprite(state);
    }

    pub fn aggressive(&self, other: &Self) -> bool
    {
        self.faction.aggressive(&other.faction)
    }

    fn set_sprite(&mut self, state: SpriteState)
    {
        self.sprite_state.set_state(state);
    }
}
