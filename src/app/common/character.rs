use std::{
    f32,
    mem,
    borrow::Cow,
    sync::Arc
};

use parking_lot::{RwLock, Mutex};

use serde::{Serialize, Deserialize};

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
        define_layers,
        angle_between,
        ENTITY_SCALE,
        render_info::*,
        lazy_transform::*,
        collider::*,
        watcher::*,
        damage::*,
        damaging::*,
        particle_creator::*,
        raycast::*,
        Hairstyle,
        Side1d,
        Physical,
        PhysicalProperties,
        AnyEntities,
        Entity,
        EntityInfo,
        CharacterId,
        CharactersInfo,
        ItemsInfo,
        Item,
        InventoryItem,
        ItemInfo,
        Parent,
        Anatomy,
        entity::ClientEntities
    }
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
pub enum CharacterAction
{
    Throw(Vector3<f32>),
    Poke,
    Bash,
    Ranged(Vector3<f32>)
}

pub const HELD_DISTANCE: f32 = 0.1;
pub const POKE_DISTANCE: f32 = 0.75;

// hands r actually 0.1 meters in size but they look too small that way
pub const HAND_SCALE: f32 = 0.3;
const HANDS_UNSTANCE: f32 = 0.7;

#[derive(Clone, Copy)]
pub struct PartialCombinedInfo<'a>
{
    pub passer: &'a Arc<RwLock<ConnectionsHandler>>,
    pub common_textures: &'a CommonTextures,
    pub items_info: &'a ItemsInfo,
    pub characters_info: &'a CharactersInfo
}

