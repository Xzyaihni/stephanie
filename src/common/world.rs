use std::{
    borrow::Borrow,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use vulkano::{
    buffer::subbuffer::BufferContents,
    pipeline::graphics::vertex_input::Vertex
};

use yanyaengine::{Transform, game_object::*};

use crate::{
    client::{
        VisibilityChecker,
        TilesFactory,
        world_receiver::{WorldReceiver, ChunkWorldReceiver}
    },
    common::{
        some_or_return,
        collider::*,
        TileMap,
        TileInfo,
        Entity,
        OccludingCaster,
        SpatialGrid,
        entity::ClientEntities,
        message::Message
    }
};

pub use overmap::{
    Overmap,
    FlatChunksContainer,
    ChunksContainer,
    chunks_container::debug_3d_slices,
    chunk::{
        self,
        CHUNK_SIZE,
        CHUNK_VISUAL_SIZE,
        TILE_SIZE,
        Axis,
        Pos3,
        Chunk,
        ChunkLocal,
        GlobalPos,
        LocalPos,
        PosDirection,
        DirectionsGroup,
        MaybeGroup,
        AlwaysGroup,
        tile::{Tile, TileExisting, TileRotation}
    }
};

pub use client_overmap::TilePos;
pub use visual_overmap::{OccludedChecker, OccludedCheckerInfo};

use client_overmap::ClientOvermap;
use visual_overmap::{VisualOvermap, VisualOvermapChunk, OccludedSlice};

pub use sky_light::SkyLight;

use overmap::OvermapIndexing;

pub mod overmap;

mod client_overmap;
mod visual_overmap;

mod sky_light;

pub mod pathfind;


pub const CLIENT_OVERMAP_SIZE: usize = 8;
pub const CLIENT_OVERMAP_SIZE_Z: usize = 3;

pub const DAY_LENGTH: f64 = 60.0 * 6.0;
pub const BETWEEN_LENGTH: f64 = 60.0 * 2.0;
pub const NIGHT_LENGTH: f64 = 60.0 * 5.0;

#[derive(BufferContents, Vertex, Debug, Clone, Copy)]
#[repr(C)]
pub struct SkyOccludingVertex
{
    #[format(R32G32_SFLOAT)]
    pub position: [f32; 2]
}

impl From<([f32; 4], [f32; 2])> for SkyOccludingVertex
{
    fn from(([x, y, _z, _w], _uv): ([f32; 4], [f32; 2])) -> Self
    {
        Self{position: [x, y]}
    }
}

#[derive(BufferContents, Vertex, Debug, Clone, Copy)]
#[repr(C)]
pub struct SkyLightVertex
{
    #[format(R32G32_SFLOAT)]
    pub position: [f32; 2],
    #[format(R32_SFLOAT)]
    pub intensity: f32
}

impl From<([f32; 4], [f32; 2])> for SkyLightVertex
{
    fn from(([x, y, _z, _w], [u, _v]): ([f32; 4], [f32; 2])) -> Self
    {
        Self{position: [x, y], intensity: u}
    }
}

#[derive(Debug, Clone)]
pub struct ChunkWithEntities
{
    pub chunk: Chunk,
    pub entities: Vec<Entity>
}

pub struct World
{
    tilemap: Arc<TileMap>,
    world_receiver: WorldReceiver,
    overmap: ClientOvermap,
    time_speed: f64,
    time: f64
}

impl World
{
    pub fn new(
        world_receiver: WorldReceiver,
        tiles_factory: TilesFactory,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>,
        time: f64
    ) -> Self
    {
        let tilemap = tiles_factory.tilemap().clone();
        let size = Self::overmap_size();

        let visual_overmap = VisualOvermap::new(tiles_factory, size, camera_size, player_position);
        let overmap = ClientOvermap::new(
            ChunkWorldReceiver::new(world_receiver.clone()),
            visual_overmap,
            size,
            player_position
        );

        Self{tilemap, world_receiver, overmap, time_speed: 1.0, time}
    }

    pub fn tilemap(&self) -> &TileMap
    {
        &self.tilemap
    }

    pub fn tile_info(&self, tile: Tile) -> &TileInfo
    {
        self.tilemap.info(tile)
    }

    pub fn overmap_size() -> Pos3<usize>
    {
        Pos3::new(CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE_Z)
    }

    pub fn zoom_limit() -> f32
    {
        //make the camera smaller by 3 tiles so theres time for the missing chunks to load
        let padding = 3;

        let padding = TILE_SIZE * padding as f32;

        let max_scale = (CLIENT_OVERMAP_SIZE - 1) as f32 * CHUNK_VISUAL_SIZE - padding;

        max_scale
    }

    pub fn inbounds(&self, pos: GlobalPos) -> bool
    {
        self.overmap.to_local(pos).is_some()
    }

    pub fn build_spatial(&self, entities: &ClientEntities, follow_target: Entity) -> SpatialGrid
    {
        SpatialGrid::new(entities, &self.overmap, follow_target)
    }

    pub fn exists_missing(&self) -> (u32, u32)
    {
        self.overmap.exists_missing()
    }

    fn chunk_of(pos: Pos3<f32>) -> GlobalPos
    {
        TilePos::from(pos).chunk
    }

    pub fn inside_chunk(&self, pos: Pos3<f32>) -> bool
    {
        self.overmap.contains(Self::chunk_of(pos))
    }

    pub fn overmap(&self) -> &ClientOvermap
    {
        &self.overmap
    }

    pub fn debug_visual_overmap(&self)
    {
        self.overmap.debug_visual_overmap();
    }

    pub fn debug_chunk(&self, pos: Pos3<f32>, visual: bool) -> String
    {
        self.overmap.debug_chunk(Self::chunk_of(pos), visual)
    }

    pub fn debug_tile_field(&self, entities: &ClientEntities)
    {
        self.overmap.debug_tile_field(entities)
    }

    pub fn tiles_inside<'a, Colliding, Predicate>(
        &self,
        collider: &'a CollidingInfo<Colliding>,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Colliding, Predicate>
    where
        Colliding: Borrow<Collider> + 'a,
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        self.tiles_inside_inner::<false, _, _, _>(
            collider,
            predicate,
            |info| collider.collide_immutable(&info, |_| {})
        )
    }

    pub fn tiles_contacts<'a, Colliding, ContactAdder, Predicate>(
        &self,
        collider: &'a CollidingInfo<Colliding>,
        mut add_contact: ContactAdder,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Colliding, Predicate, ContactAdder>
    where
        Colliding: Borrow<Collider> + 'a,
        ContactAdder: FnMut(Contact),
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        self.tiles_inside_inner::<true, _, _, _>(
            collider,
            predicate,
            move |info| collider.collide_immutable(&info, &mut add_contact)
        )
    }

    fn tiles_inside_inner<'a, const CHECK_NEIGHBORS: bool, Colliding, CheckCollision, Predicate>(
        &self,
        collider: &'a CollidingInfo<Colliding>,
        predicate: Predicate,
        mut check_collision: CheckCollision
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Colliding, CHECK_NEIGHBORS, Predicate, CheckCollision>
    where
        Colliding: Borrow<Collider> + 'a,
        CheckCollision: FnMut(CollidingInfoRef) -> bool,
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        let half_scale = collider.half_bounds();

        let top_left = TilePos::from(collider.transform.position - half_scale);
        let bottom_right = TilePos::from(collider.transform.position + half_scale);

        top_left.tiles_between(bottom_right).filter(move |pos|
        {
            predicate(self.tile(*pos))
        }).filter(move |pos|
        {
            let check_tile = |pos|
            {
                predicate(self.tile(pos))
            };

            let world = if CHECK_NEIGHBORS
            {
                DirectionsGroup{
                    left: check_tile(pos.offset(Pos3::new(-1, 0, 0))),
                    right: check_tile(pos.offset(Pos3::new(1, 0, 0))),
                    down: check_tile(pos.offset(Pos3::new(0, -1, 0))),
                    up: check_tile(pos.offset(Pos3::new(0, 1, 0)))
                }
            } else
            {
                DirectionsGroup::repeat(false)
            };

            let world_collider = ColliderInfo{
                kind: ColliderType::Tile(world),
                layer: ColliderLayer::World,
                ghost: false,
                override_transform: None,
                sleeping: false
            }.into();

            let info = CollidingInfoRef{
                entity: None,
                transform: Transform{
                    position: pos.entity_position(),
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                },
                collider: &world_collider
            };

            check_collision(info)
        })
    }

    pub fn modify_tile<T>(&mut self, pos: TilePos, f: impl FnOnce(&mut Self, &mut Tile) -> T) -> Option<T>
    {
        let tile: Tile = *some_or_return!(self.tile(pos));
        let mut new_tile: Tile = tile;

        let value = f(self, &mut new_tile);

        if tile != new_tile
        {
            self.set_tile(pos, new_tile);
        }

        Some(value)
    }

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.overmap.tile(index)
    }

    pub fn set_tile(&mut self, pos: TilePos, tile: Tile)
    {
        if self.set_tile_local(pos, tile)
        {
            self.world_receiver.set_tile(pos, tile);
        }
    }

    fn set_tile_local(&mut self, pos: TilePos, new_tile: Tile) -> bool
    {
        if self.tile(pos).copied() == Some(new_tile)
        {
            return false;
        }

        self.overmap.set_tile(pos, new_tile);

        true
    }

    pub fn time(&self) -> f64
    {
        self.time
    }

    pub fn set_time(&mut self, time: f64)
    {
        self.time = time % (DAY_LENGTH + NIGHT_LENGTH + BETWEEN_LENGTH * 2.0);
    }

    pub fn set_time_speed(&mut self, speed: f64)
    {
        self.time_speed = speed;
    }

    pub fn sky_light(&self) -> SkyLight
    {
        if self.time < DAY_LENGTH
        {
            SkyLight::Day
        } else if self.time < (DAY_LENGTH + BETWEEN_LENGTH)
        {
            SkyLight::Sunset((self.time - DAY_LENGTH) / BETWEEN_LENGTH)
        } else if self.time < (DAY_LENGTH + BETWEEN_LENGTH + NIGHT_LENGTH)
        {
            SkyLight::Night
        } else
        {
            SkyLight::Sunrise((self.time - (DAY_LENGTH + BETWEEN_LENGTH + NIGHT_LENGTH)) / BETWEEN_LENGTH)
        }
    }

    pub fn update(&mut self, dt: f32)
    {
        {
            let time = self.time() + dt as f64 * self.time_speed;
            self.set_time(time);
        }

        self.overmap.update(dt);
    }

    pub fn rescale(&mut self, size: Vector2<f32>)
    {
        self.overmap.rescale(size);
    }

    pub fn camera_position(&self) -> Pos3<f32>
    {
        self.overmap.camera_position()
    }

    pub fn camera_moved(&mut self, pos: Pos3<f32>, on_change: impl FnOnce())
    {
        self.overmap.camera_moved(pos, on_change);
    }

    pub fn handle_message(&mut self, message: Message) -> Option<Message>
    {
        match message
        {
            Message::SetTile{pos, tile} =>
            {
                self.set_tile_local(pos, tile);
                None
            },
            Message::ChunkSync{pos, chunk, entities} =>
            {
                self.overmap.set(pos, chunk);

                if entities.is_empty()
                {
                    None
                } else
                {
                    Some(Message::EntitySetMany{entities})
                }
            },
            _ => Some(message)
        }
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.overmap.update_buffers(info);
    }

    pub fn update_buffers_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster
    )
    {
        self.overmap.update_buffers_shadows(info, visibility, caster);
    }

    pub fn visual_chunks(&self) -> &ChunksContainer<VisualOvermapChunk>
    {
        self.overmap.visual_chunks()
    }

    pub fn visual_occluded(&self) -> &ChunksContainer<[OccludedSlice; CHUNK_SIZE]>
    {
        self.overmap.visual_occluded()
    }

    pub fn occluded_checker_info(&self) -> OccludedCheckerInfo
    {
        self.overmap.occluded_checker_info()
    }

    pub fn occluded_checker(&self, transform: &Transform) -> OccludedChecker
    {
        self.overmap.occluded_checker(transform)
    }

    pub fn update_buffers_light_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        id: usize
    )
    {
        self.overmap.update_buffers_light_shadows(info, visibility, caster, id)
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo
    )
    {
        self.overmap.draw_shadows(info);
    }

    pub fn draw_light_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        id: usize,
        f: impl FnOnce(&mut DrawInfo)
    )
    {
        self.overmap.draw_light_shadows(info, visibility, id, f);
    }

    pub fn draw_sky_occluders(
        &self,
        info: &mut DrawInfo
    )
    {
        self.overmap.draw_sky_occluders(info);
    }

    pub fn draw_sky_lights(
        &self,
        info: &mut DrawInfo
    )
    {
        self.overmap.draw_sky_lights(info);
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        is_shaded: bool
    )
    {
        self.overmap.draw_tiles(info, is_shaded);
    }
}
