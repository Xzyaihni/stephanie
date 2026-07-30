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
    DataInfos,
    AnyEntities,
    ServerScripts,
    Inventory,
    Anatomy,
    HumanAnatomy,
    Faction,
    Character,
    Enemy,
    EnemyId,
    Entity,
    EntityInfo,
    Health,
    Parent,
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
    infos: &DataInfos,
    scripts: &ServerScripts,
    id: EnemyId,
    pos: Vector3<f32>,
    params: Vec<SpawnEnemyParam>,
    mut create_entity: impl FnMut(EntityInfo) -> Entity
)
{
    let info = infos.enemies_info.get(id);

    let name = info.name.clone();

    let anatomy = Anatomy::Human(HumanAnatomy::new(info.anatomy.clone()));

    let mut inventory = Inventory::new(BASE_INVENTORY_LIMIT);
    let mut character = Character::new(info.character, Faction::Zob);

    let scale = infos.characters_info.get(info.character).normal.scale;

    let transform = Transform{
        position: pos,
        scale: with_z(scale, ENTITY_SCALE),
        rotation: random_rotation(),
        ..Default::default()
    };

    {
        let scripts = scripts.enemy_generator(id);

        scripts.on_contents.create(&infos.items_info)
            .into_iter()
            .for_each(|item| { inventory.push(&infos.items_info, item); });

        scripts.on_equip.create(&infos.items_info).into_iter().for_each(|item|
        {
            let item_info = infos.items_info.get(item.id);

            if let Some(clothing) = item_info.clothing.as_ref()
            {
                let slot = clothing.slot;

                let id = inventory.push(&infos.items_info, item);

                character.set_equip(slot, Some(id));
            } else
            {
                eprintln!("cant equip {}", item_info.name);
            }
        });
    }

    let mut enemy = Enemy::new(&infos.enemies_info, id);

    params.iter().any(|param|
    {
        if let SpawnEnemyParam::Aggro(attack_target) = param
        {
            enemy.set_attacking(*attack_target);

            true
        } else
        {
            false
        }
    });

    create_entity(EntityInfo{
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
    });

    params.iter().any(|param|
    {
        if let SpawnEnemyParam::Shield(shield_target) = param
        {
            let shield_sprite = infos.common_textures.shield;

            create_entity(EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    inherit_scale: false,
                    transform: Transform{
                        scale: with_z(shield_sprite.scale, ENTITY_SCALE),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(*shield_target)),
                render: Some(RenderInfo{
                    object: Some(RenderObject{
                        kind: RenderObjectKind::TextureId{id: shield_sprite.id}
                    }),
                    z_level: ZLevel::Shield,
                    mix: Some(MixColor{color: [0.0; 4], amount: 0.6, palette: Some(ColorPalette::Purple), only_alpha: true, ..Default::default()}),
                    ..Default::default()
                }),
                health: Some(Health::Normal(1.0)),
                saveable: Some(()),
                ..Default::default()
            });

            true
        } else
        {
            false
        }
    });
}
