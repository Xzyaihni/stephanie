use std::f32;

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    random_rotation,
    render_info::*,
    physics::*,
    ENTITY_SCALE,
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


pub const ENEMY_MASS: f32 = 50.0;

pub fn create(
    enemies_info: &EnemiesInfo,
    loot: &Loot,
    id: EnemyId,
    pos: Vector3<f32>
) -> EntityInfo
{
    let info = enemies_info.get(id);

    let name = info.name.clone();

    let mut inventory = Inventory::new();
    loot.create(&name).into_iter().for_each(|item| { inventory.push(item); });

    let character = Character::new(info.character, Faction::Zob);

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
                position: pos,
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
            inverse_mass: (info.scale / ENTITY_SCALE) * ENEMY_MASS.recip(),
            fixed: PhysicalFixed{rotation: true, ..Default::default()},
            ..Default::default()
        }.into()),
        inventory: Some(inventory),
        anatomy: Some(Anatomy::Human(HumanAnatomy::new(info.anatomy.clone()))),
        character: Some(character),
        named: Some(name),
        enemy: Some(Enemy::new(enemies_info, id)),
        saveable: Some(()),
        ..Default::default()
    }
}
