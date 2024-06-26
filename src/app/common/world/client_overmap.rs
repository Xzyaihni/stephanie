use std::sync::Arc;

use nalgebra::Vector2;

use serde::{Serialize, Deserialize};

use yanyaengine::{ShaderId, game_object::*};

use crate::{
    client::{
        VisibilityChecker,
        world_receiver::WorldReceiver
    },
    common::OccludingCasters
};

use super::{
    Tile,
    visual_overmap::VisualOvermap,
    overmap::{
        ChunksContainer,
        Overmap,
        OvermapIndexing,
        CommonIndexing,
        chunk::{
            CHUNK_SIZE,
            Pos3,
            Chunk,
            GlobalPos,
            LocalPos,
            ChunkLocal
        }
    }
};


#[derive(Debug)]
struct Indexer
{
    pub size: Pos3<usize>,
    pub player_position: Pos3<f32>
}

impl Indexer
{
    pub fn new(size: Pos3<usize>, player_position: Pos3<f32>) -> Self
    {
        Self{size, player_position}
    }
}

impl CommonIndexing for Indexer
{
    fn size(&self) -> Pos3<usize>
    {
        self.size
    }
}

impl OvermapIndexing for Indexer
{
    fn player_position(&self) -> GlobalPos
    {
        self.player_position.rounded()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TilePos
{
    pub chunk: GlobalPos,
    pub local: ChunkLocal
}

impl TilePos
{
    pub fn position(&self) -> Pos3<f32>
    {
        let big_pos: Pos3<f32> = self.chunk.into();
        let small_pos: Pos3<f32> = self.local.into();

        big_pos + small_pos
    }

    pub fn tiles_between(&self, other: Self) -> impl Iterator<Item=TilePos> + '_
    {
        let distance = self.distance(other);

        (0..=distance.z).flat_map(move |z|
        {
            (0..=distance.y).flat_map(move |y|
            {
                (0..=distance.x).map(move |x| Pos3::new(x, y, z))
            })
        }).map(|pos|
        {
            self.offset(pos)
        })
    }

    pub fn distance(&self, other: Self) -> Pos3<i32>
    {
        let chunk = other.chunk.0 - self.chunk.0;
        let local = other.local.pos().map(|x| x as i32) - self.local.pos().map(|x| x as i32);

        chunk * CHUNK_SIZE as i32 + local
    }

    pub fn offset(self, offset: Pos3<i32>) -> Self
    {
        let (chunk, local) = self.local.pos()
            .zip(self.chunk.0)
            .zip(offset)
            .map(|((local, chunk), offset)|
            {
                let new_local = local as i32 + offset;

                let chunk_size = CHUNK_SIZE as i32;

                if new_local < 0
                {
                    let out_local = (chunk_size + new_local % chunk_size) as usize;

                    (chunk - 1 + new_local / chunk_size, out_local)
                } else if new_local >= chunk_size
                {
                    (chunk + new_local / chunk_size, (new_local % chunk_size) as usize)
                } else
                {
                    (chunk, new_local as usize)
                }
            }).unzip();

        TilePos{
            chunk: GlobalPos::from(chunk),
            local: ChunkLocal::from(local)
        }
    }
}

pub struct ClientOvermap
{
    world_receiver: WorldReceiver,
    visual_overmap: VisualOvermap,
    chunks: ChunksContainer<Option<Arc<Chunk>>>,
    chunk_ordering: Box<[LocalPos]>,
    indexer: Indexer
}

impl ClientOvermap
{
    pub fn new(
        world_receiver: WorldReceiver,
        visual_overmap: VisualOvermap,
        size: Pos3<usize>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let indexer = Indexer::new(size, player_position);

        let chunks = ChunksContainer::new(size);

        let chunk_ordering = indexer.default_ordering(chunks.positions());

        let mut this = Self{
            world_receiver,
            visual_overmap,
            chunks,
            chunk_ordering,
            indexer
        };

        this.generate_missing();

        this
    }

    pub fn rescale(&mut self, camera_size: Vector2<f32>)
    {
        self.visual_overmap.rescale(camera_size);
    }

    pub fn set(&mut self, pos: GlobalPos, chunk: Chunk)
    {
        if let Some(local_pos) = self.to_local(pos)
        {
            self.chunks[local_pos] = Some(Arc::new(chunk));

            self.check_neighbors(local_pos);
        }
    }

    pub fn update(&mut self, dt: f32)
    {
        self.visual_overmap.update(dt);
    }

    pub fn tile(&self, index: TilePos) -> Option<&Tile>
    {
        self.to_local(index.chunk).and_then(|local_pos|
        {
            self.chunks[local_pos].as_ref()
        }).map(|chunk|
        {
            &chunk[index.local]
        })
    }

    pub fn set_tile(&mut self, pos: TilePos, tile: Tile)
    {
        if let Some(local) = self.to_local(pos.chunk)
        {
            if let Some(ref chunk) = self.chunks[local]
            {
                let new_chunk = chunk.with_set_tile(pos.local, tile);

                self.chunks[local] = Some(Arc::new(new_chunk));

                local.directions_inclusive().flatten().for_each(|pos|
                {
                    self.visual_overmap.generate(&self.chunks, pos)
                });
            }
        }
    }

    pub fn tile_of(&self, position: Pos3<f32>) -> TilePos
    {
        TilePos{
            chunk: position.rounded(),
            local: ChunkLocal::from(position.to_tile())
        }
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>)
    {
        self.visual_overmap.camera_moved(position);

        let rounded_position = position.rounded();
        let old_rounded_position = self.indexer.player_position.rounded();

        let position_difference = (rounded_position - old_rounded_position).0;

        self.indexer.player_position = position;

        if position_difference != Pos3::repeat(0)
        {
            self.position_offset(position_difference);
        }
    }

    fn request_chunk(&self, pos: GlobalPos)
    {
        self.world_receiver.request_chunk(pos);
    }

    fn check_neighbors(&self, pos: LocalPos)
    {
        pos.directions_inclusive().flatten().for_each(|position|
            self.check_visual(position)
        );
    }

    fn check_visual(&self, pos: LocalPos)
    {
        let this_visual_exists = self.visual_overmap.is_generated(pos);

        if !this_visual_exists
        {
            let neighbors_exist = pos.directions_inclusive().flatten().all(|pos|
            {
                self.chunks[pos].is_some()
            });

            if neighbors_exist
            {
                self.visual_overmap.generate(&self.chunks, pos);
            }
        }
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        casters: &OccludingCasters
    )
    {
        self.visual_overmap.update_buffers(info, visibility, casters);
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        shadow: ShaderId
    )
    {
        self.visual_overmap.draw_objects(info);

        info.bind_pipeline(shadow);

        self.visual_overmap.draw_shadows(info, visibility);
    }
}

impl Overmap<Arc<Chunk>> for ClientOvermap
{
    fn get_local(&self, pos: LocalPos) -> &Option<Arc<Chunk>>
    {
        &self.chunks[pos]
    }

    fn remove(&mut self, pos: LocalPos)
    {
        self.chunks[pos] = None;

        self.visual_overmap.remove(pos);
    }

    fn swap(&mut self, a: LocalPos, b: LocalPos)
    {
        self.chunks.swap(a, b);
        self.visual_overmap.swap(a, b);
    }

    fn mark_ungenerated(&mut self, pos: LocalPos)
    {
        self.visual_overmap.mark_ungenerated(pos);
    }

    fn generate_missing(&mut self)
    {
        self.chunk_ordering
            .iter()
            .filter(|pos| self.get_local(**pos).is_none())
            .for_each(|pos|
            {
                let global_pos = self.to_global(*pos);

                self.request_chunk(global_pos);
            });
    }
}

impl CommonIndexing for ClientOvermap
{
    fn size(&self) -> Pos3<usize>
    {
        self.indexer.size()
    }
}

impl OvermapIndexing for ClientOvermap
{
    fn player_position(&self) -> GlobalPos
    {
        self.indexer.player_position()
    }
}
