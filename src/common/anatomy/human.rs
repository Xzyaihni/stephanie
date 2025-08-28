use std::{
    f32,
    mem,
    rc::Rc
};

use serde::{Serialize, Deserialize};

use crate::{
    debug_config::*,
    common::{
        some_or_return,
        TILE_SIZE,
        Side1d,
        Side2d,
        DamageHeight,
        Damage,
        Damageable,
        WeightedPicker
    }
};

use super::{
    DebugName,
    Health,
    Halves,
    BodyPartInfo,
    Organ,
    HealReceiver,
    SizeGetter,
    DamagerGetter,
    AverageHealthGetter,
    SkinHealthGetter,
    MuscleHealthGetter,
    BoneHealthGetter,
    AccessedGetter,
    SimpleHealth,
    ChangedKind,
    ChangedPart
};

pub use parts::{OrganId, AnatomyId, BrainId, HumanPartId, HumanPart};
use parts::*;

mod parts;


#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Speeds<T=f32>
{
    arms: T,
    legs: T
}

impl<T> Speeds<T>
{
    fn map<U>(self, mut f: impl FnMut(T) -> U) -> Speeds<U>
    {
        Speeds{arms: f(self.arms), legs: f(self.legs)}
    }
}

#[derive(Debug, Clone)]
pub struct CachedProps
{
    speed: Speeds<f32>,
    strength: f32,
    vision: f32,
    oxygen_change: f32,
    blood_change: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomyInfo
{
    pub bone_toughness: f32,
    pub muscle_toughness: f32,
    pub skin_toughness: f32,
    pub base_speed: f32,
    pub base_strength: f32
}

impl Default for HumanAnatomyInfo
{
    fn default() -> Self
    {
        Self{
            bone_toughness: 1.0,
            muscle_toughness: 1.0,
            skin_toughness: 1.0,
            base_speed: 1.0,
            base_strength: 1.0
        }
    }
}

struct PierceType
{
    possible: Vec<AnatomyId>,
    action: Rc<dyn Fn(&mut HumanAnatomyValues, Damage) -> Option<Damage>>
}

impl PierceType
{
    fn empty() -> Self
    {
        Self{possible: Vec::new(), action: Rc::new(|_, damage| { Some(damage) })}
    }

    fn always(id: AnatomyId) -> Self
    {
        Self{
            possible: vec![id],
            action: Rc::new(move |this, damage|
            {
                #[allow(clippy::question_mark)]
                if this.body.get::<()>(id).is_none()
                {
                    return None;
                }

                this.body.get_mut::<DamagerGetter>(id).unwrap()(damage)
            })
        }
    }

    fn no_follow() -> fn(&mut HumanAnatomyValues, Option<Damage>) -> Option<Damage>
    {
        |_this, damage|
        {
            damage
        }
    }

    fn pelvis(side: Side1d) -> Self
    {
        Self::always(HumanPartId::Thigh(side.opposite()).into())
    }

    fn head_back() -> Self
    {
        let possible = vec![OrganId::Eye(Side1d::Left).into(), OrganId::Eye(Side1d::Right).into()];

        Self::possible_pierce(possible, 1, Self::no_follow())
    }

    fn torso_front() -> Self
    {
        Self::possible_pierce(vec![HumanPartId::Spine.into()], 2, Self::no_follow())
    }

    fn possible_pierce<F>(possible: Vec<AnatomyId>, misses: usize, f: F) -> Self
    where
        F: Fn(&mut HumanAnatomyValues, Option<Damage>) -> Option<Damage> + 'static
    {
        let possible_cloned = possible.clone();

        Self{
            possible,
            action: Rc::new(move |this: &mut HumanAnatomyValues, damage|
            {
                let mut possible_actions = possible_cloned.clone();
                possible_actions.retain(|x| this.body.get::<()>(*x).is_some());

                if possible_actions.is_empty()
                {
                    return f(this, None);
                }

                let miss_check = fastrand::usize(0..possible_actions.len() + misses);
                if miss_check >= possible_actions.len()
                {
                    return f(this, None);
                }

                let target = fastrand::choice(possible_actions).unwrap();

                let pierce = this.body.get_mut::<DamagerGetter>(target).unwrap()(damage);

                f(this, pierce)
            })
        }
    }

