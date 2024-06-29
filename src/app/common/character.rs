use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{Transform, TextureId};

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
    InventoryItem,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterInfo
{
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
                parent: Some(Parent::new(entity, false)),
                lazy_transform: Some(LazyTransformInfo{
                    origin_rotation: -f32::consts::FRAC_PI_2,
                    transform: Transform{
                        rotation: f32::consts::FRAC_PI_2,
                        position: Vector3::new(1.0, 0.0, 0.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                watchers: Some(Default::default()),
                ..Default::default()
            }
        };

        let info = AfterInfo{
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

    fn update_held(&mut self, entities: &ClientEntities)
    {
        let info = some_or_return!(self.info.as_ref());

        let holding_entity = info.holding;
        let holding_right = info.holding_right;

        let mut parent = entities.parent_mut(holding_entity).unwrap();
        let mut parent_right = entities.parent_mut(holding_right).unwrap();

        parent.visible = true;
        drop(parent);

        /*
        let assets = self.game_state.assets.lock();

        if let Some(item) = self.holding.and_then(|holding| self.item_info(holding))
        {
            parent_right.visible = false;

            let texture = assets.texture(item.texture);

            let mut lazy_transform = entities.lazy_transform_mut(holding_entity).unwrap();
            let target = lazy_transform.target();

            target.scale = item.scale3();
            target.position = self.item_position(target.scale);

            let mut render = entities.render_mut(holding_entity).unwrap();
            render.set_texture(texture.clone());
        } else
        {
            let holding_left = holding_entity;

            parent_right.visible = true;

            let character_info = self.game_state.characters_info.get(self.id);

            let texture = assets.texture(player_character.hand);

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
        lazy_for(holding_right);*/
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
        characters_info: &CharactersInfo,
        transform: &mut Transform,
        render: &mut ClientRenderInfo,
        set_sprite: impl FnOnce(&mut ClientRenderInfo, &Transform, TextureId)
    ) -> bool
    {
        if !self.update_sprite_common(characters_info, transform)
        {
            return false;
        }

        let info = characters_info.get(self.id);
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

        set_sprite(render, transform, texture);

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
