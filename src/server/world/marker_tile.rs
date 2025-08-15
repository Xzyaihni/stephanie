use std::{
    f32,
    any::type_name,
    str::FromStr
};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::{
    server::ConnectionsHandler,
    common::{
        furniture_creator,
        enemy_creator,
        rotate_point_z_3d,
        render_info::*,
        collider::*,
        physics::*,
        door::*,
        lazy_transform::*,
        EntityPasser,
        Parent,
        Loot,
        EnemiesInfo,
        EntityInfo,
        FurnituresInfo,
        Light,
        watcher::Watchers,
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

pub struct CreateInfos<'a>
{
    pub enemies: &'a EnemiesInfo,
    pub furnitures: &'a FurnituresInfo
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
        CreateInfos{
            enemies,
            furnitures
        }: CreateInfos,
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
            MarkerKind::Enemy{name} =>
            {
                let id = if let Some(x) = enemies.get_id(&name)
                {
                    x
                } else
                {
                    eprintln!("cant find enemy named `{name}`");
                    return;
                };

                add_entity(enemy_creator::create(
                    enemies,
                    loot,
                    id,
                    position
                ));
            },
            MarkerKind::Furniture{name} =>
            {
                let id = if let Some(x) = furnitures.get_id(&name)
                {
                    x
                } else
                {
                    eprintln!("cant find furniture named `{name}`");
                    return;
                };

                add_entity(furniture_creator::create(furnitures, loot, id, position));
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
                let door = Door::new(position, rotation, material, width);

                let texture = door.texture();

                let transform = door.door_transform();

                let origin = Vector3::new(-0.5, 0.0, 0.0);

                let door_entity = add_entity(EntityInfo{
                    lazy_transform: Some(LazyTransformInfo{
                        transform: transform.clone(),
                        combine_origin_rotation: true,
                        origin_rotation_interpolation: Some(10.0),
                        origin,
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
                    collider: door.door_collider(),
                    physical: Some(PhysicalProperties{
                        inverse_mass: 0.0,
                        floating: true,
                        move_z: false,
                        ..Default::default()
                    }.into()),
                    watchers: Some(Watchers::new(Vec::new())),
                    occluder: door.door_occluder(),
                    saveable: Some(()),
                    ..Default::default()
                });

                let scale = if rotation.is_horizontal()
                {
                    Vector3::new(transform.scale.x, TILE_SIZE, transform.scale.z)
                } else
                {
                    Vector3::new(TILE_SIZE, transform.scale.x, transform.scale.z)
                };

                add_entity(EntityInfo{
                    transform: Some(Transform{
                        position: transform.position,
                        scale,
                        ..Default::default()
                    }),
                    collider: Some(ColliderInfo{
                        kind: ColliderType::Aabb,
                        layer: ColliderLayer::Door,
                        ghost: true,
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(door_entity, false)),
                    door: Some(door),
                    saveable: Some(()),
                    ..Default::default()
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum MarkerKind
{
    Enemy{name: String},
    Furniture{name: String},
    Door{rotation: TileRotation, material: DoorMaterial, width: u32},
    Light{strength: f32, offset: Vector3<f32>}
}

impl MarkerKind
{
    pub fn rotated(mut self, tile_rotation: TileRotation) -> Self
    {
        match &mut self
        {
            Self::Door{rotation, ..} =>
            {
                *rotation = rotation.combine(tile_rotation);
            },
            Self::Light{offset, ..} =>
            {
                *offset = rotate_point_z_3d(*offset, -(tile_rotation.to_angle() - f32::consts::FRAC_PI_2));
            },
            _ => ()
        }

        self
    }

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
                let rotation = TileRotation::from_lisp_value(*next_value("door rotation")?)?;
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
                let name = next_value("name")?.as_symbol()?;

                Ok(Self::Enemy{name})
            },
            "furniture" =>
            {
                let name = next_value("name")?.as_symbol()?;

                Ok(Self::Furniture{name})
            },
            x => Err(lisp::Error::Custom(format!("unknown marker id `{x}`")))
        }
    }
}