    fn middle_pierce(side: Side1d) -> PierceType
    {
        let opposite = side.opposite();

        let possible = vec![
            HumanPartId::Arm(opposite).into(),
            HumanPartId::Forearm(opposite).into(),
            HumanPartId::Hand(opposite).into()
        ];

        Self::possible_pierce(possible, 0, Self::no_follow())
    }

    fn arm_pierce(side: Side1d) -> PierceType
    {
        let possible = vec![HumanPartId::Spine.into(), HumanPartId::Torso.into()];

        Self::possible_pierce(possible, 0, move |this, pierce|
        {
            (Self::middle_pierce(side).action)(this, pierce?)
        })
    }

    fn leg_pierce(side: Side1d) -> PierceType
    {
        let opposite = side.opposite();

        let possible = vec![
            HumanPartId::Thigh(opposite).into(),
            HumanPartId::Calf(opposite).into(),
            HumanPartId::Foot(opposite).into()
        ];

        Self::possible_pierce(possible, 0, Self::no_follow())
    }

    fn any_exists(&self, anatomy: &HumanAnatomyValues) -> bool
    {
        self.possible.iter().any(|x| anatomy.body.get::<()>(*x).is_some())
    }

    fn combined_scale(&self, anatomy: &HumanAnatomyValues) -> f64
    {
        self.possible.iter().filter_map(|x| anatomy.body.get::<SizeGetter>(*x)).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomyValues
{
    base_speed: f32,
    base_strength: f32,
    override_crawling: bool,
    blood: SimpleHealth,
    oxygen: SimpleHealth,
    external_oxygen_change: f32,
    hypoxic: f32,
    body: HumanBody,
    broken: Vec<AnatomyId>,
    killed: Option<bool>
}

impl Default for HumanAnatomyValues
{
    fn default() -> Self
    {
        Self::new(HumanAnatomyInfo::default())
    }
}

impl From<HumanAnatomy> for HumanAnatomyValues
{
    fn from(x: HumanAnatomy) -> Self
    {
        x.this
    }
}

impl HumanAnatomyValues
{
    pub fn new(mut info: HumanAnatomyInfo) -> Self
    {
        info.bone_toughness *= 0.5;

        info.base_speed *= 12.0;

        let bone_toughness = info.bone_toughness;

        let base_speed = info.base_speed;
        let base_strength = info.base_strength;

        let part = BodyPartInfo::from(info);

        fn new_part_with_contents<Contents>(
            name: DebugName,
            part: BodyPartInfo,
            bone_toughness: f32,
            health: f32,
            size: f64,
            contents: Contents
        ) -> HumanPart<Contents>
        {
            HumanPart::new(
                name,
                part,
                bone_toughness * health,
                size,
                contents
            )
        }

        let new_part = |name, health, size|
        {
            new_part_with_contents(name, part.clone(), bone_toughness, health, size, ())
        };

        // max hp is amount of newtons i found on the interner needed to break a bone
        // like half of them i just made up

        let with_name = |side_name|
        {
            move |name|
            {
                format!("{side_name} {name}")
            }
        };

        let make_leg = |side_name|
        {
            let with_name = with_name(side_name);

            let upper = new_part(DebugName::new(with_name("upper leg")), 40.0, 0.6);
            let lower = new_part(DebugName::new(with_name("lower leg")), 35.0, 0.44);
            let foot = {
                let mut x = new_part(DebugName::new(with_name("foot")), 20.0, 0.17);
                x.muscle = Health::zero().into();

                Some(x)
            };

            Some(Limb{
                upper,
                lower: Some(LowerLimb{
                    lower,
                    leaf: foot
                })
            })
        };

        let make_arm = |side_name|
        {
            let with_name = with_name(side_name);

            let upper = new_part(DebugName::new(with_name("upper arm")), 25.0, 0.2);
            let lower = new_part(DebugName::new(with_name("lower arm")), 20.0, 0.17);
            let hand = {
                let mut x = new_part(DebugName::new(with_name("hand")), 20.0, 0.07);
                x.muscle = Health::zero().into();

                Some(x)
            };


            Some(Limb{
                upper,
                lower: Some(LowerLimb{
                    lower,
                    leaf: hand
                })
            })
        };

        let spine = {
            // the spine is very complex sizing wise so im just gonna pick a low-ish number
            let mut x = new_part_with_contents(
                DebugName::new("spine"),
                part.clone(),
                bone_toughness,
                34.0,
                0.25,
                SpinalCord::new(1.0)
            );

            x.muscle = Health::zero().into();

            x
        };

        let head = {
            let mut head = new_part_with_contents(
                DebugName::new("head"),
                part.clone(),
                bone_toughness,
                50.0,
                0.39,
                HeadOrgans{eyes: Halves::repeat(Some(Eye::new(1.0))), brain: Some(Brain::new(0.5))}
            );

            head.muscle = Health::zero().into();

            head
        };

        let pelvis = new_part(DebugName::new("pelvis"), 60.0, 0.37);

        let pelvis = Pelvis{
            pelvis,
            legs: Halves{left: make_leg("left"), right: make_leg("right")}
        };

        let torso = new_part_with_contents(
            DebugName::new("torso"),
            part.clone(),
            bone_toughness,
            33.0,
            0.82,
            TorsoOrgans{
                lungs: Halves::repeat(Some(Lung::new(1.0)))
            }
        );

        let spine = Spine{
            spine,
            torso: Some(torso),
            arms: Halves{left: make_arm("left"), right: make_arm("right")},
            pelvis: Some(pelvis)
        };

        let body = HumanBody{
            head: Some(head),
            spine: Some(spine)
        };

        Self{
            base_speed,
            base_strength,
            override_crawling: false,
            blood: SimpleHealth::new(4.0),
            oxygen: SimpleHealth::new(1.0),
            external_oxygen_change: 0.0,
            hypoxic: 0.0,
            body,
            broken: Vec::new(),
            killed: None
        }.into()
    }

    fn damage_random_part(
        &mut self,
        damage: Damage
    ) -> Option<Damage>
    {
        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("start damage {damage:?}");
        }

        let no_pierce = PierceType::empty;

        let mut ids: Vec<(AnatomyId, _)> = match damage.direction.height
        {
            DamageHeight::Top =>
            {
                match damage.direction.side
                {
                    Side2d::Back => vec![
                        (HumanPartId::Spine.into(), no_pierce()),
                        (HumanPartId::Head.into(), PierceType::head_back())
                    ],
                    Side2d::Front => vec![
                        (HumanPartId::Spine.into(), no_pierce()),
                        (HumanPartId::Head.into(), no_pierce()),
                        (OrganId::Eye(Side1d::Left).into(), PierceType::always(HumanPartId::Head.into())),
                        (OrganId::Eye(Side1d::Right).into(), PierceType::always(HumanPartId::Head.into()))
                    ],
                    Side2d::Left | Side2d::Right => vec![
                        (HumanPartId::Spine.into(), no_pierce()),
                        (HumanPartId::Head.into(), no_pierce())
                    ]
                }
            },
            DamageHeight::Middle =>
            {
                match damage.direction.side
                {
                    Side2d::Back => vec![
                        (HumanPartId::Spine.into(), PierceType::always(HumanPartId::Torso.into())),
                        (HumanPartId::Torso.into(), no_pierce()),
                        (HumanPartId::Arm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Arm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Right).into(), no_pierce())
                    ],
                    Side2d::Front => vec![
                        (HumanPartId::Torso.into(), PierceType::torso_front()),
                        (HumanPartId::Arm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Arm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Right).into(), no_pierce())
                    ],
                    Side2d::Left => vec![
                        (HumanPartId::Spine.into(), PierceType::middle_pierce(Side1d::Left)),
                        (HumanPartId::Torso.into(), PierceType::middle_pierce(Side1d::Left)),
                        (HumanPartId::Arm(Side1d::Left).into(), PierceType::arm_pierce(Side1d::Left)),
                        (HumanPartId::Forearm(Side1d::Left).into(), PierceType::arm_pierce(Side1d::Left)),
                        (HumanPartId::Hand(Side1d::Left).into(), PierceType::arm_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::Spine.into(), PierceType::middle_pierce(Side1d::Right)),
                        (HumanPartId::Torso.into(), PierceType::middle_pierce(Side1d::Right)),
                        (HumanPartId::Arm(Side1d::Right).into(), PierceType::arm_pierce(Side1d::Right)),
                        (HumanPartId::Forearm(Side1d::Right).into(), PierceType::arm_pierce(Side1d::Right)),
                        (HumanPartId::Hand(Side1d::Right).into(), PierceType::arm_pierce(Side1d::Right))
                    ]
                }
            },
            DamageHeight::Bottom =>
            {
                match damage.direction.side
                {
                    Side2d::Back | Side2d::Front => vec![
                        (HumanPartId::Pelvis.into(), no_pierce()),
                        (HumanPartId::Thigh(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Calf(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Foot(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Thigh(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Calf(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Foot(Side1d::Right).into(), no_pierce())
                    ],
                    Side2d::Left => vec![
                        (HumanPartId::Pelvis.into(), PierceType::pelvis(Side1d::Left)),
                        (HumanPartId::Thigh(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Calf(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Foot(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::Pelvis.into(), PierceType::pelvis(Side1d::Right)),
                        (HumanPartId::Thigh(Side1d::Right).into(), PierceType::leg_pierce(Side1d::Right)),
                        (HumanPartId::Calf(Side1d::Right).into(), PierceType::leg_pierce(Side1d::Right)),
                        (HumanPartId::Foot(Side1d::Right).into(), PierceType::leg_pierce(Side1d::Right))
                    ]
                }
            }
        };

        ids.retain(|(id, pierce)|
        {
            self.body.get::<()>(*id).is_some() || pierce.any_exists(self)
        });

        let ids: &Vec<_> = &ids;

        let picked = WeightedPicker::pick_from(
            fastrand::f64(),
            ids,
            |(id, pierce)|
            {
                self.body.get::<SizeGetter>(*id).copied().unwrap_or_else(|| pierce.combined_scale(self))
            }
        );

        picked.and_then(|(picked, on_pierce)|
        {
            let picked_damage = self.body.get_mut::<DamagerGetter>(*picked).map(|x| x(damage.clone()));

            if let Some(damage) = picked_damage
            {
                damage.and_then(|pierce|
                {
                    (on_pierce.action)(self, pierce)
                })
            } else
            {
                (on_pierce.action)(self, damage)
            }
        })
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(from = "HumanAnatomyValues")]
#[serde(into = "HumanAnatomyValues")]
pub struct HumanAnatomy
{
    this: HumanAnatomyValues,
    cached: Option<CachedProps>
}

impl From<HumanAnatomyValues> for HumanAnatomy
{
    fn from(this: HumanAnatomyValues) -> Self
    {
        let mut this = Self{
            this,
            cached: None
        };

        this.update_cache();

        this
    }
}

impl HumanAnatomy
{
    pub fn new(info: HumanAnatomyInfo) -> Self
    {
        Self::from(HumanAnatomyValues::new(info))
    }

    pub fn update(&mut self, dt: f32) -> bool
    {
        let cached = self.cached.as_ref().unwrap();

        self.this.blood.change(cached.blood_change * dt);

        {
            let oxygen_change = cached.oxygen_change + self.this.external_oxygen_change;
            self.this.oxygen.change(oxygen_change * dt);
        }

        if self.this.oxygen.current == 0.0
        {
            self.this.hypoxic += dt;
        } else
        {
            self.this.hypoxic = 0.0;
        }

        if self.this.hypoxic > 1.0
        {
            let is_damaged = {
                let damager = self.this.body.get_mut::<DamagerGetter>(OrganId::Brain(None, None).into());

                let is_damaged = damager.is_some();

                if let Some(damager) = damager
                {
                    damager(Damage::area_each(0.01 * dt));
                }

                is_damaged
            };

            if is_damaged
            {
                self.on_damage();
            }

            is_damaged
        } else
        {
            false
        }
    }

    fn cached(&self) -> &CachedProps
    {
        self.cached.as_ref().unwrap()
    }

    pub fn body(&self) -> &HumanBody
    {
        &self.this.body
    }

    pub fn is_dead(&self) -> bool
    {
        !self.can_move() && self.strength() == 0.0
    }

    pub fn take_killed(&mut self) -> bool
    {
        if let Some(killed) = self.this.killed.as_mut()
        {
            if *killed
            {
                *killed = false;
                return true;
            }
        }

        false
    }

    pub fn speed(&self) -> f32
    {
        if self.is_crawling() { self.cached().speed.arms } else { self.cached().speed.legs }
    }

    pub fn can_move(&self) -> bool
    {
        self.cached().speed.arms != 0.0 || self.cached().speed.legs != 0.0
    }

    pub fn strength(&self) -> f32
    {
        self.cached().strength
    }

    pub fn oxygen_speed(&self) -> f32
    {
        self.cached().oxygen_change
    }

    pub fn oxygen(&self) -> SimpleHealth
    {
        self.this.oxygen
    }

    pub fn oxygen_mut(&mut self) -> &mut SimpleHealth
    {
        &mut self.this.oxygen
    }

    pub fn external_oxygen_change_mut(&mut self) -> &mut f32
    {
        &mut self.this.external_oxygen_change
    }

    pub fn vision(&self) -> f32
    {
        self.cached().vision
    }

    pub fn vision_angle(&self) -> f32
    {
        (self.vision() * 0.5).min(1.0) * f32::consts::PI
    }

    pub fn is_crawling(&self) -> bool
    {
        let crawl_threshold = self.cached().speed.arms * 0.9; // prefer walking

        self.this.override_crawling || (self.cached().speed.legs < crawl_threshold)
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.this.base_speed = speed;

        self.update_cache();
    }

    pub fn for_accessed_parts(&mut self, mut f: impl FnMut(ChangedPart))
    {
        {
            let f = &mut f;
            mem::take(&mut self.this.broken).into_iter().for_each(|broken|
            {
                f(ChangedPart::whole(broken));
            });
        }

        AnatomyId::iter().for_each(|id|
        {
            let f = &mut f;

            match id
            {
                AnatomyId::Part(id) =>
                {
                    if let Some(x) = self.this.body.get_part_mut::<AccessedGetter>(id)
                    {
                        x(&mut |kind| f(ChangedPart::Part(id, Some(kind))));
                    }
                },
                AnatomyId::Organ(id) =>
                {
                    if self.this.body.get_organ_mut::<AccessedGetter>(id).unwrap_or(false)
                    {
                        f(ChangedPart::Organ(id));
                    }
                }
            }
        });
    }

    pub fn get_health(&self, id: ChangedPart) -> Option<f32>
    {
        let body = &self.this.body;
        match id
        {
            ChangedPart::Part(x, kind) =>
            {
                if let Some(kind) = kind
                {
                    let health = match kind
                    {
                        ChangedKind::Bone => body.get_part::<BoneHealthGetter>(x).copied(),
                        ChangedKind::Muscle => body.get_part::<MuscleHealthGetter>(x).copied(),
                        ChangedKind::Skin => body.get_part::<SkinHealthGetter>(x).copied()
                    };

                    health.and_then(|x| x.fraction())
                } else
                {
                    body.get_part::<AverageHealthGetter>(x)
                }
            },
            ChangedPart::Organ(x) => body.get_organ::<AverageHealthGetter>(x).flatten()
        }
    }

    fn speed_scale(&self) -> Speeds
    {
        let brain = some_or_return!(self.brain());

        let motor = brain.as_ref().map(|hemisphere|
        {
            Speeds{
                arms: hemisphere.frontal.motor.arms.fraction().unwrap_or(0.0).powi(3),
                legs: hemisphere.frontal.motor.legs.fraction().unwrap_or(0.0).powi(3)
            }
        });

        self.body().spine.as_ref().map(|spine|
        {
            let arms = spine.arms.as_ref().map(|arm|
            {
                arm.as_ref().map(|x| x.arm_speed()).unwrap_or(0.0)
            }).map(|x|
            {
                x * spine.spine.contents.cervical.fraction().unwrap_or(0.0)
            });

            let legs = spine.pelvis.as_ref().map(|pelvis|
            {
                pelvis.legs.as_ref().map(|leg|
                {
                    leg.as_ref().map(|x| x.leg_speed()).unwrap_or(0.0)
                })
            }).unwrap_or_else(|| Halves::repeat(0.0)).map(|x|
            {
                x * spine.spine.contents.lumbar.fraction().unwrap_or(0.0)
            });

            arms.zip(legs).zip(motor.flip()).map(|((arms, legs), motor)|
            {
                Speeds{
                    legs: legs * motor.legs,
                    arms: arms * motor.arms
                }
            }).combine(|a, b| Speeds{legs: a.legs + b.legs, arms: a.arms + b.arms})
        }).unwrap_or_default()
    }

    fn brain(&self) -> Option<&Brain>
    {
        self.body().head.as_ref()?.contents.brain.as_ref()
    }

    fn updated_speed(&self) -> Speeds<f32>
    {
        self.speed_scale().map(|x|
        {
            self.this.base_speed * x
        })
    }

    pub fn override_crawling(&mut self, state: bool)
    {
        self.this.override_crawling = state;
    }

    fn updated_strength(&self) -> f32
    {
        let fraction = self.speed_scale().arms * 2.5;

        self.this.base_strength * fraction
    }

    fn lung(&self, side: Side1d) -> Option<&Lung>
    {
        let spine = self.body().spine.as_ref()?;
        let torso = spine.torso.as_ref()?;

        torso.contents.lungs.as_ref()[side].as_ref()
    }

    fn updated_blood_change(&self) -> f32
    {
        0.0
    }

    fn updated_oxygen_regen(&self) -> f32
    {
        let brain = some_or_return!(self.brain());

        let amount = brain.as_ref().map_sides(|side, hemisphere|
        {
            let lung = some_or_return!(self.lung(side.opposite()));

            lung.0.fraction().unwrap_or(0.0) * hemisphere.frontal.motor.body.fraction().unwrap_or(0.0).powi(3)
        }).combine(|a, b| a + b) / 2.0;

        let spine = some_or_return!(self.body().spine.as_ref());

        let nerve_fraction = spine.spine.contents.cervical.fraction().unwrap_or(0.0);

        let torso = some_or_return!(spine.torso.as_ref());

        let torso_muscle = torso.muscle.fraction().unwrap_or(0.0);

        0.25 * amount * torso_muscle * nerve_fraction
    }

    fn updated_oxygen_change(&self) -> f32
    {
        let regen = self.updated_oxygen_regen();
        let consumption = 0.05;

        regen - consumption
    }

    fn updated_oxygen_max(&self) -> f32
    {
        Halves{left: Side1d::Left, right: Side1d::Right}.map(|side|
        {
            some_or_return!(self.lung(side)).0.fraction().unwrap_or(0.0)
        }).combine(|a, b| a + b) / 2.0
    }

    fn updated_vision(&self) -> f32
    {
        let base = TILE_SIZE * 10.0;

        let brain = some_or_return!(self.brain());

        let vision = brain.as_ref().map(|hemisphere|
        {
            hemisphere.occipital.0.fraction().unwrap_or(0.0).powi(3)
        }).flip().zip(some_or_return!(self.body().head.as_ref()).contents.eyes.as_ref()).map(|(fraction, eye)|
        {
            eye.as_ref().and_then(|x| x.average_health()).unwrap_or(0.0) * fraction
        }).combine(|a, b| a.max(b));

        base * vision
    }

    fn update_cache(&mut self)
    {
        self.cached = Some(CachedProps{
            speed: self.updated_speed(),
            strength: self.updated_strength(),
            vision: self.updated_vision(),
            oxygen_change: self.updated_oxygen_change(),
            blood_change: self.updated_blood_change()
        });

        self.this.oxygen.set_max(self.updated_oxygen_max());

        if self.is_dead() && self.this.killed.is_none()
        {
            self.this.killed = Some(true);
        }
    }

    fn on_damage(&mut self)
    {
        self.this.body.detach_broken(|id| { self.this.broken.push(id); });
        self.update_cache();
    }
}

impl Damageable for HumanAnatomy
{
    fn damage(&mut self, mut damage: Damage) -> Option<Damage>
    {
        if self.is_crawling()
        {
            damage = damage * 2.0;
        }

        let damage = self.this.damage_random_part(damage);

        self.on_damage();

        damage
    }

    fn is_full(&self) -> bool
    {
        self.body().is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        let x = self.this.body.heal(amount);

        self.update_cache();

        x
    }
}
