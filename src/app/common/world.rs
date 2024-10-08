use std::sync::Arc;

use nalgebra::{Vector2, Vector3};

use yanyaengine::{Transform, ShaderId, game_object::*};

use crate::{
    client::{
        VisibilityChecker,
        TilesFactory,
        world_receiver::WorldReceiver
    },
    common::{
        collider::*,
        TileMap,
        TileInfo,
        Entity,
        OccludingCaster,
        message::Message
    }
};

pub use overmap::{
    Overmap,
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
        tile::{Tile, TileRotation}
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

    pub fn tile_info(&self, tile: Tile) -> &TileInfo
    {
        self.tilemap.info(tile)
    }

    pub fn overmap_size() -> Pos3<usize>
    {
        Pos3::new(CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE, CLIENT_OVERMAP_SIZE_Z)
    }

    pub fn zoom_limits() -> (f32, f32)
    {
        //make the camera smaller by 3 tiles so theres time for the missing chunks to load
        let padding = 3;

        let padding = TILE_SIZE * padding as f32;

        let max_scale = (CLIENT_OVERMAP_SIZE - 1) as f32 * CHUNK_VISUAL_SIZE - padding;
        let min_scale = 0.2;

        (min_scale, max_scale)
    }

    fn chunk_of(&self, pos: Pos3<f32>) -> GlobalPos
    {
        self.tile_of(pos).chunk
    }

    pub fn inside_chunk(&self, pos: Pos3<f32>) -> bool
    {
        self.overmap.contains(self.chunk_of(pos))
    }

    pub fn debug_chunk(&self, pos: Pos3<f32>, visual: bool) -> String
    {
        self.overmap.debug_chunk(self.chunk_of(pos), visual)
    }

    pub fn tiles_inside<'a>(
        &'a self,
        collider: &'a CollidingInfo<'a>,
        predicate: impl Fn(Option<&'a Tile>) -> bool + 'a + Copy
    ) -> impl Iterator<Item=TilePos> + 'a
    {
        self.tiles_inside_inner::<fn(_)>(collider, None, predicate)
    }

    pub fn tiles_contacts<'a, ContactAdder: FnMut(Contact) + 'a>(
        &'a self,
        collider: &'a CollidingInfo<'a>,
        add_contact: ContactAdder,
        predicate: impl Fn(Option<&'a Tile>) -> bool + 'a + Copy
    ) -> impl Iterator<Item=TilePos> + 'a
    {
        self.tiles_inside_inner(collider, Some(add_contact), predicate)
    }

    fn tiles_inside_inner<'a, ContactAdder: FnMut(Contact) + 'a>(
        &'a self,
        collider: &'a CollidingInfo<'a>,
        mut add_contact: Option<ContactAdder>,
        predicate: impl Fn(Option<&'a Tile>) -> bool + 'a + Copy
    ) -> impl Iterator<Item=TilePos> + 'a
    {
        let half_scale = collider.bounds();

        let top_left = self.tile_of((collider.transform.position - half_scale).into());
        let bottom_right = self.tile_of((collider.transform.position + half_scale).into());

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

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.overmap.tile(index)
    }

    pub fn tile_of(&self, position: Pos3<f32>) -> TilePos
    {
        self.overmap.tile_of(position)
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
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster
    )
    {
        self.overmap.update_buffers(info, visibility, caster);
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        shadow: ShaderId
    )
    {
        self.overmap.draw(info, visibility, shadow);
    }
}
