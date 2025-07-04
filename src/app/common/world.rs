use std::sync::Arc;

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
        world_receiver::WorldReceiver
    },
    common::{
        some_or_return,
        collider::*,
        TileMap,
        TileInfo,
        Entity,
        OccludingCaster,
        entity::ClientEntities,
        message::Message
    }
};

pub use overmap::{
    Overmap,
    FlatChunksContainer,
    ChunksContainer,
    chunks_container::{
        debug_3d_slices,
        Axis
    },
    chunk::{
        self,
        CHUNK_SIZE,
        CHUNK_VISUAL_SIZE,
        TILE_SIZE,
        Pos3,
        Chunk,
        ChunkLocal,
        GlobalPos,
        LocalPos,
        PosDirection,
        DirectionsGroup,
        Directions3dGroup,
        MaybeGroup,
        AlwaysGroup,
        tile::{Tile, TileExisting, TileRotation}
    }
};

pub use client_overmap::TilePos;

use client_overmap::ClientOvermap;
use visual_overmap::VisualOvermap;

pub mod overmap;

mod client_overmap;
mod visual_overmap;


pub const CLIENT_OVERMAP_SIZE: usize = 8;
pub const CLIENT_OVERMAP_SIZE_Z: usize = 3;

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
    overmap: ClientOvermap
}

impl World
{
    pub fn new(
        world_receiver: WorldReceiver,
        tiles_factory: TilesFactory,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let tilemap = tiles_factory.tilemap().clone();
        let size = Self::overmap_size();

        let visual_overmap = VisualOvermap::new(tiles_factory, size, camera_size, player_position);
        let overmap = ClientOvermap::new(
            world_receiver.clone(),
            visual_overmap,
            size,
            player_position
        );

        Self{tilemap, world_receiver, overmap}
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

    pub fn debug_chunk(&self, pos: Pos3<f32>, visual: bool) -> String
    {
        self.overmap.debug_chunk(Self::chunk_of(pos), visual)
    }

    pub fn debug_tile_occlusion(&self, entities: &ClientEntities)
    {
        self.overmap.debug_tile_occlusion(entities)
    }

    pub fn tiles_inside<'a, Predicate>(
        &self,
        collider: &'a CollidingInfo<'a>,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Predicate>
    where
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        self.tiles_inside_inner::<fn(_), Predicate>(collider, None, predicate)
    }

    pub fn tiles_contacts<'a, ContactAdder, Predicate>(
        &self,
        collider: &'a CollidingInfo<'a>,
        add_contact: ContactAdder,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Predicate, ContactAdder>
    where
        ContactAdder: FnMut(Contact),
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        self.tiles_inside_inner(collider, Some(add_contact), predicate)
    }

    fn tiles_inside_inner<'a, ContactAdder, Predicate>(
        &self,
        collider: &'a CollidingInfo<'a>,
        mut add_contact: Option<ContactAdder>,
        predicate: Predicate
    ) -> impl Iterator<Item=TilePos> + use<'a, '_, Predicate, ContactAdder>
    where
        ContactAdder: FnMut(Contact),
        Predicate: Fn(Option<&Tile>) -> bool + Copy
    {
        let half_scale = collider.bounds();

        let top_left = TilePos::from(Pos3::from(collider.transform.position - half_scale));
        let bottom_right = TilePos::from(Pos3::from(collider.transform.position + half_scale));

        top_left.tiles_between(bottom_right).filter(move |pos|
        {
            predicate(self.tile(*pos))
        }).filter(move |pos|
        {
            let check_tile = |pos|
            {
                predicate(self.tile(pos))
            };

            let world = if add_contact.is_some()
            {
                Directions3dGroup{
                    left: check_tile(pos.offset(Pos3::new(-1, 0, 0))),
                    right: check_tile(pos.offset(Pos3::new(1, 0, 0))),
                    down: check_tile(pos.offset(Pos3::new(0, -1, 0))),
                    up: check_tile(pos.offset(Pos3::new(0, 1, 0))),
                    back: check_tile(pos.offset(Pos3::new(0, 0, -1))),
                    forward: check_tile(pos.offset(Pos3::new(0, 0, 1)))
                }
            } else
            {
                Directions3dGroup::repeat(false)
            };

            let mut world_collider = ColliderInfo{
                kind: ColliderType::Tile(world),
                layer: ColliderLayer::World,
                ghost: false,
                scale: None
            }.into();

            let info = CollidingInfo{
                entity: None,
                transform: Transform{
                    position: pos.entity_position(),
                    scale: Vector3::repeat(TILE_SIZE),
                    ..Default::default()
                },
                collider: &mut world_collider
            };

            if let Some(add_contact) = add_contact.as_mut()
            {
                collider.collide_immutable(&info, add_contact)
            } else
            {
                collider.collide_immutable(&info, |_| {})
            }
        })
    }

    pub fn modify_tile(&mut self, pos: TilePos, f: impl FnOnce(&mut Self, &mut Tile))
    {
        let tile: Tile = *some_or_return!(self.tile(pos));
        let mut new_tile: Tile = tile;

        f(self, &mut new_tile);

        if tile != new_tile
        {
            self.set_tile(pos, new_tile);
        }
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

    pub fn update(&mut self, dt: f32)
    {
        self.overmap.update(dt);
    }

    pub fn rescale(&mut self, size: Vector2<f32>)
    {
        self.overmap.rescale(size);
    }

    pub fn camera_moved(&mut self, pos: Pos3<f32>)
    {
        self.overmap.camera_moved(pos);
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
            Message::ChunkSync{pos, chunk} =>
            {
                self.overmap.set(pos, chunk);
                None
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

    pub fn sky_occluded(&self, transform: &Transform) -> bool
    {
        self.overmap.sky_occluded(transform)
    }

    pub fn wall_occluded(&self, transform: &Transform) -> bool
    {
        self.overmap.wall_occluded(transform)
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

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        is_shaded: bool
    )
    {
        self.overmap.draw_tiles(info, is_shaded);
    }
}
