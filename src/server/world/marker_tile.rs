use std::{
    f32,
    any::type_name,
    str::FromStr
};

use nalgebra::{Vector2, Vector3};

use strum::{EnumString, IntoStaticStr};

use yanyaengine::Transform;

use crate::{
    server::ConnectionsHandler,
    common::{
        furniture_creator,
        enemy_creator,
        rotate_point_z_3d,
        some_or_return,
        lazy_transform::*,
        render_info::*,
        collider::*,
        physics::*,
        joint::*,
        EntityPasser,
        Loot,
        EnemiesInfo,
        EntityInfo,
        Occluder,
        Parent,
        Light,
        entity::ServerEntities,
        lisp::{self, *},
        world::{
            TILE_SIZE,
            Pos3,
            ChunkLocal,
            TileRotation
        }
    }
};


fn parse_enum<T: FromStr<Err=strum::ParseError>>(value: OutputWrapperRef) -> Result<T, lisp::Error>
{
    let name = value.as_symbol()?.to_lowercase();
    T::from_str(&name).map_err(|err| lisp::Error::Custom(format!("{} parse error: {err}", type_name::<T>())))
}

#[derive(Debug, Clone, EnumString, IntoStaticStr)]
#[strum(ascii_case_insensitive)]
pub enum DoorMaterial
{
    Metal,
    Wood
}

#[derive(Debug, Clone)]
pub struct MarkerTile
{
    pub kind: MarkerKind,
    pub pos: ChunkLocal
}

impl MarkerTile
{
    pub fn create(
        self,
        writer: &mut ConnectionsHandler,
        entities: &mut ServerEntities,
        enemies: &EnemiesInfo,
        loot: &Loot,
        chunk_pos: Pos3<f32>
    )
    {
        let pos = chunk_pos + self.pos.pos().map(|x| x as f32 * TILE_SIZE);

        let half_tile = TILE_SIZE / 2.0;
        let position = Vector3::from(pos) + Vector3::repeat(half_tile);

        let mut add_entity = |info|
        {
            let (message, entity) = entities.push_message(info);

            writer.send_message(message);

            entity
        };

        match self.kind
        {
            MarkerKind::Enemy{} =>
            {
                let picked = some_or_return!(enemies.weighted_random(1.0));

                add_entity(enemy_creator::create(
                    enemies,
                    loot,
                    picked,
                    position
                ));
            },
            MarkerKind::Furniture{} =>
            {
                add_entity(furniture_creator::create(loot, position));
            },
            MarkerKind::Light{strength, offset} =>
            {
                add_entity(EntityInfo{
                    transform: Some(Transform{
                        position: position + offset,
                        scale: Vector3::repeat(TILE_SIZE),
                        ..Default::default()
                    }),
                    light: Some(Light{source: None, strength}),
                    saveable: Some(()),
                    ..Default::default()
                });
            },
            MarkerKind::Door{rotation, material, width} =>
            {
                let offset_inside = 0.15;

                let rotation = rotation.to_angle() + f32::consts::PI;

                let mut position = position;
                position += rotate_point_z_3d(
                    Vector3::new(-(TILE_SIZE / 2.0 + TILE_SIZE * offset_inside), 0.0, 0.0),
                    rotation
                );

                let hinge = add_entity(EntityInfo{
                    transform: Some(Transform{
                        position,
                        scale: Vector3::repeat(TILE_SIZE),
                        rotation,
                        ..Default::default()
                    }),
                    saveable: Some(()),
                    ..Default::default()
                });

                let texture = format!(
                    "furniture/{}_door{width}.png",
                    <&str>::from(material).to_lowercase()
                );

                add_entity(EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        scaling: Scaling::Ignore,
                        transform: Transform{
                            position: rotate_point_z_3d(
                                Vector3::new((0.5 * width as f32) + offset_inside / 2.0, 0.0, 0.0),
                                rotation
                            ),
                            scale: Vector2::new(1.0 * width as f32 + offset_inside, 0.3).xyx(),
                            ..Default::default()
                        },
                        inherit_rotation: false,
                        ..Default::default()
                    }.into()),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::Texture{
                            name: texture.to_owned()
                        }.into()),
                        shadow_visible: true,
                        z_level: ZLevel::Door,
                        ..Default::default()
                    }),
                    collider: Some(ColliderInfo{
                        kind: ColliderType::Rectangle,
                        layer: ColliderLayer::Door,
                        ..Default::default()
                    }.into()),
                    physical: Some(PhysicalProperties{
                        inverse_mass: (10.0 * width as f32).recip(),
                        restitution: 0.0,
                        floating: true,
                        move_z: false,
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(hinge, true)),
                    saveable: Some(()),
                    occluder: Some(Occluder::Door),
                    joint: Some(Joint::Hinge(HingeJoint{
                        origin: Vector3::new(-0.5, 0.0, 0.0),
                        angle_limit: Some(HingeAngleLimit{
                            base: rotation,
                            distance: f32::consts::FRAC_PI_2 * 0.9
                        })
                    })),
                    ..Default::default()
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum MarkerKind
{
    Enemy{},
    Furniture{},
    Door{rotation: TileRotation, material: DoorMaterial, width: u32},
    Light{strength: f32, offset: Vector3<f32>}
}

impl MarkerKind
{
    pub fn from_lisp_value(value: OutputWrapperRef) -> Result<Vec<Self>, lisp::Error>
    {
        value.as_pairs_list()?.into_iter().map(|value|
        {
            Self::read_single(value)
        }).collect()
    }

    fn read_single(value: OutputWrapperRef) -> Result<Self, lisp::Error>
    {
        let mut values = value.as_pairs_list()?.into_iter();

        let mut next_value = |name|
        {
            values.next().ok_or_else(|| lisp::Error::Custom(format!("expected {name}")))
        };

        let next_position = |value: Option<GenericOutputWrapper<&LispMemory>>|
        {
            value.map(|x| -> Result<_, _>
            {
                let lst = x.as_pairs_list()?;

                let mut values = lst.into_iter();
                let mut next_value = ||
                {
                    values.next().map(|x| x.as_float()).unwrap_or(Ok(0.0)).map(|x| x * TILE_SIZE)
                };

                Ok(Vector3::new(next_value()?, next_value()?, next_value()?))
            }).unwrap_or_else(|| Ok(Vector3::zeros()))
        };

        let id = next_value("marker tile id")?.as_symbol()?;

        match id.as_ref()
        {
            "door" =>
            {
                let rotation = parse_enum(next_value("door rotation")?)?;
                let material = parse_enum(next_value("door material")?)?;

                let width = next_value("door width")?.as_integer()? as u32;

                Ok(Self::Door{rotation, material, width})
            },
            "light" =>
            {
                let strength = next_value("light strength")?.as_float()?;
                let offset = next_position(next_value("").ok())?;

                Ok(Self::Light{strength, offset})
            },
            "enemy" =>
            {
                Ok(Self::Enemy{})
            },
            "furniture" =>
            {
                Ok(Self::Furniture{})
            },
            x => Err(lisp::Error::Custom(format!("unknown marker id `{x}`")))
        }
    }
}