impl<'a> PartialCombinedInfo<'a>
{
    pub fn to_full(
        self,
        entities: &'a ClientEntities,
        assets: &'a Arc<Mutex<Assets>>
    ) -> CombinedInfo<'a>
    {
        CombinedInfo{
            entities,
            assets,
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
    pub assets: &'a Arc<Mutex<Assets>>,
    pub common_textures: &'a CommonTextures,
    pub items_info: &'a ItemsInfo,
    pub characters_info: &'a CharactersInfo
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterInfo
{
    this: Entity,
    holding: Entity,
    holding_right: Entity,
    hair: Vec<Entity>
}

#[derive(Default, Debug, Clone)]
struct CachedInfo
{
    pub bash_distance: Option<f32>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
    pub id: CharacterId,
    pub faction: Faction,
    pub strength: f32,
    pub sprinting: bool,
    oversprint_cooldown: f32,
    stamina: f32,
    holding: Option<InventoryItem>,
    #[serde(skip, default)]
    cached: CachedInfo,
    info: Option<AfterInfo>,
    held_update: bool,
    stance_time: f32,
    attack_cooldown: f32,
    bash_side: Side1d,
    actions: Vec<CharacterAction>,
    sprite_state: Stateful<SpriteState>
}

impl Character
{
    pub fn new(
        id: CharacterId,
        faction: Faction,
        strength: f32
    ) -> Self
    {
        Self{
            id,
            faction,
            strength,
            sprinting: false,
            oversprint_cooldown: 0.0,
            stamina: 1.0,
            info: None,
            holding: None,
            cached: CachedInfo::default(),
            held_update: true,
            stance_time: 0.0,
            attack_cooldown: 0.0,
            bash_side: Side1d::Left,
            actions: Vec::new(),
            sprite_state: SpriteState::Normal.into()
        }
    }

    fn default_connection() -> Connection
    {
        Connection::Spring(SpringConnection{
            physical: PhysicalProperties{
                mass: 0.5,
                friction: 0.4,
                floating: true
            }.into(),
            limit: 0.004,
            damping: 0.02,
            strength: 6.0
        })
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

    pub fn initialize(
        &mut self,
        characters_info: &CharactersInfo,
        entity: Entity,
        mut inserter: impl FnMut(EntityInfo) -> Entity
    )
    {
        let character_info = characters_info.get(self.id);

        let held_item = |flip|
        {
            EntityInfo{
                render: Some(RenderInfo{
                    object: Some(RenderObject{
                        kind: RenderObjectKind::Texture{
                            name: "placeholder.png".to_owned()
                        },
                        flip: if flip { Uvs::FlipHorizontal } else { Uvs::Normal }
                    }),
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Arms,
                    ..Default::default()
                }),
                parent: Some(Parent::new(entity, false)),
                lazy_transform: Some(LazyTransformInfo{
                    connection: Self::default_connection(),
                    rotation: Self::default_lazy_rotation(),
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

        let mut hair = Vec::new();

        let pon = |texture, position: Vector3<f32>|
        {
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    connection: Connection::Spring(
                        SpringConnection{
                            physical: PhysicalProperties{
                                mass: 0.01,
                                friction: 0.8,
                                floating: true
                            }.into(),
                            limit: 0.004,
                            damping: 0.02,
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
                            animation: ValueAnimation::EaseOut(2.0),
                            limit: 1.3,
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
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Hair,
                    ..Default::default()
                }),
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

        let info = AfterInfo{
            this: entity,
            holding: inserter(held_item(true)),
            holding_right: inserter(held_item(false)),
            hair
        };

        self.info = Some(info);
    }

    pub fn with_previous(&mut self, previous: Self)
    {
        self.sprite_state.set_state(*previous.sprite_state.value());
        self.sprite_state.dirty();
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

    pub fn newtons(&self) -> f32
    {
        self.strength * 30.0
    }

    fn attack_cooldown(&self, combined_info: CombinedInfo) -> f32
    {
        let item_info = self.held_info(combined_info);
        let liftability = self.strength / item_info.mass;

        let usability = liftability * item_info.comfort;

        usability.recip()
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

    fn bash_distance(&self, combined_info: CombinedInfo) -> f32
    {
        let item_info = self.held_info(combined_info);

        let item_scale = item_info.scale3().y;
        let over_scale = HELD_DISTANCE + item_scale;

        1.0 + over_scale * 2.0
    }

    fn update_cached(&mut self, combined_info: CombinedInfo)
    {
        self.cached.bash_distance = Some(self.bash_distance(combined_info));
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

        let entities = &combined_info.entities;

        let info = some_or_return!(self.info.as_ref());

        let holding_entity = info.holding;
        let holding_right = info.holding_right;

        let mut parent = some_or_return!(entities.parent_mut(holding_entity));
        let mut parent_right = some_or_return!(entities.parent_mut(holding_right));

        self.held_update = false;

        self.update_cached(combined_info);

        parent.visible = true;
        drop(parent);

        let get_texture = |texture|
        {
            combined_info.assets.lock().texture(texture).clone()
        };

        if let Some(item) = self.holding.and_then(|holding| self.item_info(combined_info, holding))
        {
            parent_right.visible = false;

            let texture = get_texture(item.texture.unwrap());

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

                target.scale = Vector3::repeat(HAND_SCALE);

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

    fn throw_held(
        &mut self,
        combined_info: CombinedInfo,
        target: Vector3<f32>
    )
    {
        let entities = &combined_info.entities;
        let held = some_or_return!(self.holding.take());

        if let Some(item_info) = self.item_info(combined_info, held)
        {
            let info = self.info.as_ref().unwrap();

            let entity_info = {
                let holding_transform = entities.transform(info.holding).unwrap();

                let direction = {
                    let rotation = angle_between(
                        target,
                        holding_transform.position
                    );

                    Vector3::new(rotation.cos(), -rotation.sin(), 0.0)
                };

                let mut physical: Physical = PhysicalProperties{
                    mass: item_info.mass,
                    friction: 0.999,
                    floating: false
                }.into();

                let mass = physical.mass;

                let strength = self.newtons() * 0.4;
                let throw_limit = 0.1 * strength;
                let throw_amount = (strength / mass).min(throw_limit);
                physical.velocity = direction * throw_amount;

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
                        kind: ColliderType::Circle,
                        ..Default::default()
                    }.into()),
                    damaging: Some(DamagingInfo{
                        damage: DamagingType::Mass(mass),
                        faction: Some(Faction::Player),
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
                                        mass: 0.01,
                                        friction: 0.1,
                                        floating: true
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

    fn can_move(&self, combined_info: CombinedInfo) -> bool
    {
        self.info.as_ref().and_then(|info|
        {
            combined_info.entities.anatomy(info.this).map(|anatomy|
            {
                anatomy.speed().is_some()
            })
        }).unwrap_or(true)
    }

    fn bash_attack(&mut self, combined_info: CombinedInfo)
    {
        if !self.can_move(combined_info)
        {
            return;
        }

        if self.attack_cooldown > 0.0
        {
            return;
        }

        self.attack_cooldown = self.attack_cooldown(combined_info);
        self.stance_time = self.attack_cooldown * 2.0;

        self.bash_side = self.bash_side.opposite();

        self.bash_projectile(combined_info);

        let info = some_or_return!(self.info.as_ref());

        let start_rotation = some_or_return!(self.default_held_rotation(combined_info));

        let holding = if self.holding.is_some()
        {
            info.holding
        } else
        {
            match self.bash_side
            {
                Side1d::Left => info.holding_right,
                Side1d::Right => info.holding
            }
        };

        let mut lazy = some_or_return!(combined_info.entities.lazy_transform_mut(holding));

        let edge = 0.4;

        let new_rotation = match self.bash_side
        {
            Side1d::Left =>
            {
                f32::consts::FRAC_PI_2 - edge
            },
            Side1d::Right =>
            {
                -f32::consts::FRAC_PI_2 + edge
            }
        };

        match &mut lazy.rotation
        {
            Rotation::EaseOut(x) => x.set_decay(30.0),
            _ => ()
        }

        lazy.target().rotation = start_rotation - new_rotation;

        let mut watchers = combined_info.entities.watchers_mut(holding).unwrap();

        watchers.push(Watcher{
            kind: WatcherType::Lifetime(0.2.into()),
            action: WatcherAction::SetLazyRotation(Self::default_lazy_rotation()),
            ..Default::default()
        });
    }

    fn default_held_rotation(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let origin_rotation = combined_info.entities
            .lazy_transform(self.info.as_ref().unwrap().holding)?
            .origin_rotation();

        Some(-origin_rotation)
    }

    fn unstance(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());

        let start_rotation = some_or_return!(self.default_held_rotation(combined_info));

        let set_rotation = |entity|
        {
            if let Some(mut lazy) = combined_info.entities.lazy_transform_mut(entity)
            {
                lazy.target().rotation = start_rotation;
            }
        };

        set_rotation(info.holding);
        set_rotation(info.holding_right);
    }

    fn poke_attack(&mut self, combined_info: CombinedInfo)
    {
        if !self.can_move(combined_info)
        {
            return;
        }

        let item = some_or_return!(self.held_item(combined_info));

        if self.attack_cooldown > 0.0
        {
            return;
        }

        self.unstance(combined_info);

        self.attack_cooldown = self.attack_cooldown(combined_info);

        self.poke_projectile(combined_info, item);

        let info = some_or_return!(self.info.as_ref());

        let entities = &combined_info.entities;

        if let Some(mut lazy) = entities.lazy_transform_mut(info.holding)
        {
            let lifetime = self.attack_cooldown.min(0.5);
            lazy.connection = Connection::Timed(TimedConnection::from(Lifetime::from(lifetime)));

            let held_position = self.held_item_position(combined_info).unwrap();

            lazy.target().position.x = held_position.x + POKE_DISTANCE;

            let parent_transform = entities.parent_transform(info.holding);
            let new_target = lazy.target_global(parent_transform.as_ref());

            entities.transform_mut(info.holding).unwrap().position = new_target.position;

            let mut watchers = entities.watchers_mut(info.holding).unwrap();

            let extend_time = 0.2;

            watchers.push(Watcher{
                kind: WatcherType::Lifetime(extend_time.into()),
                action: WatcherAction::SetTargetPosition(held_position),
                ..Default::default()
            });

            watchers.push(Watcher{
                kind: WatcherType::Lifetime(lifetime.into()),
                action: WatcherAction::SetLazyConnection(Self::default_connection()),
                ..Default::default()
            });
        }
    }

    fn ranged_attack(
        &mut self,
        combined_info: CombinedInfo,
        target: Vector3<f32>
    )
    {
        if !self.can_move(combined_info)
        {
            return;
        }

        let item = some_or_return!(self.held_item(combined_info));

        let items_info = combined_info.items_info;
        let ranged = some_or_return!(&items_info.get(item.id).ranged);

        if self.attack_cooldown > 0.0
        {
            return;
        }

        self.unstance(combined_info);

        self.attack_cooldown = ranged.cooldown();

        let info = some_or_return!(self.info.as_ref());

        let start = &combined_info.entities.transform(info.this).unwrap().position;
        
        let info = RaycastInfo{
            pierce: None,
            layer: ColliderLayer::Damage,
            ignore_entity: Some(info.this),
            ignore_end: true
        };

        let hits = combined_info.entities.raycast(info, start, &target);

        let damage = ranged.damage();

        let height = DamageHeight::random();

        for hit in &hits.hits
        {
            #[allow(clippy::single_match)]
            match hit.id
            {
                RaycastHitId::Entity(id) =>
                {
                    let transform = combined_info.entities.transform(id)
                        .unwrap();

                    let hit_position = hits.hit_position(hit);

                    let angle = angle_between(hit_position, transform.position);

                    let damage = DamagePartial{
                        data: damage,
                        height
                    };

                    drop(transform);

                    let mut passer = combined_info.passer.write();
                    combined_info.entities.damage_entity(
                        &mut *passer,
                        combined_info.common_textures.blood,
                        angle,
                        id,
                        self.faction,
                        damage
                    );
                },
                _ => ()
            }
        }
    }

    fn bash_projectile(&mut self, combined_info: CombinedInfo)
    {
        let info = some_or_return!(self.info.as_ref());

        let scale = self.bash_distance(combined_info);

        let item_info = self.held_info(combined_info);

        let damage = DamagePartial{
            data: item_info.bash_damage().scale(self.strength),
            height: DamageHeight::random()
        };

        let angle = self.bash_side.to_angle() - f32::consts::FRAC_PI_2;

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
                    damage: DamagingType::Damage{
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

        let item_info = combined_info.items_info.get(item.id);
        let item_scale = item_info.scale3().y;
        let mut scale = Vector3::repeat(1.0);

        let projectile_scale = POKE_DISTANCE / item_scale;
        scale.y += projectile_scale;

        let offset = projectile_scale / 2.0;

        let damage = DamagePartial{
            data: item_info.poke_damage().scale(self.strength),
            height: DamageHeight::random()
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
                    kind: ColliderType::Circle,
                    layer: ColliderLayer::Damage,
                    ghost: true,
                    ..Default::default()
                }.into()),
                damaging: Some(DamagingInfo{
                    damage: DamagingType::Damage{
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

        if let Some(mut lazy) = combined_info.entities.lazy_transform_mut(info.holding)
        {
            lazy.connection = Self::default_connection();
        }
    }

    fn handle_actions(&mut self, combined_info: CombinedInfo)
    {
        if self.info.is_none()
        {
            return;
        }

        mem::take(&mut self.actions).into_iter().for_each(|action|
        {
            match action
            {
                CharacterAction::Throw(target) => self.throw_held(combined_info, target),
                CharacterAction::Poke => self.poke_attack(combined_info),
                CharacterAction::Bash => self.bash_attack(combined_info),
                CharacterAction::Ranged(target) => self.ranged_attack(combined_info, target)
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
            .map(|x| Cow::Borrowed(x))
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
        combined_info: CombinedInfo,
        dt: f32
    )
    {
        if Self::decrease_timer(&mut self.stance_time, dt)
        {
            self.unstance(combined_info);
        }

        let unstance_hands = |this: &mut Self|
        {
            if this.holding.is_none()
            {
                this.unstance(combined_info);
            }
        };

        if Self::decrease_timer(&mut self.attack_cooldown, dt)
        {
            unstance_hands(self);
        }

        if self.attack_cooldown < (self.attack_cooldown(combined_info) * HANDS_UNSTANCE)
        {
            unstance_hands(self);
        }

        Self::decrease_timer(&mut self.oversprint_cooldown, dt);
    }

    pub fn update_common(
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
                transform.scale = Vector3::repeat(info.scale * 1.5);
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
    ) -> bool
    {
        let entities = &combined_info.entities;

        self.handle_actions(combined_info);

        if self.held_update
        {
            self.update_held(combined_info);
        }

        self.update_sprint(dt);
        self.update_attacks(combined_info, dt);

        let mut target = entities.target(entity).unwrap();

        if !self.update_common(combined_info.characters_info, &mut target)
        {
            return false;
        }

        let character_info = combined_info.characters_info.get(self.id);

        let mut render = entities.render_mut(entity).unwrap();

        let set_visible = |entity, is_visible|
        {
            if let Some(mut parent) = entities.parent_mut(entity)
            {
                parent.visible = is_visible;
            } else if let Some(mut render) = entities.render_mut(entity)
            {
                render.visible = is_visible;
            }
        };

        let set_children_visibility = |is_visible|
        {
            let set_visible = |entity| set_visible(entity, is_visible);

            if let Some(info) = self.info.as_ref()
            {
                set_visible(info.holding);
                set_visible(info.holding_right);

                info.hair.iter().copied().for_each(set_visible);
            }
        };

        let texture = match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                entities.lazy_setter.borrow_mut().set_collider(entity, Some(ColliderInfo{
                    kind: ColliderType::Circle,
                    ghost: false,
                    ..Default::default()
                }.into()));

                entities.lazy_setter.borrow_mut().set_physical(entity, Some(PhysicalProperties{
                    mass: 50.0,
                    friction: 0.5,
                    floating: false
                }.into()));

                render.z_level = ZLevel::Head;
                set_children_visibility(true);

                character_info.normal
            },
            SpriteState::Lying =>
            {
                entities.lazy_setter.borrow_mut().set_collider(entity, Some(ColliderInfo{
                    kind: ColliderType::Circle,
                    ghost: true,
                    ..Default::default()
                }.into()));

                entities.lazy_setter.borrow_mut().set_physical(entity, None);

                render.z_level = ZLevel::Feet;
                set_children_visibility(false);

                character_info.lying
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

    fn is_sprinting(&self) -> bool
    {
        if self.oversprint_cooldown <= 0.0
        {
            self.sprinting
        } else
        {
            false
        }
    }

    fn update_sprint(&mut self, dt: f32)
    {
        if self.sprinting
        {
            Self::decrease_timer(&mut self.stamina, dt);
            if self.stamina < 0.0
            {
                self.oversprint_cooldown = 1.0 + self.stamina.abs();
            }
        }

        if !self.is_sprinting()
        {
            self.stamina += dt;
        }
    }

    pub fn walk(
        &self,
        anatomy: &Anatomy,
        physical: &mut Physical,
        direction: Unit<Vector3<f32>>
    )
    {
        if let Some(speed) = anatomy.speed()
        {
            let speed = if self.is_sprinting()
            {
                speed * 1.5
            } else
            {
                speed
            };

            let velocity = direction.into_inner() * (speed / physical.mass);

            let new_velocity = (physical.velocity + velocity).zip_map(&velocity, |value, limit|
            {
                let limit = limit.abs();

                value.min(limit).max(-limit)
            });

            physical.velocity.x = new_velocity.x;
            physical.velocity.y = new_velocity.y;
        }
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
