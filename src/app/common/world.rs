use nalgebra::Vector2;

use yanyaengine::{ShaderId, game_object::*};

use crate::{
    client::{
        VisibilityChecker,
        TilesFactory,
        world_receiver::WorldReceiver
    },
    common::{
        Entity,
        OccludingCasters,
        message::Message
    }
};

pub use overmap::{
    ChunksContainer,
    Axis,
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
        MaybeGroup,
        AlwaysGroup,
        tile::Tile
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
        let size = Self::overmap_size();

        let visual_overmap = VisualOvermap::new(tiles_factory, size, camera_size, player_position);
        let overmap = ClientOvermap::new(
            world_receiver.clone(),
            visual_overmap,
            size,
            player_position
        );

        Self{world_receiver, overmap}
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

    pub fn debug_chunk(&self, pos: Pos3<f32>) -> String
    {
        self.overmap.debug_chunk(self.tile_of(pos).chunk)
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
        casters: &OccludingCasters
    )
    {
        self.overmap.update_buffers(info, visibility, casters);
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
