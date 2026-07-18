use std::f32;

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::common::{
    with_z,
    random_rotation,
    render_info::*,
    physics::*,
    lisp::{self, *},
    ENTITY_SCALE,
    AnyEntities,
    ServerScripts,
    Inventory,
    Anatomy,
    HumanAnatomy,
    Faction,
    Character,
    Enemy,
    EnemyId,
    EnemiesInfo,
    Entity,
    CharactersInfo,
    ItemsInfo,
    EntityInfo,
    inventory::BASE_INVENTORY_LIMIT,
    scripts_container::{parse_symbol_or_string, parse_entity},
    lazy_transform::*
};


pub const ENEMY_MASS: f32 = 50.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpawnEnemyParam
{
    Aggro(Entity),
    Shield(Entity)
}

impl SpawnEnemyParam
{
    pub fn parse(
        entities: Option<&impl AnyEntities>,
        value: OutputWrapperRef
    ) -> Result<Self, lisp::Error>
    {
        let LispList{car, cdr} = value.as_list()?;

        let name = parse_symbol_or_string(car)?;

        let output = match (entities, name.as_ref())
        {
            (Some(entities), "aggro") => Self::Aggro(parse_entity(entities, cdr)?),
            (Some(entities), "shield") => Self::Shield(parse_entity(entities, cdr)?),
            (_, x) => return Err(lisp::Error::Custom(format!("{x} is not an enemy param")))
        };

        Ok(output)
    }
}

pub fn create(
    enemies_info: &EnemiesInfo,
    characters_info: &CharactersInfo,
    items_info: &ItemsInfo,
    scripts: &ServerScripts,
    id: EnemyId,
    pos: Vector3<f32>,
    params: Vec<SpawnEnemyParam>
) -> EntityInfo
{
    let info = enemies_info.get(id);

    let name = info.name.clone();

    let anatomy = Anatomy::Human(HumanAnatomy::new(info.anatomy.clone()));

    let mut inventory = Inventory::new(BASE_INVENTORY_LIMIT);
    let mut character = Character::new(info.character, Faction::Zob);

    let scale = characters_info.get(info.character).normal.scale;

    let transform = Transform{
        position: pos,
        scale: with_z(scale, ENTITY_SCALE),
        rotation: random_rotation(),
        ..Default::default()
    };

    {
        let scripts = scripts.enemy_generator(id);

        scripts.on_contents.create(items_info)
            .into_iter()
            .for_each(|item| { inventory.push(items_info, item); });

        scripts.on_equip.create(items_info).into_iter().for_each(|item|
        {
            let item_info = items_info.get(item.id);

            if let Some(clothing) = item_info.clothing.as_ref()
            {
                let slot = clothing.slot;

                let id = inventory.push(items_info, item);

                character.set_equip(slot, Some(id));
            } else
            {
                eprintln!("cant equip {}", item_info.name);
            }
        });
    }

    let mut enemy = Enemy::new(enemies_info, id);

    if let Some(SpawnEnemyParam::Aggro(attack_target)) = params.iter().find(|param| matches!(param, SpawnEnemyParam::Aggro(_)))
    {
        enemy.set_attacking(*attack_target);
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
            transform,
            ..Default::default()
        }.into()),
        render: Some(RenderInfo{
            z_level: ZLevel::Head,
            aspect: Aspect::KeepMax,
            ..Default::default()
        }),
        physical: Some(PhysicalProperties{
            inverse_mass: (scale.max() / ENTITY_SCALE) * ENEMY_MASS.recip(),
            fixed: PhysicalFixed{rotation: true, ..Default::default()},
            ..Default::default()
        }.into()),
        inventory: Some(inventory),
        anatomy: Some(anatomy),
        character: Some(character),
        named: Some(name),
        enemy: Some(enemy),
        saveable: Some(()),
        ..Default::default()
    }
}
