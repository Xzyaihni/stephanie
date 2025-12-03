use std::{
    f32,
    mem,
    rc::Rc
};

use serde::{Serialize, Deserialize};

use crate::{
    debug_config::*,
    common::{
        falloff,
        lerp,
        some_or_return,
        some_or_unexpected_return,
        TILE_SIZE,
        damage::*,
        WeightedAverager,
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
    OrgansDamagerGetter,
    SimpleHealth,
    ChangedKind,
    ChangedPart
};

pub use parts::{OrganId, AnatomyId, BrainId, HumanPartId, HumanPart};
use parts::*;

mod parts;


fn maybe_update(current_value: &mut f32, new_value: f32) -> bool
{
    let changed = *current_value != new_value;

    *current_value = new_value;

    changed
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Speeds<T=f32>
{
    pub arms: T,
    pub legs: T
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
    oxygen_regen: f32,
    oxygen_consumption: f32,
    blood_change: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomyInfo
{
    pub bone: f32,
    pub muscle: f32,
    pub skin: f32,
    pub base_speed: f32,
    pub base_strength: f32
}

impl Default for HumanAnatomyInfo
{
    fn default() -> Self
    {
        Self{
            bone: 1.0,
            muscle: 1.0,
            skin: 1.0,
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

                if let AnatomyId::Part(id) = id
                {
                    this.maybe_wound(id, &damage);
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

                if let AnatomyId::Part(id) = target
                {
                    this.maybe_wound(id, &damage);
                }

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

// pointless complexity, go!
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WoundKind
{
    Abrasion,
    Puncture{deep: bool},
    Laceration{deep: bool},
    Incision{deep: bool},
    Avulsion
}

impl WoundKind
{
    pub fn blood_loss(&self) -> f32
    {
        let factor = match self
        {
            Self::Abrasion => 0.03,
            Self::Puncture{deep} => if *deep { 0.1 } else { 0.05 },
            Self::Laceration{deep} => if *deep { 0.2 } else { 0.1 },
            Self::Incision{deep} => if *deep { 0.4 } else { 0.2 },
            Self::Avulsion => 1.0
        };

        factor * 0.1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wound
{
    pub part: HumanPartId,
    pub duration: SimpleHealth,
    pub kind: WoundKind
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomyValues
{
    base_speed: f32,
    base_strength: f32,
    encumbrance_speed: f32,
    crawling: bool,
    blood: SimpleHealth,
    oxygen: SimpleHealth,
    external_oxygen_change: f32,
    hypoxic: f32,
    fainted: f32,
    conscious: bool,
    body: HumanBody,
    wounds: Vec<Wound>,
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
    pub fn new(info: HumanAnatomyInfo) -> Self
    {
        let part = BodyPartInfo{
            bone: info.bone,
            muscle: info.muscle,
            skin: info.skin
        };

        let base_speed = info.base_speed * 12.0;
        let base_strength = info.base_strength;

        let new_part = |name, part: BodyPartInfo, size: f64|
        {
            HumanPart::new(name, part, size, ())
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

            let upper = new_part(
                DebugName::new(with_name("upper leg")),
                BodyPartInfo{bone: part.bone * 20.0, muscle: part.muscle * 0.5, ..part},
                0.6
            );

            let lower = new_part(
                DebugName::new(with_name("lower leg")),
                BodyPartInfo{bone: part.bone * 10.0, muscle: part.muscle * 0.5, ..part},
                0.44
            );

            let foot = {
                let mut x = new_part(
                    DebugName::new(with_name("foot")),
                    BodyPartInfo{bone: part.bone * 5.0, ..part},
                    0.17
                );

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

            let upper = new_part(
                DebugName::new(with_name("upper arm")),
                BodyPartInfo{bone: part.bone * 15.0, muscle: part.muscle * 0.5, ..part},
                0.2
            );

            let lower = new_part(
                DebugName::new(with_name("lower arm")),
                BodyPartInfo{bone: part.bone * 7.0, muscle: part.muscle * 0.5, ..part},
                0.17
            );

            let hand = {
                let mut x = new_part(
                    DebugName::new(with_name("hand")),
                    BodyPartInfo{bone: part.bone * 4.0, ..part},
                    0.07
                );

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

        let spine = HumanPart::new_full(
            DebugName::new("spine"),
            Health::new(0.999, part.bone * 40.0 * 10.0), // * 10 for missing muscle protection
            Health::new(0.5, part.skin * 10.0),
            Health::new(0.0, 0.0),
            0.1,
            SpinalCord::new(1.0)
        );

        let head = {
            let mut head = HumanPart::new_full(
                DebugName::new("head"),
                Health::new(0.95, part.bone * 100.0),
                Health::new(0.5, part.skin),
                Health::new(0.0, 0.0),
                0.39,
                HeadOrgans{eyes: Halves::repeat(Some(Eye::new(1.0))), brain: Some(Brain::new(0.5))}
            );

            head.muscle = Health::zero().into();

            head
        };

        let pelvis = new_part(
            DebugName::new("pelvis"),
            BodyPartInfo{bone: part.bone * 60.0, ..part},
            0.37
        );

        let pelvis = Pelvis{
            pelvis,
            legs: Halves{left: make_leg("left"), right: make_leg("right")}
        };

        let torso = HumanPart::new_full(
            DebugName::new("torso"),
            Health::new(0.8, part.bone * 9.0),
            Health::new(0.5, part.skin * 2.0),
            Health::new(0.9, part.muscle * 5.0),
            0.82,
            TorsoOrgans{
                lungs: Halves::repeat(Some(Lung::new(1.0))),
                heart: Some(Heart::new(3.0))
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
            encumbrance_speed: 1.0,
            crawling: false,
            blood: SimpleHealth::new(4.0),
            oxygen: SimpleHealth::new(1.0),
            external_oxygen_change: 0.0,
            hypoxic: 0.0,
            fainted: 0.0,
            conscious: true,
            body,
            wounds: Vec::new(),
            broken: Vec::new(),
            killed: None
        }
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

        let ids = if ids.is_empty()
        {
            HumanPartId::iter().filter(|id|
            {
                self.body.get_part::<()>(*id).is_some()
            }).map(|id|
            {
                (AnatomyId::Part(id), PierceType::empty())
            }).collect::<Vec<_>>()
        } else
        {
            ids
        };

        let picked = WeightedPicker::pick_from(
            fastrand::f64(),
            &ids,
            |(id, pierce)|
            {
                self.body.get::<SizeGetter>(*id).copied().unwrap_or_else(|| pierce.combined_scale(self))
            }
        );

        picked.and_then(|(picked, on_pierce)|
        {
            if let AnatomyId::Part(id) = picked
            {
                self.maybe_wound(*id, &damage);
            }

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

    fn maybe_wound(&mut self, id: HumanPartId, damage: &Damage)
    {
        let blunt_damage = |x: f32| -> Option<WoundKind>
        {
            let abrasion_chance = falloff(1.0, x * 0.3);

            let laceration_chance = falloff(1.0, x * 0.1);
            let deep_laceration_chance = laceration_chance * 0.5;

            if DebugConfig::is_enabled(DebugTool::PrintDamage)
            {
                eprintln!(
                    "[{id:?} blunt damage] abrasion chance: {:.3}%",
                    abrasion_chance * 100.0
                );

                eprintln!(
                    "[{id:?} blunt damage] laceration chance: {:.3}% (deep {:.3}%)",
                    abrasion_chance * laceration_chance * 100.0,
                    deep_laceration_chance * 100.0
                );
            }

            if fastrand::f32() >= abrasion_chance
            {
                return None;
            }

            if fastrand::f32() < laceration_chance
            {
                Some(WoundKind::Laceration{deep: fastrand::f32() < deep_laceration_chance})
            } else
            {
                Some(WoundKind::Abrasion)
            }
        };

        let is_poke = damage.poke;
        let kind = match damage.data
        {
            DamageType::AreaEach(_) => return,
            DamageType::Blunt(x) => some_or_return!(blunt_damage(x)),
            DamageType::Sharp{sharpness, damage} =>
            {
                let sharp_chance = falloff(1.8, sharpness * 1.5);

                if DebugConfig::is_enabled(DebugTool::PrintDamage)
                {
                    eprintln!("{sharpness} sharp chance: {:.3}%", sharp_chance * 100.0);
                }

                if fastrand::f32() < sharp_chance
                {
                    let sharp_damage = damage * sharpness;

                    if is_poke
                    {
                        let puncture_chance = falloff(1.0, sharp_damage);
                        let deep_puncture_chance = puncture_chance * 0.5;

                        if DebugConfig::is_enabled(DebugTool::PrintDamage)
                        {
                            eprintln!(
                                "[{id:?} poke sharp damage] puncture chance: {:.3}% (deep {:.3}%)",
                                puncture_chance * 100.0,
                                deep_puncture_chance * 100.0
                            );
                        }

                        if fastrand::f32() >= puncture_chance
                        {
                            return;
                        }

                        if fastrand::f32() < 0.1
                        {
                            WoundKind::Incision{deep: true}
                        } else
                        {
                            WoundKind::Puncture{deep: fastrand::f32() < deep_puncture_chance}
                        }
                    } else
                    {
                        let incision_chance = falloff(1.0, sharp_damage);
                        let deep_incision_chance = incision_chance * 0.5;

                        if DebugConfig::is_enabled(DebugTool::PrintDamage)
                        {
                            eprintln!(
                                "[{id:?} non poke sharp damage] incision chance: {:.3}% (deep {:.3}%)",
                                incision_chance * 100.0,
                                deep_incision_chance * 100.0
                            );
                        }

                        if fastrand::f32() >= incision_chance
                        {
                            return;
                        }

                        WoundKind::Incision{deep: fastrand::f32() < deep_incision_chance}
                    }
                } else
                {
                    some_or_return!(blunt_damage(damage * (1.0 + sharpness * 2.0)))
                }
            },
            DamageType::Bullet(_) =>
            {
                WoundKind::Puncture{deep: true}
            }
        };

        let relative_damage = damage.data.as_flat();

        let wound = Wound{
            part: id,
            duration: lerp(1.0, 50.0, falloff(1.0, relative_damage).clamp(0.0, 1.0)).into(),
            kind
        };

        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("[{id:?} damage] wound: {wound:#?}");
        }

        self.wounds.push(wound);
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

    pub fn update(&mut self, is_player: bool, dt: f32) -> bool
    {
        let cached = self.cached.as_ref().unwrap();

        {
            let blood_loss: f32 = self.this.wounds.iter().map(|wound|
            {
                wound.kind.blood_loss() * wound.duration.fraction().unwrap_or(0.0)
            }).sum();

            let blood_change = cached.blood_change - blood_loss;

            self.this.blood.change(blood_change * dt);
        }

        self.this.wounds.retain_mut(|wound|
        {
            let rate = if self.this.hypoxic > 0.0 { 0.3 } else { 1.0 };
            wound.duration.current -= rate * dt;

            wound.duration.current > 0.0
        });

        {
            let internal_oxygen_change = (cached.oxygen_regen * self.this.blood.fraction().unwrap_or(0.0)) - cached.oxygen_consumption;

            let oxygen_change = internal_oxygen_change + self.this.external_oxygen_change;
            self.this.oxygen.change(oxygen_change * dt);
        }

        if self.this.oxygen.current == 0.0
        {
            self.this.hypoxic += dt;
        } else
        {
            self.this.hypoxic = 0.0;
        }

        self.this.fainted = (self.this.fainted - dt).max(0.0);

        let is_winded = self.this.hypoxic > 0.1;

        if is_winded && !is_player
        {
            self.this.fainted = 15.0;
        }

        let is_conscious = !self.is_dead() && self.this.fainted == 0.0;

        let mut changed = false;

        if self.this.conscious != is_conscious
        {
            self.this.conscious = is_conscious;
            changed = true;
        }

        if self.this.hypoxic > 10.0
        {
            let is_damaged = {
                let damager = self.this.body.get_mut::<DamagerGetter>(OrganId::Brain(None, None).into());

                let is_damaged = damager.is_some();

                if let Some(damager) = damager
                {
                    damager(Damage::area_each(0.005 * dt));
                }

                is_damaged
            };

            if is_damaged
            {
                self.on_damage();
                changed = true;
            }
        }

        changed
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
        let cached = self.cached();

        cached.speed.arms == 0.0
            && cached.speed.legs == 0.0
            && self.strength() == 0.0
    }

    pub fn is_conscious(&self) -> bool
    {
        self.this.conscious
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
        if !self.can_move()
        {
            return 0.0;
        }

        if self.is_crawling() { self.cached().speed.arms } else { self.cached().speed.legs }
    }

    pub fn can_move(&self) -> bool
    {
        self.this.conscious && (self.cached().speed.arms != 0.0 || self.cached().speed.legs != 0.0)
    }

    pub fn strength(&self) -> f32
    {
        self.cached().strength
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

    pub fn blood(&self) -> SimpleHealth
    {
        self.this.blood
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
        self.this.crawling
    }

    pub fn is_standing(&self) -> bool
    {
        !self.is_crawling() && self.can_move()
    }

    pub fn set_encumbrance_speed(&mut self, speed: f32)
    {
        if maybe_update(&mut self.this.encumbrance_speed, speed)
        {
            self.update_cache();
        }
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        if maybe_update(&mut self.this.base_speed, speed)
        {
            self.update_cache();
        }
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


    pub fn bone_heal(&mut self, amount: u32) -> bool
    {
        (0..amount).any(|_|
        {
            HumanPartId::iter().any(|id|
            {
                if let Some(bone) = self.this.body.get_part_mut::<BoneHealthGetter>(id)
                {
                    if bone.is_zero()
                    {
                        bone.heal_remainder(bone.health.max * 0.1);

                        true
                    } else
                    {
                        false
                    }
                } else
                {
                    false
                }
            })
        })
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
                let gluteal = pelvis.pelvis.muscle.fraction().unwrap_or(0.0);

                pelvis.legs.as_ref().map(|leg|
                {
                    leg.as_ref().map(|x| x.leg_speed()).unwrap_or(0.0) * gluteal
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
            self.this.base_speed * self.this.encumbrance_speed * x
        })
    }

    pub fn speeds(&self) -> Speeds
    {
        self.cached().speed
    }

    pub fn set_crawling(&mut self, state: bool)
    {
        self.this.crawling = state;
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

        let heart_health = torso.contents.heart.as_ref().and_then(|x| x.0.fraction()).unwrap_or(0.0);

        0.25 * amount * torso_muscle * nerve_fraction * heart_health
    }

    fn updated_oxygen_consumption(&self) -> f32
    {
        let mut avger = WeightedAverager::new();

        avger.add(10.0, self.brain().and_then(|x| x.average_health()).unwrap_or(0.0));

        [Side1d::Left, Side1d::Right].into_iter().for_each(|side|
        {
            avger.add(1.0, self.lung(side).and_then(|x| x.average_health()).unwrap_or(0.0));
        });

        avger.average() * 0.05
    }

    fn updated_blood_change(&self) -> f32
    {
        let (amount, total) = [
            HumanPartId::Spine,
            HumanPartId::Torso,
            HumanPartId::Pelvis,
            HumanPartId::Thigh(Side1d::Left),
            HumanPartId::Thigh(Side1d::Right),
            HumanPartId::Arm(Side1d::Left),
            HumanPartId::Arm(Side1d::Right)
        ].into_iter().fold((0, 0.0), |(amount, total), id|
        {
            let health = self.this.body.get_part::<BoneHealthGetter>(id).and_then(|x| x.fraction()).unwrap_or(0.0);

            (amount + 1, total + health)
        });

        let fraction = total / amount as f32;

        // around a month for 4 liters of blood, i guess like 5 mins is reasonable lol
        let change = 4.0 / (60.0 * 5.0);

        fraction * change
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
            oxygen_regen: self.updated_oxygen_regen(),
            oxygen_consumption: self.updated_oxygen_consumption(),
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
        let was_standing = self.is_standing();

        HumanPartId::iter().for_each(|id|
        {
            if let Some(muscle) = self.this.body.get_part::<MuscleHealthGetter>(id)
            {
                let has_muscle = muscle.health.max != 0.0;

                let remove_skin = if has_muscle
                {
                    muscle.is_zero()
                } else
                {
                    some_or_unexpected_return!(self.this.body.get_part::<BoneHealthGetter>(id)).is_zero()
                };

                if remove_skin
                {
                    some_or_unexpected_return!(self.this.body.get_part_mut::<SkinHealthGetter>(id)).health.current = 0.0;
                }
            }
        });

        {
            let mut any_detached = false;

            self.this.body.detach_broken(|id|
            {
                any_detached = true;

                if let AnatomyId::Part(id) = id
                {
                    let wound = Wound{
                        part: id,
                        duration: 60.0.into(),
                        kind: WoundKind::Avulsion
                    };

                    if DebugConfig::is_enabled(DebugTool::PrintDamage)
                    {
                        eprintln!("[{id:?} break] detaching with: {wound:?}");
                    }

                    self.this.wounds.push(wound);
                }

                self.this.broken.push(id);
            });

            if any_detached
            {
                self.this.wounds.retain(|wound| self.this.body.get_part::<()>(wound.part).is_some());
            }
        }

        self.update_cache();

        if was_standing && !self.is_standing()
        {
            Self::fall_damage(self, true, 1.0);
        }
    }

    fn fall_damage(&mut self, body_height: bool, damage: f32)
    {
        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("fall damage {damage}");
        }

        let side = if fastrand::bool() { Side1d::Left } else { Side1d::Right };
        let opposite_side = side.opposite();

        let parts_fall_info = [
            (HumanPartId::Foot(side), 0.2, 0.95),
            (HumanPartId::Foot(opposite_side), 0.2, 0.95),
            (HumanPartId::Calf(side), 1.0, 0.8),
            (HumanPartId::Calf(opposite_side), 1.0, 0.8),
            (HumanPartId::Thigh(side), 1.0, 0.5),
            (HumanPartId::Thigh(opposite_side), 1.0, 0.5),
            (HumanPartId::Pelvis, 1.0, 0.9),
            (HumanPartId::Spine, 1.0, 0.2),
            (HumanPartId::Hand(side), 1.0, 0.9),
            (HumanPartId::Hand(opposite_side), 1.0, 0.9),
            (HumanPartId::Forearm(side), 1.0, 0.5),
            (HumanPartId::Forearm(opposite_side), 1.0, 0.5),
            (HumanPartId::Arm(side), 1.0, 0.5),
            (HumanPartId::Arm(opposite_side), 1.0, 0.5),
            (HumanPartId::Torso, 1.0, 0.9),
            (HumanPartId::Head, 1.0, 0.2)
        ];

        let parts = if !body_height
        {
            if self.is_standing()
            {
                parts_fall_info
            } else
            {
                let mut parts = parts_fall_info;
                fastrand::shuffle(&mut parts);

                parts
            }
        } else
        {
            parts_fall_info.into_iter().rev().collect::<Vec<_>>().try_into().unwrap()
        };

        parts.into_iter().fold(damage, |damage, (id, scale, damping)|
        {
            if let Some(muscle) = self.this.body.get_part_mut::<MuscleHealthGetter>(id)
            {
                let min_health = muscle.health.max * 0.1;
                let max_change = (muscle.health.current - min_health).max(0.0);
                let change = (damage * scale * 0.5).min(max_change);

                muscle.health.subtract_hp(change);
            }

            if let Some(bone) = self.this.body.get_part_mut::<BoneHealthGetter>(id)
            {
                let abrasion_chance = falloff(1.0, damage * 0.5);

                if DebugConfig::is_enabled(DebugTool::PrintDamage)
                {
                    eprintln!("[fall damage] abrasion chance: {:.3}%", abrasion_chance * 100.0);
                }

                if fastrand::f32() < abrasion_chance
                {
                    let laceration_chance = falloff(1.0, damage * 0.1);

                    if DebugConfig::is_enabled(DebugTool::PrintDamage)
                    {
                        eprintln!("[fall damage] laceration chance: {:.3}%", laceration_chance * abrasion_chance * 100.0);
                    }

                    let kind = if fastrand::f32() < laceration_chance
                    {
                        WoundKind::Laceration{deep: false}
                    } else
                    {
                        WoundKind::Abrasion
                    };

                    let wound = Wound{
                        part: id,
                        duration: lerp(1.0, 50.0, falloff(1.0, damage * 0.2).clamp(0.0, 1.0)).into(),
                        kind
                    };

                    if DebugConfig::is_enabled(DebugTool::PrintDamage)
                    {
                        eprintln!("[fall damage] wound: {wound:#?}");
                    }

                    self.this.wounds.push(wound);
                }

                let organ_damage = bone.simple_pierce(damage * scale);
                if let Some(organ_damager) = self.this.body.get_part_mut::<OrgansDamagerGetter>(id)
                {
                    organ_damager(Side2d::random(), DamageType::Blunt(organ_damage.unwrap_or(0.0)));
                }

                damage * damping
            } else
            {
                damage
            }
        });

        self.on_damage();
    }
}

impl Damageable for HumanAnatomy
{
    fn damage(&mut self, mut damage: Damage) -> Option<Damage>
    {
        if !self.is_standing()
        {
            damage = damage * 2.0;
        }

        let damage = self.this.damage_random_part(damage);

        self.on_damage();

        damage
    }

    fn fall_damage(&mut self, damage: f32)
    {
        Self::fall_damage(self, false, damage)
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
