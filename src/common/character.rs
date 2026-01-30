use std::{
    f32,
    mem,
    iter,
    cell::Ref,
    sync::Arc,
    ops::Index
};

use parking_lot::Mutex;

use serde::{Serialize, Deserialize};

use strum::{IntoEnumIterator, EnumIter, EnumCount};

use nalgebra::{Unit, Vector2, Vector3};

use yanyaengine::{Assets, Transform};

use crate::{
    client::{
        CommonTextures,
        ConnectionsHandler
    },
    common::{
        with_z,
        some_or_unexpected_return,
        some_or_return,
        some_or_value,
        some_or_false,
        define_layers,
        angle_between,
        opposite_angle,
        short_rotation,
        angle_to_direction_3d,
        ease_out,
        random_f32,
        inventory_remove_item_with,
        damage_durability_with,
        ENTITY_PIXEL_SCALE,
        ENTITY_SCALE,
        render_info::*,
        lazy_transform::*,
        collider::*,
        watcher::*,
        damage::*,
        damaging::*,
        raycast::*,
        physics::*,
        item::*,
        anatomy::*,
        clothing::EquipSlot,
        Sprite,
        Side1d,
        Side2d,
        AnyEntities,
        Entity,
        EntityInfo,
        CharacterId,
        CharactersInfo,
        Light,
        InventoryItem,
        ItemInfo,
        ItemsInfo,
        Parent,
        World,
        characters_info::*,
        player::StatId,
        entity::ClientEntities
    }
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EquipState
{
    Held,
    Equipped
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteState
{
    Normal,
    Crawling,
    Lying
}

impl<T> Index<SpriteState> for CharacterSprites<T>
{
    type Output = T;

    fn index(&self, index: SpriteState) -> &Self::Output
    {
        match index
        {
            SpriteState::Normal => &self.base,
            SpriteState::Crawling => &self.crawling,
            SpriteState::Lying => &self.lying
        }
    }
}

fn true_fn() -> bool
{
    true
}

fn hair_offset_of(offset: Vector2<i8>, pixel_offset: Vector2<f32>) -> Vector3<f32>
{
    let combined_offset = (offset.cast() / ENTITY_PIXEL_SCALE as f32 * ENTITY_SCALE) + pixel_offset;

    with_z(combined_offset, 0.0)
}

fn base_hair_z(state: SpriteState, is_player: bool) -> ZLevel
{
    match state
    {
        SpriteState::Normal => if is_player { ZLevel::PlayerHair } else { ZLevel::Hair },
        SpriteState::Crawling | SpriteState::Lying => if is_player { ZLevel::PlayerHairLying } else { ZLevel::HairLying }
    }
}

fn accessory_hair_z(state: SpriteState, is_player: bool) -> ZLevel
{
    match state
    {
        SpriteState::Normal => if is_player { ZLevel::PlayerHairAccessory } else { ZLevel::HairAccessory },
        SpriteState::Crawling | SpriteState::Lying => if is_player { ZLevel::PlayerHairAccessoryLying } else { ZLevel::HairAccessoryLying }
    }
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
pub enum CharacterAction
{
    Throw{state: bool, target: Vector3<f32>},
    Poke{state: bool},
    Bash,
    Ranged{state: bool, target: Vector3<f32>}
}

pub const DEFAULT_HELD_DISTANCE: f32 = 0.1;
pub const POKE_DISTANCE: f32 = 0.75;

#[derive(Clone, Copy)]
pub struct PartialCombinedInfo<'a>
{
    pub world: &'a World,
    pub passer: &'a ConnectionsHandler,
    pub assets: &'a Arc<Mutex<Assets>>,
    pub common_textures: &'a CommonTextures,
    pub items_info: &'a ItemsInfo,
    pub characters_info: &'a CharactersInfo
}

impl<'a> PartialCombinedInfo<'a>
{
    pub fn to_full(
        self,
        entities: &'a ClientEntities
    ) -> CombinedInfo<'a>
    {
        CombinedInfo{
            entities,
            world: self.world,
            assets: self.assets,
            passer: self.passer,
            common_textures: self.common_textures,
            items_info: self.items_info,
            characters_info: self.characters_info
        }
    }
}

#[derive(Clone, Copy)]
pub struct CombinedInfo<'a>
{
    pub passer: &'a ConnectionsHandler,
    pub entities: &'a ClientEntities,
    pub world: &'a World,
    pub assets: &'a Arc<Mutex<Assets>>,
    pub common_textures: &'a CommonTextures,
    pub items_info: &'a ItemsInfo,
    pub characters_info: &'a CharactersInfo
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSyncInfo
{
    pub rotation: f32
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, EnumIter, EnumCount)]
enum BufferedAction
{
    Bash = 0,
    Poke,
    Aim,
    Throw
}

// a spontaneous blink has about a 100ms down time and 250ms up time
// the median blinking rate is 11.1 per minute
#[derive(Debug, Clone)]
struct BlinkingInfo
{
    next_blink: f32,
    blink_length: f32,
    value: f32
}

impl BlinkingInfo
{
    fn next_blink() -> f32
    {
        60.0 / random_f32(8.0..=12.0)
    }

    fn update(&mut self, dt: f32)
    {
        self.value += dt;

        if self.value > self.next_blink
        {
            self.value -= self.next_blink;
            self.next_blink = Self::next_blink();
        }
    }

    fn is_closed(&self) -> bool
    {
        self.value < self.blink_length
    }
}

#[derive(Debug, Clone)]
struct HairInfo
{
    base: Option<Entity>,
    other: Vec<(BaseHair<Vector2<f32>>, Entity)>
}

#[derive(Debug, Clone)]
struct AfterInfo
{
    this: Entity,
    hand_left: Entity,
    hand_right: Entity,
    holding: Option<Entity>,
    hair: HairInfo,
    clothing: CharacterEquips<Option<Entity>>,
    rotation: f32,
    moving: bool,
    sprint_await: bool,
    blinking: BlinkingInfo,
    last_held_item: Option<Option<ItemId>>,
    buffered: [f32; BufferedAction::COUNT]
}

#[derive(Default, Debug, Clone)]
struct CachedInfo
{
    pub bash_distance: f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum AttackState
{
    None,
    Poke,
    Aim,
    Throw
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CharacterEquips<T>
{
    pub head: T
}

impl<T> CharacterEquips<T>
{
    pub fn iter(&self) -> impl Iterator<Item=&T>
    {
        let mut index = 0;
        iter::from_fn(move ||
        {
            let value = match index
            {
                0 => Some(&self.head),
                _ => None
            };

            index += 1;

            value
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
    pub id: CharacterId,
    pub faction: Faction,
    sprinting: bool,
    jiggle: f32,
    holding: Option<InventoryItem>,
    equips: CharacterEquips<Option<InventoryItem>>,
    hands_infront: bool,
    #[serde(skip, default)]
    cached: CachedInfo,
    attack_state: AttackState,
    #[serde(skip, default)]
    info: Option<AfterInfo>,
    held_update: bool,
    clothing_update: bool,
    attack_cooldown: f32,
    knockback_recovery: f32,
    bash_side: Side1d,
    #[serde(skip, default)]
    actions: Vec<CharacterAction>,
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
            sprinting: false,
            jiggle: 0.0,
            info: None,
            holding: None,
            equips: CharacterEquips::default(),
            hands_infront: false,
            cached: CachedInfo::default(),
            attack_state: AttackState::None,
            held_update: true,
            clothing_update: false,
            attack_cooldown: 0.0,
            knockback_recovery: 1.0,
            bash_side: Side1d::Left,
            actions: Vec::new(),
            sprite_state: SpriteState::Normal.into()
        }
    }

    fn default_connection() -> Connection
    {
        Connection::EaseOut{
            decay: 30.0,
            limit: Some(LimitMode::Normal(ENTITY_SCALE * 0.5))
        }
    }

    fn default_lazy_rotation() -> Rotation
    {
        Rotation::EaseOut(
            EaseOutRotation{
                decay: 7.0,
                speed_significant: 10.0,
                momentum: 0.5
            }.into()
        )
    }

    fn fast_lazy_rotation() -> Rotation
    {
        Rotation::EaseOut(EaseOutRotation{
            decay: 15.0,
            speed_significant: 10.0,
            momentum: 0.5
        }.into())
    }

    pub fn initialize(
        &mut self,
        entities: &ClientEntities,
        entity: Entity
    )
    {
        let inserter = |info|
        {
            entities.push(true, info)
        };

        let rotation = some_or_return!(entities.transform(entity).map(|x| x.rotation));

        let is_player = entities.player_exists(entity);

        let data_infos = entities.infos();

        let character_info = data_infos.characters_info.get(self.id);

        let needs_holding = is_player;

        let hand_item = data_infos.items_info.get(character_info.hand);

        let held_item = |parent: Option<Entity>, flip: bool|
        {
            let mut scale = hand_item.scale3();

            if flip
            {
                scale.x = -scale.x;
            }

            let held = parent.is_some();
            let follow_rotation = parent.map(|parent|
            {
                FollowRotation{
                    parent,
                    rotation: Rotation::Instant
                }
            });

            EntityInfo{
                render: Some(RenderInfo{
                    object: Some(RenderObject{
                        kind: RenderObjectKind::TextureId{
                            id: hand_item.texture.id
                        }
                    }),
                    z_level: if held { ZLevel::Held } else { if flip { ZLevel::HandLow } else { ZLevel::HandHigh } },
                    visible: !held,
                    ..Default::default()
                }),
                parent: Some(Parent::new(entity)),
                follow_rotation,
                lazy_transform: Some(LazyTransformInfo{
                    connection: if held { Connection::Ignore } else { Self::default_connection() },
                    rotation: if held { Rotation::Ignore } else { Self::default_lazy_rotation() },
                    scaling: if held { Scaling::EaseOut{decay: 16.0} } else { Scaling::Instant },
                    deformation: Deformation::Stretch(
                        StretchDeformation{
                            animation: ValueAnimation::EaseOut(1.1),
                            limit: 1.3,
                            onset: 0.5,
                            strength: 0.5
                        }
                    ),
                    origin_rotation: -f32::consts::FRAC_PI_2,
                    transform: Transform{
                        rotation: f32::consts::FRAC_PI_2,
                        scale,
                        ..Default::default()
                    },
                    unscaled_position: true,
                    inherit_scale: false,
                    ..Default::default()
                }.into()),
                ..Default::default()
            }
        };

        let base_hair = |(hair_sprite, pixel_offset): (HairSprite<Sprite>, Vector2<f32>)|
        {
            let texture = hair_sprite.sprite;

            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    deformation: CHARACTER_DEFORMATION,
                    transform: Transform{
                        position: hair_offset_of(hair_sprite.offset, pixel_offset),
                        scale: with_z(texture.scale, ENTITY_SCALE * 0.1),
                        ..Default::default()
                    },
                    unscaled_position: true,
                    inherit_scale: false,
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(entity)),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::TextureId{
                        id: texture.id
                    }.into()),
                    z_level: base_hair_z(*self.sprite_state.value(), is_player),
                    ..Default::default()
                }),
                ..Default::default()
            }
        };

        let pon = |texture: Sprite, position: Vector3<f32>|
        {
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    connection: Connection::Spring(
                        SpringConnection{
                            physical: PhysicalProperties{
                                inverse_mass: 0.01_f32.recip(),
                                floating: true,
                                damping: 0.02,
                                ..Default::default()
                            }.into(),
                            limit: LimitMode::Normal(0.005),
                            strength: 100.0
                        }
                    ),
                    rotation: Rotation::EaseOut(
                        EaseOutRotation{
                            decay: 25.0,
                            speed_significant: 3.0,
                            momentum: 0.5
                        }.into()
                    ),
                    deformation: Deformation::Stretch(
                        StretchDeformation{
                            animation: ValueAnimation::EaseOut(1.1),
                            limit: 1.4,
                            onset: 0.3,
                            strength: 0.5
                        }
                    ),
                    transform: Transform{
                        scale: with_z(texture.scale, ENTITY_SCALE * 0.1),
                        position,
                        ..Default::default()
                    },
                    unscaled_position: true,
                    inherit_scale: false,
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(entity)),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::TextureId{
                        id: texture.id
                    }.into()),
                    z_level: accessory_hair_z(*self.sprite_state.value(), is_player),
                    ..Default::default()
                }),
                ..Default::default()
            }
        };

        let hair = {
            let hairstyle = character_info.hairstyle;

            let base = hairstyle.base.as_ref().map(|base|
            {
                inserter(base_hair(self.hair_size_select(character_info, base, |x| x.sprite.scale)))
            });

            let other = hairstyle.accessory.map(|accessory|
            {
                fn create_accessory(
                    f: impl FnOnce(Vector3<f32>) -> Entity,
                    (state, offset): (SpriteState, BaseHair<Vector2<f32>>)
                ) -> (BaseHair<Vector2<f32>>, Entity)
                {
                    let this_offset = match state
                    {
                        SpriteState::Normal => offset.base,
                        SpriteState::Crawling => offset.crawling,
                        SpriteState::Lying => offset.lying
                    };

                    (offset, f(with_z(this_offset, 0.0)))
                }

                let get_offset = |offset: BaseHair<Vector2<i8>>, texture: Sprite| -> (SpriteState, BaseHair<Vector2<f32>>)
                {
                    let f = |a: Vector2<i8>, b: Sprite| -> Vector2<f32>
                    {
                        hair_offset_of(a, (texture.scale - b.scale) * 0.5).xy()
                    };

                    let offsets = BaseHair{
                        base: f(offset.base, character_info.normal),
                        crawling: f(offset.crawling, character_info.crawling),
                        lying: f(offset.lying, character_info.lying)
                    };

                    (*self.sprite_state.value(), offsets)
                };

                match accessory
                {
                    HairAccessory::Pons{left, right, value: texture} => {
                        vec![
                            create_accessory(|p| inserter(pon(texture, p)), get_offset(left, texture)),
                            create_accessory(|p| inserter(pon(texture, p)), get_offset(right, texture))
                        ]
                    }
                }
            }).unwrap_or_default();

            HairInfo{
                base,
                other
            }
        };

        let hand_left = inserter(held_item(None, true));
        let info = AfterInfo{
            this: entity,
            hand_left,
            hand_right: inserter(held_item(None, false)),
            holding: needs_holding.then(|| inserter(held_item(Some(hand_left), false))),
            hair,
            clothing: CharacterEquips::default(),
            rotation,
            moving: false,
            sprint_await: false,
            blinking: BlinkingInfo{
                next_blink: BlinkingInfo::next_blink(),
                blink_length: random_f32(0.150..=0.175),
                value: 0.0
            },
            last_held_item: None,
            buffered: [0.0; BufferedAction::COUNT]
        };

        if let Some(holding) = info.holding
        {
            if !entities.light_exists(entity)
            {
                entities.set_light_no_change(entity, Some(Light{source: Some(holding), ..Default::default()}));
            }
        }

        self.info = Some(info);

        if let Some(anatomy) = entities.anatomy(entity)
        {
            self.update_anatomy_dependent(entities, &anatomy);
        }
    }

    fn hair_select<'a, T>(&self, base: &'a BaseHair<T>) -> &'a T
    {
        &base[*self.sprite_state.value()]
    }

    fn hair_size_select<T: Copy>(
        &self,
        character_info: &CharacterInfo,
        base: &BaseHair<T>,
        f: impl FnOnce(&T) -> Vector2<f32>
    ) -> (T, Vector2<f32>)
    {
        let state = *self.sprite_state.value();
        let this_size = match state
        {
            SpriteState::Normal => character_info.normal.scale,
            SpriteState::Crawling => character_info.crawling.scale,
            SpriteState::Lying => character_info.lying.scale
        };

        let state = base[state];
        let pixel_offset = (f(&state) - this_size) * 0.5;

        (state, pixel_offset)
    }

    pub fn with_previous(&mut self, previous: Self)
    {
        self.sprite_state.set_state(*previous.sprite_state.value());
        self.sprite_state.dirty();

        self.info = previous.info;
    }

    pub fn push_action(&mut self, action: CharacterAction)
    {
        self.actions.push(action);
    }

    pub fn damage_held_durability(&mut self, entities: &ClientEntities)
    {
        let info = some_or_return!(self.info.as_ref());
        let held = some_or_return!(self.holding);

        damage_durability_with(entities, info.this, held, ||
        {
            self.on_removed_item(held)
        });
    }

    pub fn set_equip(&mut self, which: EquipSlot, value: Option<InventoryItem>)
    {
        match which
        {
            EquipSlot::Head => self.equips.head = value
        }

        self.clothing_update = true;
    }

    pub fn try_set_holding(&mut self, entities: &ClientEntities, holding: Option<InventoryItem>)
    {
        if let Some(holding) = holding
        {
            let info = some_or_return!(self.info.as_ref());

            let can_hold = {
                let inventory = some_or_return!(entities.inventory(info.this));
                let item = some_or_unexpected_return!(inventory.get(holding));
                some_or_return!(self.can_hold(entities, item))
            };

            if !can_hold
            {
                return;
            }
        }

        self.set_holding(holding);
    }

    pub fn set_holding(&mut self, holding: Option<InventoryItem>)
    {
        if let Some(holding) = holding
        {
            self.holding = Some(holding);
            self.update_holding();
        } else
        {
            self.unhold();
        }
    }

    pub fn unhold(&mut self)
    {
        self.holding = None;
        self.update_holding();
    }

    pub fn update_holding(&mut self)
    {
        self.held_update = true;
    }

    pub fn on_removed_item(&mut self, item: InventoryItem)
    {
        if Some(item) == self.holding
        {
            self.unhold();
        }
    }

    pub fn verify_valid(&self, entities: &ClientEntities)
    {
        #[cfg(debug_assertions)]
        {
            let info = some_or_return!(self.info.as_ref());
            let inventory = some_or_return!(entities.inventory(info.this));

            if let Some(item) = self.holding
            {
                debug_assert!(inventory.get(item).is_some());
            }
        }
    }

    pub fn newtons(&self, entities: &ClientEntities) -> Option<f32>
    {
        Some(Self::newtons_with_anatomy(&*(self.anatomy(entities)?)))
    }

    fn newtons_with_anatomy(anatomy: &Anatomy) -> f32
    {
        anatomy.strength() * 30.0
    }

    pub fn attack_cooldown(&self) -> f32
    {
        self.attack_cooldown
    }

    pub fn oxygen_fraction(&self, entities: &ClientEntities) -> Option<f32>
    {
        let anatomy = self.anatomy(entities)?;

        anatomy.oxygen().fraction()
    }

    fn held_crit_chance(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let item = self.held_item(combined_info.entities)?;

        Some(0.01 + item.crit_chance().unwrap_or(0.0))
    }

    fn random_held_crit(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let crit_chance = self.held_crit_chance(combined_info)?;

        (fastrand::f32() < crit_chance).then_some(2.0)
    }

    fn held_attack_cooldown(&self, entities: &ClientEntities) -> Option<f32>
    {
        let info = self.info.as_ref()?;

        Some(2.0 / (1.0 + entities.player(info.this).map(|x| x.get_stat(StatId::Melee).level() as f32 * 0.08).unwrap_or(0.0)))
    }

    fn bash_attack_cooldown(&self, entities: &ClientEntities) -> Option<f32>
    {
        self.held_attack_cooldown(entities).map(|x| x * 0.8)
    }

    pub fn bash_reachable(
        &self,
        this: &Transform,
        other_scale: &Vector3<f32>,
        other: &Vector3<f32>
    ) -> bool
    {
        let bash_distance = (this.scale.xy().max() + other_scale.xy().max()) * 0.5 + self.cached.bash_distance;

        let distance = this.position.metric_distance(other);

        distance <= bash_distance
    }

    fn bash_distance(&self, combined_info: CombinedInfo) -> f32
    {
        let item_info = self.held_info(combined_info);

        let item_scale = item_info.scale3().y;
        self.held_distance() + item_scale
    }

    fn this_scale(&self, characters_info: &CharactersInfo) -> Vector2<f32>
    {
        let info = characters_info.get(self.id);

        let sprite = match *self.sprite_state.value()
        {
            SpriteState::Normal => info.normal,
            SpriteState::Crawling => info.crawling,
            SpriteState::Lying => info.lying
        };

        sprite.scale
    }

    fn update_cached(&mut self, combined_info: CombinedInfo)
    {
        self.cached.bash_distance = self.bash_distance(combined_info);
    }

    fn update_held(
        &mut self,
        combined_info: CombinedInfo
    )
    {
        if *self.sprite_state.value() == SpriteState::Lying
        {
            return;
        }

        let held_item_id = self.held_item(combined_info.entities).map(|x| x.id);

        {
            let info = some_or_return!(self.info.as_mut());

            if let Some(last_held_item) = info.last_held_item.as_mut()
            {
                if *last_held_item == held_item_id
                {
                    return;
                }

                *last_held_item = held_item_id;
            }
        }

        self.attack_cooldown = 0.5;

        let info = some_or_return!(self.info.as_ref());

        let entities = &combined_info.entities;

        let this_entity = info.this;
        let holding_entity = info.holding;
        let hand_left = info.hand_left;
        let hand_right = info.hand_right;

        self.update_cached(combined_info);

        let get_texture = |texture|
        {
            combined_info.assets.lock().texture(texture).clone()
        };

        let holding_item = self.held_item_info(combined_info);
        let holding_state = holding_item.is_some();

        if let Some(holding_entity) = holding_entity
        {
            some_or_return!(entities.render_mut_no_change(holding_entity)).visible = self.held_visible(combined_info);
        }

        entities.lazy_setter.borrow_mut().set_follow_position_no_change(hand_right, holding_item.map(|_|
        {
            FollowPosition{
                parent: hand_left,
                connection: Connection::Rigid,
                offset: Vector3::new(ENTITY_SCALE * 0.1, 0.0, 0.0)
            }
        }));

        if let Some(item) = holding_item
        {
            let holding_entity = some_or_unexpected_return!(holding_entity);

            let mut light = some_or_return!(entities.light_mut_no_change(this_entity));
            light.modify_light(|light| *light = item.lighting);

            let mut lazy_transform = some_or_return!(entities.lazy_transform_mut_no_change(holding_entity));

            let texture = get_texture(item.texture.id);

            let target = lazy_transform.target();

            target.scale = item.scale3();

            drop(lazy_transform);

            let height = entities.lazy_target_end(holding_entity).unwrap().scale.y;
            entities.lazy_setter.borrow_mut().set_follow_position_no_change(holding_entity, Some(FollowPosition{
                parent: hand_left,
                connection: Connection::Rigid,
                offset: Vector3::new(0.0, -height / 2.0, 0.0)
            }));

            let mut render = entities.render_mut(holding_entity).unwrap();
            render.set_texture(texture);

            self.update_hands_rotation(combined_info);
        } else
        {
            if let Some(mut light) = entities.light_mut_no_change(this_entity)
            {
                light.modify_light(|light| *light = Light::default());
            }
        }

        some_or_return!(entities.lazy_transform_mut_no_change(hand_right)).connection = if holding_state
        {
            Connection::Ignore
        } else
        {
            Self::default_connection()
        };

        let set_for = |entity, y|
        {
            let mut lazy = entities.lazy_transform_mut_no_change(entity).unwrap();
            let target = lazy.target();

            target.position = self.held_position(combined_info.characters_info, target.scale);

            let info = combined_info.characters_info.get(self.id);
            target.position.y = y * info.normal.scale.max();
        };

        set_for(hand_left, -0.3);
        set_for(hand_right, 0.3);

        let lazy_for = |entity|
        {
            entities.end_sync(entity, |mut current, target|
            {
                current.scale = target.scale;
                current.position = target.position;
            });

            if let Some(follow) = entities.follow_position(entity)
            {
                let mut transform = entities.transform_mut(entity).unwrap();

                let parent = entities.transform(follow.parent).unwrap();
                transform.position = follow.target_end(transform.rotation, parent.position);
            }
        };

        if let Some(holding_entity) = holding_entity
        {
            lazy_for(holding_entity);
        }

        lazy_for(hand_left);
        lazy_for(hand_right);

        if !holding_state
        {
            self.forward_point(combined_info);
        }

        some_or_return!(self.info.as_mut()).last_held_item = Some(held_item_id);

        self.held_update = false;
    }

    pub fn update_clothing(&mut self)
    {
        self.clothing_update = true;
    }

    fn update_clothing_inner(
        &mut self,
        combined_info: CombinedInfo
    )
    {
        let info = some_or_return!(self.info.as_mut());

        let entities = combined_info.entities;

        let is_player = entities.player_exists(info.this);

        let state = *self.sprite_state.value();

        let inventory = some_or_return!(entities.inventory(info.this));

        let create_if = |
            slot: &mut Option<Entity>,
            equip: &mut Option<InventoryItem>,
            exists: bool
        |
        {
            let clear_slot = |slot: &mut Option<Entity>|
            {
                if let Some(slot_entity) = slot
                {
                    entities.remove_deferred(*slot_entity);
                }

                *slot = None;
            };

            let exists = exists && equip.is_some();

            if slot.is_some()
            {
                if !exists
                {
                    clear_slot(slot);
                }
            } else
            {
                let new_slot = exists.then(||
                {
                    entities.push(true, EntityInfo{
                        transform: Some(Transform::default()),
                        parent: Some(Parent::new(info.this)),
                        ..Default::default()
                    })
                });

                *slot = new_slot;
            }

            if let Some(equip_item) = *equip
            {
                if let Some(item) = inventory.get(equip_item)
                {
                    if let Some(entity) = *slot
                    {
                        let clothing = some_or_unexpected_return!(combined_info.items_info.get(item.id).clothing.as_ref());

                        let sprite = clothing.sprites[state];

                        let z_level = if state == SpriteState::Normal
                        {
                            if is_player { ZLevel::PlayerHat } else { ZLevel::Hat }
                        } else
                        {
                            if is_player { ZLevel::PlayerHatLying } else { ZLevel::HatLying }
                        };

                        let render_object = RenderObject{
                            kind: RenderObjectKind::TextureId{id: sprite.id}
                        };

                        if let Some(mut render) = entities.render_mut(entity)
                        {
                            render.set_z_level(z_level);
                            entities.set_deferred_render_object(entity, render_object);
                        } else
                        {
                            let render = RenderInfo{
                                object: Some(render_object),
                                z_level,
                                ..Default::default()
                            };

                            entities.set_render(entity, Some(render));
                        }

                        let lazy = LazyTransformInfo{
                            transform: Transform{
                                scale: with_z(sprite.scale, ENTITY_SCALE * 0.1),
                                ..Default::default()
                            },
                            deformation: CHARACTER_DEFORMATION,
                            inherit_scale: false,
                            ..Default::default()
                        }.into();

                        entities.set_lazy_transform(entity, Some(lazy));
                    }
                } else
                {
                    clear_slot(slot);
                    *equip = None;
                }
            }
        };

        create_if(&mut info.clothing.head, &mut self.equips.head, true);

        self.clothing_update = false;
    }

    fn can_throw(&self, combined_info: CombinedInfo) -> bool
    {
        self.holding.is_some() && self.can_attack(combined_info)
    }

    fn throw_start(&mut self, combined_info: CombinedInfo)
    {
        if !self.can_throw(combined_info)
        {
            self.start_buffered(BufferedAction::Throw);

            return;
        }

        self.stop_buffered(BufferedAction::Throw);

        let hand_left = some_or_return!(self.info.as_ref()).hand_left;

        let entities = combined_info.entities;

        some_or_return!(entities.lazy_transform_mut_no_change(hand_left)).rotation = Self::fast_lazy_rotation();

        self.forward_point(combined_info);

        self.attack_state = AttackState::Throw;
    }

    fn throw_held(
        &mut self,
        combined_info: CombinedInfo,
        target: Vector3<f32>
    ) -> bool
    {
        self.stop_buffered(BufferedAction::Throw);

        if self.attack_state != AttackState::Throw
        {
            return false;
        }

        if !self.can_throw(combined_info)
        {
            return false;
        }

        let entities = &combined_info.entities;
        let strength = some_or_value!(self.newtons(entities), false) * 0.2;

        if let Some(item) = self.held_item(entities)
        {
            let held = some_or_value!(self.holding.take(), false);

            let item_info = combined_info.items_info.get(item.id);
            let damage_scale = item.damage_scale().unwrap_or(1.0);

            let info = some_or_value!(self.info.as_ref(), false);

            let holding_entity = some_or_unexpected_return!(info.holding);

            let level = if let Some(player) = combined_info.entities.player(info.this)
            {
                player.get_stat(StatId::Throw).level()
            } else
            {
                0
            };

            let level_buff = 1.0 + level as f32 * 0.1;

            let collider = item_collider();

            let entity_info = {
                let holding_transform = entities.transform(holding_entity).unwrap();

                let direction = {
                    let rotation = angle_between(
                        holding_transform.position,
                        target
                    );

                    *angle_to_direction_3d(rotation)
                };

                let mut physical: Physical = item_physical(item_info).into();

                let throw_limit = 50.0 * item_info.mass * (1.0 + level as f32 * 0.01);
                let throw_amount = (strength * 2.0 * (1.0 + level as f32 * 0.5)).min(throw_limit);

                physical.add_force(direction * throw_amount);

                let damage = item_info.poke_damage() * (damage_scale * level_buff);

                EntityInfo{
                    physical: Some(physical),
                    lazy_transform: Some(item_lazy_transform(item_info, holding_transform.position, holding_transform.rotation).into()),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::TextureId{
                            id: item_info.texture.id
                        }.into()),
                        z_level: ZLevel::Elbow,
                        ..Default::default()
                    }),
                    collider: Some(collider.clone().into()),
                    light: Some(item_info.lighting),
                    item: Some(item),
                    damaging: Some(DamagingInfo{
                        damage: DamagingType::Mass(damage),
                        faction: Some(self.faction),
                        source: Some(info.this),
                        on_hit_gain: Some((StatId::Throw, 1.5)),
                        ..Default::default()
                    }.into()),
                    ..Default::default()
                }
            };

            let throw_projectile_entity = entities.push(true, entity_info);
            let disappear_watcher = item_disappear_watcher(combined_info.common_textures);

            entities.add_watcher(throw_projectile_entity, Watcher{
                kind: WatcherType::Collision,
                action: Box::new(move |entities, entity|
                {
                    entities.set_damaging(entity, None);

                    entities.set_collider(entity, Some(ColliderInfo{
                        layer: ColliderLayer::ThrownDecal,
                        ..collider
                    }.into()));

                    entities.add_watcher(entity, disappear_watcher);

                    if let Some(mut render) = entities.render_mut_no_change(entity)
                    {
                        render.set_z_level(ZLevel::BelowFeet);
                    }
                }),
                ..Default::default()
            });

            inventory_remove_item_with(entities, info.this, held, ||
            {
                self.on_removed_item(held);
            });

            self.consume_attack_oxygen(combined_info);
        }

        self.held_update = true;

        true
    }

    pub fn can_move(&self, combined_info: CombinedInfo) -> bool
    {
        self.anatomy(combined_info.entities).map(|anatomy|
        {
            anatomy.speed() != 0.0
        }).unwrap_or(true)
    }

    fn attack_oxygen_cost(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        Some(self.held_info(combined_info).oxygen_cost(self.newtons(combined_info.entities)?))
    }

    fn consume_attack_oxygen(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());
        let cost = some_or_return!(self.attack_oxygen_cost(combined_info));

        let mut anatomy = some_or_return!(combined_info.entities.anatomy_mut_no_change(info.this));

        anatomy.oxygen_mut().change(-cost);
    }

    fn attackable_state(&self) -> bool
    {
        let state = *self.sprite_state.value();

        state == SpriteState::Normal || state == SpriteState::Crawling
    }

    pub fn can_ranged(&self) -> bool
    {
        self.attackable_state()
    }

    pub fn can_attack(&self, combined_info: CombinedInfo) -> bool
    {
        let cost = some_or_value!(self.attack_oxygen_cost(combined_info), false);
        let current = some_or_value!(self.anatomy(combined_info.entities), false).oxygen().current;

        let attackable_item = cost <= current;

        self.attackable_state() && attackable_item && self.attack_cooldown <= 0.0
    }

    fn anatomy<'a>(&'a self, entities: &'a ClientEntities) -> Option<Ref<'a, Anatomy>>
    {
        self.info.as_ref().and_then(move |info|
        {
            entities.anatomy(info.this)
        })
    }

    fn hand_rotation_with(&self, side: Side1d) -> f32
    {
        let edge = 0.4;

        match side
        {
            Side1d::Left =>
            {
                f32::consts::FRAC_PI_2 - edge
            },
            Side1d::Right =>
            {
                -f32::consts::FRAC_PI_2 + edge
            }
        }
    }

    fn current_hand_rotation(&self) -> f32
    {
        self.hand_rotation_with(self.bash_side)
    }

    fn update_hands_rotation(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());

        let start_rotation = some_or_return!(self.default_held_rotation(combined_info));

        let holding = if self.holding.is_some()
        {
            info.hand_left
        } else
        {
            match self.bash_side
            {
                Side1d::Left => info.hand_right,
                Side1d::Right => info.hand_left
            }
        };

        let mut lazy = some_or_return!(combined_info.entities.lazy_transform_mut_no_change(holding));
        let swing_time = some_or_return!(self.bash_attack_cooldown(combined_info.entities));

        let new_rotation = self.current_hand_rotation();

        if let Rotation::EaseOut(x) = &mut lazy.rotation
        {
            x.set_decay(70.0);
        }

        lazy.target().rotation = start_rotation - new_rotation;

        combined_info.entities.add_watcher(holding, Watcher{
            kind: WatcherType::Lifetime(0.2.into()),
            action: Box::new(|entities, entity|
            {
                if let Some(mut lazy) = entities.lazy_transform_mut(entity)
                {
                    lazy.rotation = Self::default_lazy_rotation();
                }
            }),
            ..Default::default()
        });

        if self.holding.is_some()
        {
            let holding_entity = some_or_unexpected_return!(info.holding);
            let mut target = some_or_return!(combined_info.entities.target(holding_entity));
            target.scale.x = match self.bash_side
            {
                Side1d::Left => target.scale.x.abs(),
                Side1d::Right => -target.scale.x.abs()
            };
        } else
        {
            combined_info.entities.add_watcher(holding, Watcher{
                kind: WatcherType::Lifetime((swing_time.min(1.0) * 0.8).into()),
                action: Box::new(move |entities, entity|
                {
                    if let Some(mut target) = entities.target(entity)
                    {
                        target.rotation = start_rotation;
                    }
                }),
                ..Default::default()
            });
        }

        self.hands_infront = false;
    }

    fn bash_attack(&mut self, combined_info: CombinedInfo, buffer: bool)
    {
        if !self.can_attack(combined_info)
        {
            if buffer
            {
                self.start_buffered(BufferedAction::Bash);
            }

            return;
        }

        self.stop_buffered(BufferedAction::Bash);

        self.attack_cooldown = some_or_return!(self.bash_attack_cooldown(combined_info.entities));

        self.bash_side = self.bash_side.opposite();

        self.consume_attack_oxygen(combined_info);

        self.bash_projectile(combined_info);

        self.update_hands_rotation(combined_info);
    }

    fn default_held_rotation(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let origin_rotation = combined_info.entities
            .lazy_transform(self.info.as_ref()?.holding?)?
            .origin_rotation();

        Some(-origin_rotation)
    }

    fn forward_point(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());

        let start_rotation = some_or_return!(self.default_held_rotation(combined_info));

        let f = |entity|
        {
            if let Some(mut lazy) = combined_info.entities.lazy_transform_mut_no_change(entity)
            {
                lazy.target().rotation = start_rotation;
            }
        };

        f(info.hand_left);
        f(info.hand_right);
    }

    fn clear_attack_state(&mut self, combined_info: CombinedInfo, successful: bool)
    {
        self.attack_state = AttackState::None;

        if !successful && self.holding.is_some()
        {
            self.update_hands_rotation(combined_info);
        }
    }

    fn aim_start(&mut self, combined_info: CombinedInfo)
    {
        if !self.can_ranged() || self.attack_cooldown > 0.0
        {
            self.start_buffered(BufferedAction::Aim);

            return;
        }

        self.stop_buffered(BufferedAction::Aim);

        let hand_left = some_or_return!(self.info.as_ref()).hand_left;

        let entities = combined_info.entities;

        some_or_return!(entities.lazy_transform_mut_no_change(hand_left)).rotation = Self::fast_lazy_rotation();

        self.forward_point(combined_info);

        self.attack_state = AttackState::Aim;
    }

    fn start_buffered(&mut self, action: BufferedAction)
    {
        let info = some_or_return!(self.info.as_mut());
        info.buffered[action as usize] = 0.5;
    }

    fn stop_buffered(&mut self, action: BufferedAction)
    {
        let info = some_or_return!(self.info.as_mut());
        if info.buffered[action as usize] > 0.0
        {
            info.buffered[action as usize] = 0.0;
        }
    }

    fn can_poke(&self) -> bool
    {
        self.holding.is_some()
    }

    fn poke_attack_start(&mut self, combined_info: CombinedInfo)
    {
        if !self.can_poke()
        {
            return;
        }

        if !self.can_attack(combined_info)
        {
            self.start_buffered(BufferedAction::Poke);

            return;
        }

        self.stop_buffered(BufferedAction::Poke);

        let hand_left = some_or_return!(self.info.as_ref()).hand_left;

        let entities = combined_info.entities;

        some_or_return!(entities.lazy_transform_mut_no_change(hand_left)).rotation = Self::fast_lazy_rotation();

        self.forward_point(combined_info);

        self.attack_state = AttackState::Poke;
    }

    fn poke_attack(&mut self, combined_info: CombinedInfo) -> bool
    {
        self.stop_buffered(BufferedAction::Poke);

        if !self.can_poke()
        {
            return false;
        }

        if self.attack_state != AttackState::Poke
        {
            return false;
        }

        if !self.can_attack(combined_info)
        {
            return false;
        }

        let item = some_or_false!(self.held_item(combined_info.entities));

        self.attack_cooldown = some_or_false!(self.held_attack_cooldown(combined_info.entities));

        self.consume_attack_oxygen(combined_info);

        self.poke_projectile(combined_info, item);

        let info = some_or_false!(self.info.as_ref());

        let entities = &combined_info.entities;

        let mut lazy = some_or_false!(entities.lazy_transform_mut_no_change(info.hand_left));

        let lifetime = self.attack_cooldown.min(0.5);
        let extend_time = lifetime * 0.2;

        lazy.rotation = Rotation::EaseOut(
            EaseOutRotation{
                decay: 16.0,
                speed_significant: 10.0,
                momentum: 0.5
            }.into()
        );

        let start_rotation = some_or_false!(self.default_held_rotation(combined_info));
        let current_hand_rotation = self.current_hand_rotation();

        let target = lazy.target();

        let item_position = self.held_position(combined_info.characters_info, target.scale);
        let held_position = Vector3::new(item_position.x, target.position.y, 0.0);

        target.position.x = item_position.x + POKE_DISTANCE * ENTITY_SCALE;
        target.rotation = start_rotation;

        let rotation = start_rotation - current_hand_rotation;

        let end = extend_time + extend_time;

        let this_entity = info.this;
        let current_holding = self.holding;

        entities.add_watcher(info.hand_left, Watcher{
            kind: WatcherType::Lifetime(end.into()),
            action: Box::new(move |entities, entity|
            {
                let character = some_or_return!(entities.character(this_entity));
                if character.holding() != current_holding
                {
                    return;
                }

                if let Some(mut lazy) = entities.lazy_transform_mut(entity)
                {
                    lazy.rotation = Self::default_lazy_rotation();
                }

                if let Some(mut target) = entities.target(entity)
                {
                    target.rotation = rotation;
                    target.position = held_position;
                }
            }),
            ..Default::default()
        });

        true
    }

    fn ranged_attack(
        &mut self,
        combined_info: CombinedInfo,
        target: Vector3<f32>
    ) -> bool
    {
        self.stop_buffered(BufferedAction::Aim);

        if self.attack_state != AttackState::Aim
        {
            return false;
        }

        if !self.can_ranged()
        {
            return false;
        }

        if self.attack_cooldown > 0.0
        {
            return false;
        }

        let item = some_or_false!(self.held_item(combined_info.entities));

        let items_info = combined_info.items_info;
        let ranged = some_or_false!(&items_info.get(item.id).ranged);

        self.attack_cooldown = ranged.cooldown();

        let info = some_or_false!(self.info.as_ref());

        let level_buff = if let Some(player) = combined_info.entities.player(info.this)
        {
            1.0 + player.get_stat(StatId::Ranged).level() as f32 * 0.1
        } else
        {
            1.0
        };

        let source = Some(info.this);
        let start = combined_info.entities.transform(info.this).unwrap().position;

        let damage_buff = item.damage_scale().unwrap_or(1.0);

        let damage = ranged.damage() * damage_buff * level_buff;

        let info = RaycastInfo{
            pierce: Some(damage.as_ranged_pierce()),
            pierce_scale: RaycastPierce::Density{ignore_anatomy: true},
            scale: 0.0,
            layer: ColliderLayer::Damage,
            ignore_entity: Some(info.this),
            ignore_end: true
        };

        let height = DamageHeight::random();

        let damage = DamagePartial{
            data: damage,
            height,
            poke: true
        };

        combined_info.entities.push(true, EntityInfo{
            damaging: Some(DamagingInfo{
                damage: DamagingType::Raycast{info, damage, start, target, scale_pierce: Some(ENTITY_SCALE.recip())},
                faction: Some(self.faction),
                source,
                ranged: true,
                on_hit_gain: Some((StatId::Ranged, 0.5)),
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        self.damage_held_durability(combined_info.entities);

        true
    }

    fn target_mass(&self, combined_info: CombinedInfo) -> f32
    {
        self.newtons(combined_info.entities).unwrap_or(0.0) * 0.005
    }

    fn mass_maxed(&self, combined_info: CombinedInfo, mass: f32) -> f32
    {
        let diff = self.target_mass(combined_info) - mass;

        if diff > 0.0
        {
            1.0
        } else
        {
            (1.0 + diff).max(0.1)
        }
    }

    fn melee_height(&self) -> DamageHeight
    {
        match self.sprite_state.value()
        {
            SpriteState::Normal => fastrand::choice([DamageHeight::Top, DamageHeight::Middle, DamageHeight::Middle, DamageHeight::Bottom]).unwrap(),
            SpriteState::Crawling
            | SpriteState::Lying => fastrand::choice([DamageHeight::Middle, DamageHeight::Bottom, DamageHeight::Bottom]).unwrap()
        }
    }

    pub fn remap_direction(&self, height: DamageHeight, side: Side2d) -> (DamageHeight, Side2d)
    {
        match self.sprite_state.value()
        {
            SpriteState::Normal => (height, side),
            SpriteState::Crawling
            | SpriteState::Lying =>
            {
                let new_height = match side
                {
                    Side2d::Left | Side2d::Right => DamageHeight::random(),
                    Side2d::Front => DamageHeight::Top,
                    Side2d::Back => DamageHeight::Bottom
                };

                let new_side = match side
                {
                    Side2d::Front | Side2d::Back =>
                    {
                        if let SpriteState::Crawling = self.sprite_state.value()
                        {
                            Side2d::Back
                        } else
                        {
                            Side2d::Front
                        }
                    },
                    x => x
                };

                (new_height, new_side)
            }
        }
    }

    fn bash_projectile(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());

        let hand_mass = self.hand_item_info(combined_info).mass;
        let item_info = self.held_info(combined_info);

        let damage_buff = self.held_item(combined_info.entities)
            .and_then(|x| x.damage_scale())
            .unwrap_or(1.0);

        let crit = self.random_held_crit(combined_info);

        let strength_scale = some_or_return!(self.newtons(combined_info.entities)) * 0.05;

        let mass_damage = self.mass_maxed(combined_info, item_info.mass);

        let hands_attack = self.holding.is_none();

        let level_buff = if let Some(player) = combined_info.entities.player(info.this)
        {
            let level = if hands_attack
            {
                player.get_stat(StatId::Melee).level()
            } else
            {
                player.get_stat(StatId::Melee).level() + player.get_stat(StatId::Bash).level()
            };

            1.0 + level as f32 * 0.1
        } else
        {
            1.0
        };

        let damage_scale = strength_scale * mass_damage * damage_buff * crit.unwrap_or(1.0) * level_buff;
        let damage = DamagePartial{
            data: (*item_info).clone().with_changed(|x| x.mass += hand_mass).bash_damage() * damage_scale,
            height: self.melee_height(),
            poke: false
        };

        let angle = short_rotation(opposite_angle(self.bash_side.opposite().to_angle() - f32::consts::FRAC_PI_2)) * 0.6;
        let minimum_distance = some_or_return!(combined_info.entities.transform(info.this)).scale.xy().max();

        let scale = self.this_scale(combined_info.characters_info).max() + self.cached.bash_distance * 2.0;
        let projectile_entity = combined_info.entities.push(
            true,
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale: with_z(Vector2::repeat(scale), ENTITY_SCALE),
                        ..Default::default()
                    },
                    inherit_scale: false,
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(info.this)),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Circle,
                    layer: ColliderLayer::Damage,
                    ghost: true,
                    ..Default::default()
                }.into()),
                damaging: Some(DamagingInfo{
                    damage: DamagingType::Collision{
                        angle,
                        damage
                    },
                    predicate: DamagingPredicate::ParentAngleLess{angle: f32::consts::PI, minimum_distance},
                    knockback: 1.0,
                    faction: Some(self.faction),
                    source: Some(info.this),
                    on_hit_gain: Some(if hands_attack { (StatId::Melee, 0.3) } else { (StatId::Bash, 0.5) }),
                    ..Default::default()
                }.into()),
                ..Default::default()
            }
        );

        combined_info.entities.add_watcher(projectile_entity, Watcher{
            kind: WatcherType::Lifetime(0.2.into()),
            action: Box::new(|entities, entity| entities.remove(entity)),
            ..Default::default()
        });
    }

    fn poke_projectile(&mut self, combined_info: CombinedInfo, item: Item)
    {
        let info = some_or_return!(self.info.as_ref());

        let holding_entity = some_or_unexpected_return!(info.holding);

        let hand_mass = self.hand_item_info(combined_info).mass;
        let item_info = combined_info.items_info.get(item.id);
        let mut scale = Vector3::repeat(1.0);

        let projectile_scale = POKE_DISTANCE * ENTITY_SCALE / item_info.scale3().y;
        scale.y += projectile_scale;

        let offset = projectile_scale / 2.0;

        let damage_buff = self.held_item(combined_info.entities)
            .and_then(|x| x.damage_scale())
            .unwrap_or(1.0);

        let crit = self.random_held_crit(combined_info);

        let strength_scale = some_or_return!(self.newtons(combined_info.entities)) * 0.03;

        let mass_damage = self.mass_maxed(combined_info, item_info.mass);

        let level_buff = if let Some(player) = combined_info.entities.player(info.this)
        {
            let level = player.get_stat(StatId::Melee).level() + player.get_stat(StatId::Poke).level();

            1.0 + level as f32 * 0.1
        } else
        {
            1.0
        };

        let damage_scale = strength_scale * mass_damage * damage_buff * crit.unwrap_or(1.0) * level_buff;
        let damage = DamagePartial{
            data: item_info.clone().with_changed(|x| x.mass += hand_mass).poke_damage() * damage_scale,
            height: self.melee_height(),
            poke: true
        };

        let projectile_entity = combined_info.entities.push(
            true,
            EntityInfo{
                follow_rotation: Some(FollowRotation::new(
                    holding_entity,
                    Rotation::Instant
                )),
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        position: Vector3::new(0.0, offset, 0.0),
                        scale,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(holding_entity)),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Rectangle,
                    layer: ColliderLayer::Damage,
                    ghost: true,
                    ..Default::default()
                }.into()),
                damaging: Some(DamagingInfo{
                    damage: DamagingType::Collision{
                        angle: 0.0,
                        damage
                    },
                    knockback: 2.0,
                    faction: Some(self.faction),
                    source: Some(info.this),
                    on_hit_gain: Some((StatId::Poke, 0.7)),
                    ..Default::default()
                }.into()),
                ..Default::default()
            }
        );

        combined_info.entities.add_watcher(projectile_entity, Watcher{
            kind: WatcherType::Lifetime(0.2.into()),
            action: Box::new(|entities, entity| entities.remove(entity)),
            ..Default::default()
        });
    }

    fn handle_actions(&mut self, combined_info: CombinedInfo)
    {
        if self.info.is_none()
        {
            return;
        }

        mem::take(&mut self.actions).into_iter().for_each(|action|
        {
            macro_rules! with_clear
            {
                ($state:expr) =>
                {
                    {
                        let state = $state;

                        if self.attack_cooldown <= 0.0
                        {
                            self.clear_attack_state(combined_info, state);
                        }
                    }
                }
            }

            match action
            {
                CharacterAction::Throw{state: false, ..} => self.throw_start(combined_info),
                CharacterAction::Throw{state: true, target} => with_clear!(self.throw_held(combined_info, target)),
                CharacterAction::Poke{state: false} => self.poke_attack_start(combined_info),
                CharacterAction::Poke{state: true} => with_clear!(self.poke_attack(combined_info)),
                CharacterAction::Ranged{state: false, ..} => self.aim_start(combined_info),
                CharacterAction::Ranged{state: true, target} => with_clear!(self.ranged_attack(combined_info, target)),
                CharacterAction::Bash => self.bash_attack(combined_info, true)
            }
        });
    }

    pub fn equips(&self) -> &CharacterEquips<Option<InventoryItem>>
    {
        &self.equips
    }

    fn held_item_info<'a>(
        &'a self,
        combined_info: CombinedInfo<'a>
    ) -> Option<&'a ItemInfo>
    {
        self.held_item(combined_info.entities).map(|x| combined_info.items_info.get(x.id))
    }

    fn hand_item_info<'a>(&self, combined_info: CombinedInfo<'a>) -> &'a ItemInfo
    {
        let info = combined_info.characters_info.get(self.id);

        combined_info.items_info.get(info.hand)
    }

    fn held_info<'a>(&'a self, combined_info: CombinedInfo<'a>) -> &'a ItemInfo
    {
        self.held_item_info(combined_info).unwrap_or_else(move ||
        {
            self.hand_item_info(combined_info)
        })
    }

    pub fn holding(&self) -> Option<InventoryItem>
    {
        self.holding
    }

    fn held_item(&self, entities: &ClientEntities) -> Option<Item>
    {
        let info = self.info.as_ref()?;
        let held = self.holding?;

        entities.inventory(info.this).and_then(|x| x.get(held).cloned())
    }

    fn held_visible(&self, combined_info: CombinedInfo) -> bool
    {
        *self.sprite_state.value() != SpriteState::Lying && self.held_item(combined_info.entities).is_some()
    }

    fn held_distance(&self) -> f32
    {
        let value = if *self.sprite_state.value() == SpriteState::Crawling
        {
            DEFAULT_HELD_DISTANCE - 0.1
        } else
        {
            DEFAULT_HELD_DISTANCE
        };

        value * ENTITY_SCALE
    }

    fn held_position(&self, characters_info: &CharactersInfo, scale: Vector3<f32>) -> Vector3<f32>
    {
        let offset = (self.this_scale(characters_info).y + scale.y) * 0.5 + self.held_distance();

        Vector3::new(offset, 0.0, 0.0)
    }

    pub fn mass_hold_limit(&self, entities: &ClientEntities) -> Option<f32>
    {
        Some(Self::mass_hold_limit_with_anatomy(&*(self.anatomy(entities)?)))
    }

    fn mass_hold_limit_with_anatomy(anatomy: &Anatomy) -> f32
    {
        let has_hand = |side|
        {
            anatomy.get_human::<()>(AnatomyId::Part(HumanPartId::Hand(side))).unwrap().is_some() as u32
        };

        let hand_count = has_hand(Side1d::Left) + has_hand(Side1d::Right);

        if hand_count == 0
        {
            return 0.0;
        }

        Self::newtons_with_anatomy(anatomy) * (hand_count as f32 / 2.0)
    }

    fn can_hold_with_anatomy(entities: &ClientEntities, anatomy: &Anatomy, item: &Item) -> bool
    {
        Self::mass_hold_limit_with_anatomy(anatomy) >= entities.infos().items_info.get(item.id).mass
    }

    pub fn can_hold(&self, entities: &ClientEntities, item: &Item) -> Option<bool>
    {
        Some(Self::can_hold_with_anatomy(entities, &*(self.anatomy(entities)?), item))
    }

    fn decrease_timer(time_variable: &mut f32, dt: f32) -> bool
    {
        if *time_variable > 0.0
        {
            *time_variable -= dt;

            if *time_variable <= 0.0
            {
                return true;
            }
        }

        false
    }

    fn update_attacks(
        &mut self,
        dt: f32
    )
    {
        Self::decrease_timer(&mut self.attack_cooldown, dt);
    }

    fn update_buffered(&mut self, combined_info: CombinedInfo, dt: f32)
    {
        if self.info.is_none()
        {
            return;
        }

        for action in BufferedAction::iter()
        {
            let is_buffered = {
                let buffered = &mut self.info.as_mut()
                    .expect("info must not disappear after creation")
                    .buffered[action as usize];

                *buffered = (*buffered - dt).max(0.0);

                *buffered > 0.0
            };

            if is_buffered
            {
                match action
                {
                    BufferedAction::Poke => { self.poke_attack_start(combined_info); },
                    BufferedAction::Bash => self.bash_attack(combined_info, false),
                    BufferedAction::Aim => self.aim_start(combined_info),
                    BufferedAction::Throw => self.throw_start(combined_info)
                }
            }
        }
    }

    pub fn collider_with_state(
        state: SpriteState,
        is_player: bool
    ) -> ColliderInfo
    {
        let layer = if is_player
        {
            ColliderLayer::Player
        } else
        {
            match state
            {
                SpriteState::Normal => ColliderLayer::NormalEnemy,
                SpriteState::Crawling
                | SpriteState::Lying => ColliderLayer::LyingEnemy
            }
        };

        let override_transform = match state
        {
            SpriteState::Normal => None,
            SpriteState::Crawling
            | SpriteState::Lying => Some(OverrideTransform{
                transform: Transform{
                    scale: Vector3::repeat(ENTITY_SCALE),
                    ..Default::default()
                },
                override_position: false
            })
        };

        ColliderInfo{
            kind: ColliderType::Circle,
            layer,
            override_transform,
            sleeping: true,
            ..Default::default()
        }
    }

    pub fn try_initialize(&mut self, entities: &ClientEntities, entity: Entity) -> Option<bool>
    {
        if self.info.is_none()
        {
            self.initialize(entities, entity);
            self.info.as_ref()?;

            Some(true)
        } else
        {
            Some(false)
        }
    }

    pub fn update(
        &mut self,
        combined_info: CombinedInfo,
        entity: Entity,
        dt: f32,
        mut set_sprite: impl FnMut(Entity, Sprite)
    )
    {
        let entities = combined_info.entities;

        self.handle_actions(combined_info);

        if self.held_update
        {
            self.update_held(combined_info);
        }

        if self.clothing_update
        {
            self.update_clothing_inner(combined_info);
        }

        self.update_jiggle(combined_info, dt);

        let knockback_drain = self.update_knockback(dt);

        self.update_sprint(combined_info);
        self.update_attacks(dt);

        self.update_buffered(combined_info, dt);

        {
            let is_sprinting = self.is_sprinting();

            let info = some_or_return!(self.info.as_mut());

            info.blinking.update(dt);

            if let Some(mut anatomy) = combined_info.entities.anatomy_mut_no_change(info.this)
            {
                let movement_drain = if info.moving
                {
                    let movement_cost = if is_sprinting
                    {
                        0.6
                    } else
                    {
                        0.03
                    };

                    movement_cost * if anatomy.is_crawling() { 0.9 } else { 1.0 }
                } else
                {
                    0.0
                };

                *anatomy.external_oxygen_change_mut() = -(movement_drain + knockback_drain * 0.1);

                info.moving = false;
            }
        }

        if !self.sprite_state.changed()
        {
            return;
        }

        let is_player = entities.player_exists(entity);

        let character_info = combined_info.characters_info.get(self.id);

        if let Some(hair_base) = character_info.hairstyle.base
        {
            let info = self.info.as_ref().unwrap();

            debug_assert!(info.hair.base.is_some());

            if let Some(base_entity) = info.hair.base
            {
                let (hair_sprite, pixel_offset) = self.hair_size_select(character_info, &hair_base, |x| x.sprite.scale);
                let sprite = hair_sprite.sprite;

                set_sprite(base_entity, sprite);
                entities.set_z_level(base_entity, base_hair_z(*self.sprite_state.value(), is_player));

                if let Some(mut target) = entities.target(base_entity)
                {
                    target.position = hair_offset_of(hair_sprite.offset, pixel_offset);
                }
            }
        }

        self.info.as_ref().unwrap().hair.other.iter().for_each(|(positions, accessory)|
        {
            entities.set_z_level(*accessory, accessory_hair_z(*self.sprite_state.value(), is_player));

            if let Some(mut target) = entities.target(*accessory)
            {
                target.position = with_z(*self.hair_select(positions), 0.0);
            }
        });

        let set_visible = |sprite_state: &mut Stateful<_>, entity, is_visible|
        {
            if let Some(mut render) = entities.render_mut_no_change(entity)
            {
                render.visible = is_visible;
            } else
            {
                // didnt update successfully, makes it rerun again
                sprite_state.dirty();
            }
        };

        let z_level = match self.sprite_state.value()
        {
            SpriteState::Normal => if is_player { ZLevel::PlayerHead } else { ZLevel::Head },
            SpriteState::Crawling
            | SpriteState::Lying => ZLevel::Feet
        };

        let texture = self.sprite_texture(character_info);

        entities.lazy_setter.borrow_mut().set_collider_no_change(
            entity,
            Some(Self::collider_with_state(*self.sprite_state.value(), is_player).into())
        );

        entities.set_z_level(entity, z_level);

        {
            let info = self.info.as_ref().unwrap();

            if let Some(holding) = info.holding
            {
                let visible = self.held_visible(combined_info);
                set_visible(&mut self.sprite_state, holding, visible);
            }
        }

        self.update_held(combined_info);
        self.update_clothing_inner(combined_info);
        self.update_cached(combined_info);

        if let Some(anatomy) = entities.anatomy(entity)
        {
            self.update_anatomy_dependent(entities, &anatomy)
        } else
        {
            self.sprite_state.dirty();
        }

        set_sprite(entity, texture);
    }

    pub fn facial_expression(&self, anatomy: &Anatomy) -> FacialExpression
    {
        if anatomy.is_dead()
        {
            FacialExpression::Dead
        } else if self.knockback_recovery < 1.0
        {
            FacialExpression::Hurt
        } else if anatomy.is_winded()
        {
            FacialExpression::Sick
        } else
        {
            FacialExpression::Normal
        }
    }

    pub fn anatomy_changed(&mut self, entities: &ClientEntities, anatomy: &Anatomy)
    {
        let state = if anatomy.is_conscious() && anatomy.can_move()
        {
            if anatomy.is_crawling()
            {
                SpriteState::Crawling
            } else
            {
                SpriteState::Normal
            }
        } else
        {
            SpriteState::Lying
        };

        self.set_sprite(state);

        self.update_anatomy_dependent(entities, anatomy);
    }

    fn update_anatomy_dependent(&mut self, entities: &ClientEntities, anatomy: &Anatomy)
    {
        let info = self.info.as_ref().unwrap();

        let hands_visibility = match self.sprite_state.value()
        {
            SpriteState::Normal
            | SpriteState::Crawling => true,
            SpriteState::Lying => false
        };

        let mut set_hand_visibility = |id, entity|
        {
            if let Some(mut render) = entities.render_mut_no_change(entity)
            {
                let id = AnatomyId::Part(HumanPartId::Hand(id));
                let is_visible = hands_visibility && anatomy.get_human::<()>(id).unwrap().is_some();

                render.set_visible(is_visible);
            } else
            {
                // render has to exist
                self.sprite_state.dirty();
            }
        };

        set_hand_visibility(Side1d::Left, info.hand_left);
        set_hand_visibility(Side1d::Right, info.hand_right);

        if let Some(item) = self.held_item(entities)
        {
            if !Self::can_hold_with_anatomy(entities, anatomy, &item)
            {
                self.unhold();
            }
        }
    }

    pub fn rotation_mut(&mut self) -> Option<&mut f32>
    {
        self.info.as_mut().map(|x| &mut x.rotation)
    }

    pub fn look_at(
        &mut self,
        entities: &ClientEntities,
        entity: Entity,
        look_position: Vector2<f32>
    )
    {
        let transform = some_or_return!(entities.transform(entity));

        let pos = look_position - transform.position.xy();

        let rotation = pos.y.atan2(pos.x);

        if let Some(x) = self.rotation_mut()
        {
            *x = rotation;
        }
    }

    pub fn get_equip_state(&self, id: InventoryItem) -> Option<EquipState>
    {
        if self.holding == Some(id)
        {
            Some(EquipState::Held)
        } else if self.equips.iter().any(|x| *x == Some(id))
        {
            Some(EquipState::Equipped)
        } else
        {
            None
        }
    }

    fn is_sprinting(&self) -> bool
    {
        if some_or_value!(self.info.as_ref(), false).sprint_await
        {
            return false;
        }

        self.sprinting
    }

    fn update_jiggle(&mut self, combined_info: CombinedInfo, dt: f32)
    {
        let info = some_or_return!(self.info.as_mut());
        let physical = some_or_return!(combined_info.entities.physical(info.this));
        let speed = physical.velocity().xy().magnitude() * 50.0;

        self.jiggle = (self.jiggle + dt * speed) % (2.0 * f32::consts::PI);

        if !info.moving
        {
            self.jiggle = ease_out(self.jiggle, 0.0, 10.0, dt);
        }

        let mut target = some_or_return!(combined_info.entities.target(info.this));

        target.rotation = if *self.sprite_state.value() == SpriteState::Crawling
        {
            info.rotation + self.jiggle.sin() * 0.25
        } else
        {
            info.rotation
        };
    }

    fn update_knockback(&mut self, dt: f32) -> f32
    {
        if self.knockback_recovery < 1.0
        {
            let old_value = self.knockback_recovery;
            let new_value = (self.knockback_recovery + dt).min(1.0);

            self.knockback_recovery = new_value;

            (new_value - old_value) / dt
        } else
        {
            0.0
        }
    }

    fn update_sprint(&mut self, combined_info: CombinedInfo)
    {
        let is_sprinting = self.is_sprinting();

        if is_sprinting
        {
            if self.anatomy(combined_info.entities).map(|x| x.oxygen().current <= 0.0).unwrap_or(true)
            {
                let info = some_or_return!(self.info.as_mut());

                info.sprint_await = true;
            }
        }
    }

    pub fn set_sprinting(&mut self, value: bool)
    {
        self.sprinting = value;

        if !value
        {
            let info = some_or_return!(self.info.as_mut());

            info.sprint_await = false;
        }
    }

    pub fn knockbacked(&mut self)
    {
        self.knockback_recovery = 0.0;
    }

    pub fn walk(
        &mut self,
        anatomy: &Anatomy,
        physical: &mut Physical,
        direction: Unit<Vector3<f32>>,
        dt: f32
    )
    {
        let speed = anatomy.speed();

        let is_sprinting = self.is_sprinting();

        if speed == 0.0
        {
            return;
        }

        some_or_return!(self.info.as_mut()).moving = true;

        let speed = if is_sprinting
        {
            speed * 1.8
        } else
        {
            speed
        };

        let speed = speed * (anatomy.oxygen().current / WINDED_OXYGEN).clamp(0.5, 1.0);

        let velocity = *direction * (speed * physical.inverse_mass);

        let current_velocity = physical.velocity();
        let new_velocity = (current_velocity + velocity).zip_map(&velocity, |value, limit|
        {
            let limit = limit.abs();

            value.min(limit).max(-limit)
        });

        let mut change_velocity = physical.velocity_as_force(new_velocity - current_velocity, dt);

        if !physical.floating()
        {
            change_velocity.z = 0.0;
        }

        physical.add_force(change_velocity * self.knockback_recovery);
    }

    pub fn aggressive(&self, other: &Self) -> bool
    {
        self.faction.aggressive(&other.faction)
    }

    pub fn is_blinking(&self) -> bool
    {
        self.info.as_ref().map(|x| x.blinking.is_closed()).unwrap_or(false)
    }

    pub fn visibility(&self) -> f32
    {
        match self.sprite_state.value()
        {
            SpriteState::Normal => 1.0,
            SpriteState::Crawling => 0.8,
            SpriteState::Lying => 0.5
        }
    }

    pub fn sprite_texture(&self, character_info: &CharacterInfo) -> Sprite
    {
        match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                character_info.normal
            },
            SpriteState::Crawling =>
            {
                character_info.crawling
            },
            SpriteState::Lying =>
            {
                character_info.lying
            }
        }
    }

    pub fn sprite_state(&self) -> SpriteState
    {
        *self.sprite_state.value()
    }

    fn set_sprite(&mut self, state: SpriteState)
    {
        self.sprite_state.set_state(state);
    }
}
