use std::{
    f32,
    mem,
    cell::Ref,
    borrow::Cow,
    sync::Arc
};

use parking_lot::{RwLock, Mutex};

use serde::{Serialize, Deserialize};

use strum::{IntoEnumIterator, EnumIter, EnumCount};

use nalgebra::{Unit, Vector3};

use yanyaengine::{Assets, Transform, TextureId};

use crate::{
    client::{
        CommonTextures,
        ConnectionsHandler
    },
    common::{
        some_or_return,
        some_or_value,
        some_or_false,
        define_layers,
        angle_between,
        opposite_angle,
        ENTITY_SCALE,
        render_info::*,
        lazy_transform::*,
        collider::*,
        watcher::*,
        damage::*,
        damaging::*,
        particle_creator::*,
        raycast::*,
        physics::*,
        Hairstyle,
        Side1d,
        Side2d,
        AnyEntities,
        Entity,
        EntityInfo,
        CharacterId,
        CharactersInfo,
        Light,
        ItemsInfo,
        Item,
        InventoryItem,
        ItemInfo,
        Parent,
        Anatomy,
        World,
        entity::ClientEntities
    }
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteState
{
    Normal,
    Crawling,
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
pub enum CharacterAction
{
    Throw(Vector3<f32>),
    Poke{state: bool},
    Bash,
    Ranged{state: bool, target: Vector3<f32>}
}

pub const DEFAULT_HELD_DISTANCE: f32 = 0.1;
pub const POKE_DISTANCE: f32 = 0.75;

// hands r actually 0.1 meters in size but they look too small that way
pub const HAND_SCALE: f32 = 0.3;

#[derive(Clone, Copy)]
pub struct PartialCombinedInfo<'a>
{
    pub world: &'a World,
    pub passer: &'a Arc<RwLock<ConnectionsHandler>>,
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
    pub passer: &'a Arc<RwLock<ConnectionsHandler>>,
    pub entities: &'a ClientEntities,
    pub world: &'a World,
    pub assets: &'a Arc<Mutex<Assets>>,
    pub common_textures: &'a CommonTextures,
    pub items_info: &'a ItemsInfo,
    pub characters_info: &'a CharactersInfo
}

impl CombinedInfo<'_>
{
    pub fn is_player(&self, entity: Entity) -> bool
    {
        self.entities.player_exists(entity)
    }
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
    Aim
}

#[derive(Debug, Clone)]
pub struct AfterInfo
{
    this: Entity,
    hand_left: Entity,
    hand_right: Entity,
    holding: Entity,
    hair: Vec<Entity>,
    rotation: f32,
    sprint_await: bool,
    buffered: [f32; BufferedAction::COUNT]
}

#[derive(Default, Debug, Clone)]
struct CachedInfo
{
    pub bash_distance: Option<f32>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum AttackState
{
    None,
    Poke,
    Aim
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
    pub id: CharacterId,
    pub faction: Faction,
    sprinting: bool,
    stamina: f32,
    jiggle: f32,
    holding: Option<InventoryItem>,
    hands_infront: bool,
    #[serde(skip, default)]
    cached: CachedInfo,
    attack_state: AttackState,
    #[serde(skip, default)]
    info: Option<AfterInfo>,
    #[serde(skip, default="true_fn")]
    held_update: bool,
    attack_cooldown: f32,
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
            stamina: f32::MAX,
            jiggle: 0.0,
            info: None,
            holding: None,
            hands_infront: false,
            cached: CachedInfo::default(),
            attack_state: AttackState::None,
            held_update: true,
            attack_cooldown: 0.0,
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
        entities: &impl AnyEntities,
        entity: Entity
    )
    {
        let inserter = |info|
        {
            entities.push(true, info)
        };

        let rotation = some_or_return!(entities.transform(entity).map(|x| x.rotation));

        let is_player = entities.player_exists(entity);

        let character_info = entities.infos().characters_info.get(self.id);

        let held_item = |parent: Option<Entity>, flip: bool|
        {
            let mut scale = Vector3::repeat(HAND_SCALE);

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
                            id: character_info.hand
                        }
                    }),
                    z_level: if held { ZLevel::Held } else { if flip { ZLevel::HandLow } else { ZLevel::HandHigh } },
                    ..Default::default()
                }),
                parent: Some(Parent::new(entity, !held)),
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
                    ..Default::default()
                }.into()),
                watchers: Some(Default::default()),
                ..Default::default()
            }
        };

        let mut hair = Vec::new();

        let pon = |texture, position: Vector3<f32>|
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
                            strength: 0.9
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
                        scale: Vector3::repeat(0.4),
                        position,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(entity, true)),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::TextureId{
                        id: texture
                    }.into()),
                    z_level: if is_player { ZLevel::PlayerHair } else { ZLevel::Hair },
                    ..Default::default()
                }),
                watchers: Some(Default::default()),
                ..Default::default()
            }
        };

        match character_info.hairstyle
        {
            Hairstyle::None => (),
            Hairstyle::Pons(texture) =>
            {
                hair.push(inserter(pon(texture, Vector3::new(-0.35, 0.35, 0.0))));
                hair.push(inserter(pon(texture, Vector3::new(-0.35, -0.35, 0.0))));
            }
        }

        let hand_left = inserter(held_item(None, true));
        let info = AfterInfo{
            this: entity,
            hand_left,
            hand_right: inserter(held_item(None, false)),
            holding: inserter(held_item(Some(hand_left), false)),
            hair,
            rotation,
            sprint_await: false,
            buffered: [0.0; BufferedAction::COUNT]
        };

        if !entities.light_exists(entity)
        {
            entities.set_light(entity, Some(Light{source: Some(info.holding), ..Default::default()}));
        }

        self.info = Some(info);
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

    pub fn set_holding(&mut self, holding: Option<InventoryItem>)
    {
        self.holding = holding;
        self.held_update = true;
    }

    pub fn dropped_item(&mut self, item: InventoryItem)
    {
        if Some(item) == self.holding
        {
            self.set_holding(None);
        }
    }

    pub fn newtons(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        self.anatomy(combined_info.entities).map(|x| x.strength().map(|strength| strength * 30.0).unwrap_or(0.0))
    }

    #[allow(dead_code)]
    pub fn stamina(&self) -> f32
    {
        self.stamina
    }

    pub fn attack_cooldown(&self) -> f32
    {
        self.attack_cooldown
    }

    pub fn stamina_fraction(&self, entities: &ClientEntities) -> Option<f32>
    {
        self.max_stamina(entities).map(|max_stamina| (self.stamina / max_stamina).min(1.0))
    }

    pub fn stamina_speed(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        self.anatomy(combined_info.entities).map(|x| x.stamina().unwrap_or(0.0))
    }

    pub fn max_stamina(&self, entities: &ClientEntities) -> Option<f32>
    {
        self.anatomy(entities).map(|x| x.max_stamina().unwrap_or(0.0))
    }

    fn held_attack_cooldown(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let item_info = self.held_info(combined_info);

        let heaviness = item_info.mass / (self.newtons(combined_info)? * 0.1);

        Some(item_info.comfort.recip() * heaviness.clamp(1.0, 2.0))
    }

    fn bash_attack_cooldown(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        self.held_attack_cooldown(combined_info).map(|x| x * 0.8)
    }

    pub fn bash_reachable(
        &self,
        this: &Transform,
        other: &Vector3<f32>
    ) -> bool
    {
        let bash_distance = some_or_value!(self.cached.bash_distance, false);
        let bash_distance = bash_distance * this.scale.x;

        let distance = this.position.metric_distance(other);

        distance <= bash_distance
    }

    fn bash_distance_parentless(&self, combined_info: CombinedInfo) -> f32
    {
        let item_info = self.held_info(combined_info);

        let item_scale = item_info.scale3().y;
        self.held_distance() + item_scale
    }

    fn bash_distance(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        self.scale_ratio(combined_info).map(|scale|
        {
            scale + self.bash_distance_parentless(combined_info) * 2.0
        })
    }

    fn update_cached(&mut self, combined_info: CombinedInfo)
    {
        self.cached.bash_distance = self.scale_ratio(combined_info).map(|scale|
        {
            scale + self.bash_distance_parentless(combined_info)
        });
    }

    fn held_scale(&self) -> f32
    {
        match self.sprite_state.value()
        {
            SpriteState::Crawling => 1.0 / 1.5,
            _ => 1.0
        }
    }

    fn update_held(
        &mut self,
        combined_info: CombinedInfo
    )
    {
        let state = *self.sprite_state.value();
        if state != SpriteState::Normal && state != SpriteState::Crawling
        {
            return;
        }

        let entities = &combined_info.entities;

        let info = some_or_return!(self.info.as_ref());

        let this_entity = info.this;
        let holding_entity = info.holding;
        let hand_left = info.hand_left;
        let hand_right = info.hand_right;

        self.update_cached(combined_info);

        let get_texture = |texture|
        {
            combined_info.assets.lock().texture(texture).clone()
        };

        let holding_item = self.holding.and_then(|holding| self.item_info(combined_info, holding));
        let holding_state = holding_item.is_some();

        some_or_return!(entities.parent_mut(holding_entity)).visible = holding_item.is_some();

        entities.lazy_setter.borrow_mut().set_follow_position(hand_right, holding_item.map(|_|
        {
            FollowPosition{
                parent: hand_left,
                connection: Connection::Rigid,
                offset: Vector3::new(ENTITY_SCALE * 0.1, 0.0, 0.0)
            }
        }));

        let mut light = some_or_return!(entities.light_mut(this_entity));
        if let Some(item) = holding_item
        {
            light.modify_light(|light| *light = item.lighting);

            let mut lazy_transform = entities.lazy_transform_mut(holding_entity).unwrap();

            let texture = get_texture(item.texture.unwrap());

            let target = lazy_transform.target();

            target.scale = item.scale3() * self.held_scale();

            drop(lazy_transform);

            let height = entities.lazy_target_end(holding_entity).unwrap().scale.y;
            entities.lazy_setter.borrow_mut().set_follow_position(holding_entity, Some(FollowPosition{
                parent: hand_left,
                connection: Connection::Rigid,
                offset: Vector3::new(0.0, -height / 2.0, 0.0)
            }));

            let mut render = entities.render_mut(holding_entity).unwrap();
            render.set_texture(texture);

            self.update_hands_rotation(combined_info);
        } else
        {
            light.modify_light(|light| *light = Light::default());
        }

        some_or_return!(entities.lazy_transform_mut(hand_right)).connection = if holding_state
        {
            Connection::Ignore
        } else
        {
            Self::default_connection()
        };

        let set_for = |entity, y|
        {
            let mut lazy = entities.lazy_transform_mut(entity).unwrap();
            let target = lazy.target();

            let x_sign = target.scale.x.signum();
            target.scale = Vector3::repeat(HAND_SCALE) * self.held_scale();
            target.scale.x *= x_sign;

            target.position = self.held_position(target.scale);
            target.position.y = y;
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

        lazy_for(holding_entity);
        lazy_for(hand_left);
        lazy_for(hand_right);

        if !holding_state
        {
            self.forward_point(combined_info);
        }

        self.held_update = false;
    }

    fn throw_held(
        &mut self,
        combined_info: CombinedInfo,
        target: Vector3<f32>
    )
    {
        if !self.can_attack(combined_info)
        {
            return;
        }

        let entities = &combined_info.entities;
        let strength = some_or_return!(self.newtons(combined_info)) * 0.2;
        let held = some_or_return!(self.holding.take());

        if let Some(item_info) = self.item_info(combined_info, held)
        {
            let info = self.info.as_ref().unwrap();

            let entity_info = {
                let holding_transform = entities.transform(info.holding).unwrap();

                let direction = {
                    let rotation = angle_between(
                        holding_transform.position,
                        target
                    );

                    Vector3::new(rotation.cos(), -rotation.sin(), 0.0)
                };

                let mut physical: Physical = PhysicalProperties{
                    inverse_mass: item_info.mass.recip(),
                    ..Default::default()
                }.into();

                let mass = physical.inverse_mass.recip();

                let throw_limit = 50.0 * mass;
                let throw_amount = (strength * 2.0).min(throw_limit);

                physical.add_force(direction * throw_amount);

                EntityInfo{
                    physical: Some(physical),
                    lazy_transform: Some(LazyTransformInfo{
                        deformation: Deformation::Stretch(StretchDeformation{
                            animation: ValueAnimation::EaseOut(2.0),
                            limit: 2.0,
                            onset: 0.05,
                            strength: 2.0
                        }),
                        transform: Transform{
                            position: holding_transform.position,
                            rotation: holding_transform.rotation,
                            scale: item_info.scale3() * ENTITY_SCALE,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::TextureId{
                            id: item_info.texture.unwrap()
                        }.into()),
                        z_level: ZLevel::Elbow,
                        ..Default::default()
                    }),
                    collider: Some(ColliderInfo{
                        kind: ColliderType::Rectangle,
                        ..Default::default()
                    }.into()),
                    light: Some(item_info.lighting),
                    damaging: Some(DamagingInfo{
                        damage: DamagingType::Mass(mass),
                        faction: Some(self.faction),
                        ..Default::default()
                    }.into()),
                    watchers: Some(Watchers::new(vec![
                        Watcher{
                            kind: WatcherType::Lifetime(2.5.into()),
                            action: WatcherAction::Explode(Box::new(ExplodeInfo{
                                keep: false,
                                info: ParticlesInfo{
                                    amount: 3..5,
                                    speed: ParticleSpeed::Random(0.1),
                                    decay: ParticleDecay::Random(3.5..=5.0),
                                    position: ParticlePosition::Spread(1.0),
                                    rotation: ParticleRotation::Random,
                                    scale: ParticleScale::Spread{
                                        scale: Vector3::repeat(ENTITY_SCALE * 0.4),
                                        variation: 0.1
                                    },
                                    min_scale: ENTITY_SCALE * 0.02
                                },
                                prototype: EntityInfo{
                                    physical: Some(PhysicalProperties{
                                        inverse_mass: 0.01_f32.recip(),
                                        floating: true,
                                        ..Default::default()
                                    }.into()),
                                    render: Some(RenderInfo{
                                        object: Some(RenderObjectKind::TextureId{
                                            id: combined_info.common_textures.dust
                                        }.into()),
                                        z_level: ZLevel::BelowFeet,
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }
                            })),
                            ..Default::default()
                        }
                    ])),
                    ..Default::default()
                }
            };

            entities.push(true, entity_info);

            entities.inventory_mut(info.this).unwrap().remove(held);
        }

        self.held_update = true;
    }

    pub fn can_move(&self, combined_info: CombinedInfo) -> bool
    {
        self.anatomy(combined_info.entities).map(|anatomy|
        {
            anatomy.speed().is_some()
        }).unwrap_or(true)
    }

    fn attack_stamina_cost(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let item_info = self.held_info(combined_info);

        let raw_use = item_info.mass / self.newtons(combined_info)? * 4.0;

        let cost = 0.3 + raw_use / item_info.comfort;

        Some(cost)
    }

    fn consume_attack_stamina(&mut self, combined_info: CombinedInfo)
    {
        self.stamina -= some_or_return!(self.attack_stamina_cost(combined_info));
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
        let cost = some_or_value!(self.attack_stamina_cost(combined_info), false);
        let attackable_item = cost <= self.stamina;

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

        let mut lazy = some_or_return!(combined_info.entities.lazy_transform_mut(holding));
        let swing_time = some_or_return!(self.bash_attack_cooldown(combined_info));

        let new_rotation = self.current_hand_rotation();

        if let Rotation::EaseOut(x) = &mut lazy.rotation
        {
            x.set_decay(70.0);
        }

        lazy.target().rotation = start_rotation - new_rotation;

        let mut watchers = combined_info.entities.watchers_mut(holding).unwrap();

        watchers.push(Watcher{
            kind: WatcherType::Lifetime(0.2.into()),
            action: WatcherAction::SetLazyRotation(Self::default_lazy_rotation()),
            ..Default::default()
        });

        if self.holding.is_some()
        {
            let mut target = some_or_return!(combined_info.entities.target(info.holding));
            target.scale.x = match self.bash_side
            {
                Side1d::Left => target.scale.x.abs(),
                Side1d::Right => -target.scale.x.abs()
            };
        } else
        {
            watchers.push(Watcher{
                kind: WatcherType::Lifetime((swing_time * 0.8).into()),
                action: WatcherAction::SetTargetRotation(start_rotation),
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

        self.attack_cooldown = some_or_return!(self.bash_attack_cooldown(combined_info));

        self.bash_side = self.bash_side.opposite();

        self.consume_attack_stamina(combined_info);

        self.bash_projectile(combined_info);

        self.update_hands_rotation(combined_info);
    }

    fn default_held_rotation(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let origin_rotation = combined_info.entities
            .lazy_transform(self.info.as_ref().unwrap().holding)?
            .origin_rotation();

        Some(-origin_rotation)
    }

    fn forward_point(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());

        let start_rotation = some_or_return!(self.default_held_rotation(combined_info));

        let f = |entity|
        {
            if let Some(mut lazy) = combined_info.entities.lazy_transform_mut(entity)
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

        some_or_return!(entities.lazy_transform_mut(hand_left)).rotation = Self::fast_lazy_rotation();

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

        some_or_return!(entities.lazy_transform_mut(hand_left)).rotation = Self::fast_lazy_rotation();

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

        let item = some_or_false!(self.held_item(combined_info));

        self.attack_cooldown = some_or_false!(self.held_attack_cooldown(combined_info));

        self.consume_attack_stamina(combined_info);

        self.poke_projectile(combined_info, item);

        let info = some_or_false!(self.info.as_ref());

        let entities = &combined_info.entities;

        let mut lazy = some_or_false!(entities.lazy_transform_mut(info.hand_left));

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

        let item_position = self.held_position(target.scale);
        let held_position = Vector3::new(item_position.x, target.position.y, 0.0);

        target.position.x = item_position.x + POKE_DISTANCE;
        target.rotation = start_rotation;

        let rotation = start_rotation - current_hand_rotation;

        let mut watchers = entities.watchers_mut(info.hand_left).unwrap();

        let end = extend_time + extend_time;
        let kind = WatcherType::Lifetime(end.into());

        watchers.push(Watcher{
            kind: kind.clone(),
            action: WatcherAction::SetLazyRotation(Self::default_lazy_rotation()),
            ..Default::default()
        });

        watchers.push(Watcher{
            kind: kind.clone(),
            action: WatcherAction::SetTargetRotation(rotation),
            ..Default::default()
        });

        watchers.push(Watcher{
            kind,
            action: WatcherAction::SetTargetPosition(held_position),
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

        let item = some_or_false!(self.held_item(combined_info));

        let items_info = combined_info.items_info;
        let ranged = some_or_false!(&items_info.get(item.id).ranged);

        self.attack_cooldown = ranged.cooldown();

        let info = some_or_false!(self.info.as_ref());

        let source = Some(info.this);
        let start = combined_info.entities.transform(info.this).unwrap().position;

        let damage = ranged.damage();

        let info = RaycastInfo{
            pierce: Some(damage.as_ranged_pierce()),
            pierce_scale: RaycastPierce::Density{ignore_anatomy: true},
            layer: ColliderLayer::Damage,
            ignore_entity: Some(info.this),
            ignore_end: true
        };

        let height = DamageHeight::random();

        let damage = DamagePartial{
            data: damage,
            height
        };

        combined_info.entities.push(true, EntityInfo{
            damaging: Some(DamagingInfo{
                damage: DamagingType::Raycast{info, damage, start, target, scale_pierce: Some(ENTITY_SCALE.recip())},
                faction: Some(self.faction),
                source,
                ..Default::default()
            }.into()),
            ..Default::default()
        });

        true
    }

    fn target_mass(&self, combined_info: CombinedInfo) -> f32
    {
        self.newtons(combined_info).unwrap_or(0.0) * 0.005
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
            SpriteState::Normal => DamageHeight::either(DamageHeight::Top, DamageHeight::Middle),
            SpriteState::Crawling
            | SpriteState::Lying => DamageHeight::Bottom
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

        let scale = some_or_return!(self.bash_distance(combined_info));

        let hand_mass = ItemInfo::hand().mass;
        let item_info = self.held_info(combined_info);

        let strength_scale = some_or_return!(self.newtons(combined_info)) * 0.05;

        let damage_scale = strength_scale * self.mass_maxed(combined_info, item_info.mass);
        let damage = DamagePartial{
            data: (*item_info).clone().with_changed(|x| x.mass += hand_mass).bash_damage() * damage_scale,
            height: self.melee_height()
        };

        let angle = opposite_angle(self.bash_side.opposite().to_angle() - f32::consts::FRAC_PI_2);

        combined_info.entities.push(
            true,
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    transform: Transform{
                        scale: Vector3::repeat(scale),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(info.this, true)),
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
                    predicate: DamagingPredicate::ParentAngleLess(f32::consts::PI),
                    faction: Some(self.faction),
                    ..Default::default()
                }.into()),
                watchers: Some(Watchers::new(vec![
                    Watcher{
                        kind: WatcherType::Lifetime(0.2.into()),
                        action: WatcherAction::Remove,
                        ..Default::default()
                    }
                ])),
                ..Default::default()
            }
        );
    }

    fn poke_projectile(&mut self, combined_info: CombinedInfo, item: Item)
    {
        let info = some_or_return!(self.info.as_ref());

        let hand_mass = ItemInfo::hand().mass;
        let item_info = combined_info.items_info.get(item.id);
        let item_scale = item_info.scale3().y;
        let mut scale = Vector3::repeat(1.0);

        let projectile_scale = POKE_DISTANCE / item_scale;
        scale.y += projectile_scale;

        let offset = projectile_scale / 2.0;

        let strength_scale = some_or_return!(self.newtons(combined_info)) * 0.03;

        let damage_scale = strength_scale * self.mass_maxed(combined_info, item_info.mass);
        let damage = DamagePartial{
            data: item_info.clone().with_changed(|x| x.mass += hand_mass).poke_damage() * damage_scale,
            height: self.melee_height()
        };

        combined_info.entities.push(
            true,
            EntityInfo{
                follow_rotation: Some(FollowRotation::new(
                    info.holding,
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
                parent: Some(Parent::new(info.holding, true)),
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
                    faction: Some(self.faction),
                    source: Some(info.this),
                    ..Default::default()
                }.into()),
                watchers: Some(Watchers::new(vec![
                    Watcher{
                        kind: WatcherType::Lifetime(0.2.into()),
                        action: WatcherAction::Remove,
                        ..Default::default()
                    }
                ])),
                ..Default::default()
            }
        );
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
                CharacterAction::Throw(target) => self.throw_held(combined_info, target),
                CharacterAction::Poke{state: false} => self.poke_attack_start(combined_info),
                CharacterAction::Poke{state: true} => with_clear!(self.poke_attack(combined_info)),
                CharacterAction::Ranged{state: false, ..} => self.aim_start(combined_info),
                CharacterAction::Ranged{state: true, target} => with_clear!(self.ranged_attack(combined_info, target)),
                CharacterAction::Bash => self.bash_attack(combined_info, true)
            }
        });
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

    fn held_info<'a>(&'a self, combined_info: CombinedInfo<'a>) -> Cow<'a, ItemInfo>
    {
        self.holding.and_then(|holding| self.item_info(combined_info, holding))
            .map(Cow::Borrowed)
            .unwrap_or_else(move || Cow::Owned(ItemInfo::hand()))
    }

    fn held_item(&self, combined_info: CombinedInfo) -> Option<Item>
    {
        self.info.as_ref().and_then(|info|
        {
            combined_info.entities.exists(info.this).then(||
            {
                let inventory = combined_info.entities.inventory(info.this).unwrap();

                self.holding.and_then(|holding| inventory.get(holding).cloned())
            }).flatten()
        })
    }

    fn held_distance(&self) -> f32
    {
        if *self.sprite_state.value() == SpriteState::Crawling
        {
            DEFAULT_HELD_DISTANCE - 0.1
        } else
        {
            DEFAULT_HELD_DISTANCE
        }
    }

    fn held_position(&self, scale: Vector3<f32>) -> Vector3<f32>
    {
        let offset = scale.y / 2.0 + 0.5 + self.held_distance();

        Vector3::new(offset, 0.0, 0.0)
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
                    BufferedAction::Aim => self.aim_start(combined_info)
                }
            }
        }
    }

    pub fn scale_ratio(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let info = combined_info.characters_info.get(self.id);
        self.info.as_ref().and_then(|this_info|
        {
            combined_info.entities.transform(this_info.this).map(|transform|
            {
                info.scale / transform.scale.x
            })
        })
    }

    pub fn update_common(
        &mut self,
        characters_info: &CharactersInfo,
        entities: &impl AnyEntities
    ) -> bool
    {
        if !self.sprite_state.changed()
        {
            return false;
        }

        let set_scale = |scale: Vector3<f32>|
        {
            let info = some_or_return!(&self.info);

            entities.target(info.this).unwrap().scale = scale;

            if let Some(end) = entities.lazy_target_end(info.this)
            {
                let mut transform = entities.transform_mut(info.this)
                    .unwrap();

                transform.scale = end.scale;
            }
        };

        let info = characters_info.get(self.id);
        match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                set_scale(Vector3::repeat(info.scale));
            },
            SpriteState::Crawling | SpriteState::Lying =>
            {
                set_scale(Vector3::repeat(info.scale * 1.5));
            }
        }

        true
    }

    pub fn update(
        &mut self,
        combined_info: CombinedInfo,
        entity: Entity,
        dt: f32,
        set_sprite: impl FnOnce(TextureId)
    )
    {
        let entities = combined_info.entities;

        if self.info.is_none()
        {
            self.initialize(entities, entity);
            if self.info.is_none()
            {
                return;
            }
        }

        self.handle_actions(combined_info);

        if self.held_update
        {
            self.update_held(combined_info);
        }

        self.update_jiggle(combined_info, dt);
        self.update_sprint(combined_info, dt);
        self.update_attacks(dt);

        self.update_buffered(combined_info, dt);

        if !self.update_common(combined_info.characters_info, combined_info.entities)
        {
            return;
        }

        let character_info = combined_info.characters_info.get(self.id);

        let set_visible = |entity, is_visible|
        {
            if let Some(mut parent) = entities.parent_mut_no_change(entity)
            {
                parent.visible = is_visible;
            } else if let Some(mut render) = entities.render_mut_no_change(entity)
            {
                render.visible = is_visible;
            }
        };

        let is_player = combined_info.is_player(entity);
        let layer = if is_player
        {
            ColliderLayer::Player
        } else
        {
            match self.sprite_state.value()
            {
                SpriteState::Normal => ColliderLayer::NormalEnemy,
                SpriteState::Crawling
                | SpriteState::Lying => ColliderLayer::LyingEnemy
            }
        };

        let z_level = match self.sprite_state.value()
        {
            SpriteState::Normal => if is_player { ZLevel::PlayerHead } else { ZLevel::Head },
            SpriteState::Crawling
            | SpriteState::Lying => ZLevel::Feet
        };

        let hair_visibility = match self.sprite_state.value()
        {
            SpriteState::Normal => true,
            SpriteState::Crawling
            | SpriteState::Lying => false
        };

        let held_visibility = match self.sprite_state.value()
        {
            SpriteState::Normal
            | SpriteState::Crawling => true,
            SpriteState::Lying => false
        };

        let texture = match self.sprite_state.value()
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
        };

        let scale = match self.sprite_state.value()
        {
            SpriteState::Normal => None,
            SpriteState::Crawling
            | SpriteState::Lying => Some(Vector3::repeat(ENTITY_SCALE))
        };

        entities.lazy_setter.borrow_mut().set_collider(entity, Some(ColliderInfo{
            kind: ColliderType::Circle,
            layer,
            scale,
            ..Default::default()
        }.into()));

        entities.set_z_level(entity, z_level);

        if let Some(info) = self.info.as_ref()
        {
            {
                let set_visible = |entity| set_visible(entity, held_visibility);

                set_visible(info.hand_left);
                set_visible(info.hand_right);
                set_visible(info.holding);
            }

            let set_visible = |entity| set_visible(entity, hair_visibility);

            info.hair.iter().copied().for_each(set_visible);
        }

        self.update_held(combined_info);

        set_sprite(texture);
    }

    pub fn anatomy_changed(&mut self, anatomy: &Anatomy)
    {
        let can_move = anatomy.speed().is_some();

        let state = if can_move
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
    }

    pub fn rotation_mut(&mut self) -> Option<&mut f32>
    {
        self.info.as_mut().map(|x| &mut x.rotation)
    }

    fn is_sprinting(&self) -> bool
    {
        if some_or_value!(self.info.as_ref(), false).sprint_await
        {
            return false;
        }

        self.sprinting && self.stamina > 0.0
    }

    fn update_jiggle(&mut self, combined_info: CombinedInfo, dt: f32)
    {
        let info = some_or_return!(self.info.as_mut());
        let physical = some_or_return!(combined_info.entities.physical(info.this));
        let speed = physical.velocity().xy().magnitude() * 50.0;

        self.jiggle = (self.jiggle + dt * speed) % (2.0 * f32::consts::PI);

        let mut target = some_or_return!(combined_info.entities.target(info.this));

        target.rotation = if *self.sprite_state.value() == SpriteState::Crawling
        {
            info.rotation + self.jiggle.sin() * 0.25
        } else
        {
            info.rotation
        };
    }

    fn update_sprint(&mut self, combined_info: CombinedInfo, dt: f32)
    {
        let max_stamina = some_or_return!(self.max_stamina(combined_info.entities));
        let recharge_speed = some_or_return!(self.stamina_speed(combined_info));

        if self.is_sprinting()
        {
            Self::decrease_timer(&mut self.stamina, 0.5 * dt);
            if self.stamina <= 0.0
            {
                let info = some_or_return!(self.info.as_mut());

                info.sprint_await = true;
            }
        } else
        {
            self.stamina += dt * recharge_speed;
        }

        self.stamina = self.stamina.min(max_stamina);
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

    pub fn walk(
        &self,
        anatomy: &Anatomy,
        physical: &mut Physical,
        direction: Unit<Vector3<f32>>,
        dt: f32
    )
    {
        let speed = some_or_return!(anatomy.speed());

        let speed = if self.is_sprinting()
        {
            speed * 1.8
        } else
        {
            speed
        };

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

        physical.add_force(change_velocity);
    }

    pub fn aggressive(&self, other: &Self) -> bool
    {
        self.faction.aggressive(&other.faction)
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

    fn set_sprite(&mut self, state: SpriteState)
    {
        self.sprite_state.set_state(state);
    }
}
