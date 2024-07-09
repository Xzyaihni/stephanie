use std::{
    f32,
    mem,
    cell::Ref,
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
    Poke,
    Bash,
    Ranged(Vector3<f32>)
}

pub const DEFAULT_HELD_DISTANCE: f32 = 0.1;
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
    pub bash_distance_parentless: Option<f32>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
    pub id: CharacterId,
    pub faction: Faction,
    pub sprinting: bool,
    pub rotation: f32,
    was_sprinting: bool,
    oversprint_cooldown: f32,
    stamina: f32,
    jiggle: f32,
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
        faction: Faction
    ) -> Self
    {
        Self{
            id,
            faction,
            sprinting: false,
            rotation: 0.0,
            was_sprinting: false,
            oversprint_cooldown: 0.0,
            stamina: f32::MAX,
            jiggle: 0.0,
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

    pub fn newtons(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        self.anatomy(combined_info.entities).and_then(|x| x.strength().map(|strength| strength * 30.0))
    }

    #[allow(dead_code)]
    pub fn stamina(&self) -> f32
    {
        self.stamina
    }

    pub fn stamina_fraction(&self, entities: &ClientEntities) -> Option<f32>
    {
        self.max_stamina(entities).map(|max_stamina| self.stamina / max_stamina)
    }

    pub fn stamina_speed(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        self.anatomy(combined_info.entities).and_then(|x| x.stamina())
    }

    pub fn max_stamina(&self, entities: &ClientEntities) -> Option<f32>
    {
        self.anatomy(entities).and_then(|x| x.max_stamina())
    }

    fn attack_cooldown(&self, combined_info: CombinedInfo) -> Option<f32>
    {
        let item_info = self.held_info(combined_info);

        Some(item_info.comfort.recip())
    }

    pub fn bash_reachable(
        &self,
        this: &Transform,
        other: &Vector3<f32>
    ) -> bool
    {
        let bash_distance = some_or_value!(self.cached.bash_distance_parentless, false);
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

    fn bash_distance(&self, combined_info: CombinedInfo) -> f32
    {
        1.0 + self.bash_distance_parentless(combined_info) * 2.0
    }

    fn update_cached(&mut self, combined_info: CombinedInfo)
    {
        self.cached.bash_distance_parentless = Some(
            1.0 + self.bash_distance_parentless(combined_info)
        );
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

            target.scale = item.scale3() * self.held_scale();
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

                target.scale = Vector3::repeat(HAND_SCALE) * self.held_scale();

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
                        target,
                        holding_transform.position
                    );

                    Vector3::new(rotation.cos(), -rotation.sin(), 0.0)
                };

                let mut physical: Physical = PhysicalProperties{
                    mass: item_info.mass,
                    friction: 0.7,
                    floating: false
                }.into();

                let mass = physical.mass;

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

        let raw_use = item_info.mass / self.newtons(combined_info)? * 70.0;

        let cost = raw_use / item_info.comfort;

        Some(cost)
    }

    fn consume_attack_stamina(&mut self, combined_info: CombinedInfo)
    {
        self.stamina -= some_or_return!(self.attack_stamina_cost(combined_info));
    }
    
    pub fn can_attack(&self, combined_info: CombinedInfo) -> bool
    {
        let state = *self.sprite_state.value();

        let attackable_state = state == SpriteState::Normal || state == SpriteState::Crawling;

        let cost = some_or_value!(self.attack_stamina_cost(combined_info), false);
        let attackable_item = cost <= self.stamina;

        attackable_state && attackable_item
    }

    fn anatomy<'a>(&'a self, entities: &'a ClientEntities) -> Option<Ref<'a, Anatomy>>
    {
        self.info.as_ref().and_then(move |info|
        {
            entities.anatomy(info.this)
        })
    }

    fn bash_attack(&mut self, combined_info: CombinedInfo)
    {
        if !self.can_attack(combined_info)
        {
            return;
        }

        if self.attack_cooldown > 0.0
        {
            return;
        }

        self.attack_cooldown = some_or_return!(self.attack_cooldown(combined_info)) * 0.8;
        self.stance_time = self.attack_cooldown * 2.0;

        self.bash_side = self.bash_side.opposite();

        self.consume_attack_stamina(combined_info);

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

        if let Rotation::EaseOut(x) = &mut lazy.rotation
        {
            x.set_decay(30.0);
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
        if !self.can_attack(combined_info)
        {
            return;
        }

        let item = some_or_return!(self.held_item(combined_info));

        if self.attack_cooldown > 0.0
        {
            return;
        }

        self.unstance(combined_info);

        self.attack_cooldown = some_or_return!(self.attack_cooldown(combined_info));

        self.consume_attack_stamina(combined_info);

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

        let damage_scale = some_or_return!(self.newtons(combined_info)) * 0.05;
        let damage = DamagePartial{
            data: item_info.bash_damage().scale(damage_scale),
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

        let damage_scale = some_or_return!(self.newtons(combined_info)) * 0.03;
        let damage = DamagePartial{
            data: item_info.poke_damage().scale(damage_scale),
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

    fn held_item_position(
        &self,
        combined_info: CombinedInfo
    ) -> Option<Vector3<f32>>
    {
        let item = self.item_info(combined_info, self.holding?)?;
        let scale = item.scale3();

        Some(self.item_position(scale))
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

    fn item_position(&self, scale: Vector3<f32>) -> Vector3<f32>
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

        if let Some(attack_cooldown) = self.attack_cooldown(combined_info)
        {
            if self.attack_cooldown < (attack_cooldown * HANDS_UNSTANCE)
            {
                unstance_hands(self);
            }
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
            SpriteState::Crawling | SpriteState::Lying =>
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

        self.update_jiggle(combined_info, dt);
        self.update_sprint(combined_info, dt);
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

        let (
            collider,
            physical,
            z_level,
            hair_visibility,
            held_visibility,
            texture
        ) = match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                (
                    Some(ColliderInfo{
                        kind: ColliderType::Circle,
                        ghost: false,
                        ..Default::default()
                    }.into()),
                    Some(PhysicalProperties{
                        mass: 50.0,
                        friction: 0.99,
                        floating: false
                    }.into()),
                    ZLevel::Head,
                    true,
                    true,
                    character_info.normal
                )
            },
            SpriteState::Crawling =>
            {
                (
                    Some(ColliderInfo{
                        kind: ColliderType::Circle,
                        ghost: false,
                        scale: Some(Vector3::repeat(0.4)),
                        ..Default::default()
                    }.into()),
                    Some(PhysicalProperties{
                        mass: 50.0,
                        friction: 0.999,
                        floating: false
                    }.into()),
                    ZLevel::Feet,
                    false,
                    true,
                    character_info.crawling
                )
            },
            SpriteState::Lying =>
            {
                (
                    Some(ColliderInfo{
                        kind: ColliderType::Circle,
                        ghost: true,
                        ..Default::default()
                    }.into()),
                    None,
                    ZLevel::Feet,
                    false,
                    false,
                    character_info.lying
                )
            }
        };

        {
            let mut setter = entities.lazy_setter.borrow_mut();
            setter.set_collider(entity, collider);
            setter.set_physical(entity, physical);
        }

        render.z_level = z_level;

        if let Some(info) = self.info.as_ref()
        {
            {
                let set_visible = |entity| set_visible(entity, held_visibility);

                set_visible(info.holding);
                set_visible(info.holding_right);
            }

            let set_visible = |entity| set_visible(entity, hair_visibility);

            info.hair.iter().copied().for_each(set_visible);
        }

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

    fn update_jiggle(&mut self, combined_info: CombinedInfo, dt: f32)
    {
        let info = some_or_return!(self.info.as_ref());
        let physical = some_or_return!(combined_info.entities.physical(info.this));
        let speed = physical.velocity.magnitude() * 50.0;

        self.jiggle = (self.jiggle + dt * speed) % (2.0 * f32::consts::PI);

        let mut target = some_or_return!(combined_info.entities.target(info.this));

        target.rotation = if *self.sprite_state.value() == SpriteState::Crawling
        {
            self.rotation + self.jiggle.sin() * 0.25
        } else
        {
            self.rotation
        };
    }

    fn update_sprint(&mut self, combined_info: CombinedInfo, dt: f32)
    {
        let max_stamina = some_or_return!(self.max_stamina(combined_info.entities));
        let recharge_speed = some_or_return!(self.stamina_speed(combined_info));

        if self.is_sprinting()
        {
            Self::decrease_timer(&mut self.stamina, dt);
            if self.stamina < 0.0
            {
                let until_half = ((max_stamina / 2.0) - self.stamina) / recharge_speed;

                self.oversprint_cooldown = until_half;
            }
        }

        if self.was_sprinting && !self.sprinting
        {
            self.oversprint_cooldown = 0.7;
        }

        if !self.is_sprinting()
        {
            self.stamina += dt * recharge_speed;
        }

        self.stamina = self.stamina.min(max_stamina);

        self.was_sprinting = self.is_sprinting();
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
                speed * 1.8
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
