use std::f32;

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    random_rotation,
    render_info::*,
    physics::*,
    ENTITY_SCALE,
    ItemsInfo,
    Loot,
    Inventory,
    Anatomy,
    HumanAnatomy,
    Faction,
    Character,
    Enemy,
    EnemyId,
    EnemiesInfo,
    EntityInfo,
    lazy_transform::*
};


pub struct EnemyBuilder<'a>
{
    enemies_info: &'a EnemiesInfo,
    items_info: &'a ItemsInfo,
    pos: Vector3<f32>,
    id: EnemyId
}

impl<'a> EnemyBuilder<'a>
{
    pub fn new(
        enemies_info: &'a EnemiesInfo,
        items_info: &'a ItemsInfo,
        id: EnemyId,
        pos: Vector3<f32>
    ) -> Self
    {
        Self{enemies_info, items_info, pos, id}
    }

    pub fn build(self) -> EntityInfo
    {
        let info = self.enemies_info.get(self.id);

        let mut inventory = Inventory::new();

        let mut loot = Loot::new(
            self.items_info,
            vec!["utility", "weapons", "animals"],
            info.loot_commonness * 0.6
        );

        loot.create_random(&mut inventory, 1..4);

        let mut character = Character::new(info.character, Faction::Zob);

        if fastrand::f32() < 0.1
        {
            character.set_holding(Some(inventory.random()));
        }

        EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                rotation: Rotation::EaseOut(
                    EaseOutRotation{
                        decay: 6.0,
                        speed_significant: 0.0,
                        momentum: 0.0
                    }.into()
                ),
                transform: Transform{
                    position: self.pos,
                    scale: Vector3::repeat(info.scale),
                    rotation: random_rotation(),
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            render: Some(RenderInfo{
                z_level: ZLevel::Head,
                aspect: Aspect::KeepMax,
                ..Default::default()
            }),
            physical: Some(PhysicalProperties{
                inverse_mass: (info.scale / ENTITY_SCALE) * 50.0_f32.recip(),
                static_friction: 0.9,
                dynamic_friction: 0.8,
                fixed: PhysicalFixed{rotation: true, ..Default::default()},
                ..Default::default()
            }.into()),
            inventory: Some(inventory),
            anatomy: Some(Anatomy::Human(HumanAnatomy::new(info.anatomy.clone()))),
            character: Some(character),
            named: Some(self.enemies_info.get(self.id).name.clone()),
            enemy: Some(Enemy::new(self.enemies_info, self.id)),
            ..Default::default()
        }
    }
}
