use std::{
    cmp::Ordering,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use serde::{Serialize, Deserialize};

use yanyaengine::{ShaderId, game_object::*};

use crate::{
    client::{
        VisibilityChecker,
        world_receiver::WorldReceiver
    },
    common::OccludingCaster
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
            TILE_SIZE,
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

    pub fn tiles_between(&self, other: Self) -> impl Iterator<Item=TilePos>
    {
        let start = self.min_componentwise(other);
        let end = self.max_componentwise(other);

        let distance = start.distance(end);

        (0..=distance.z).flat_map(move |z|
        {
            (0..=distance.y).flat_map(move |y|
            {
                (0..=distance.x).map(move |x| Pos3::new(x, y, z))
            })
        }).map(move |pos|
        {
            start.offset(pos)
        })
    }

    pub fn min_componentwise(&self, other: Self) -> Self
    {
        self.order_componentwise(other, true)
    }

    pub fn max_componentwise(&self, other: Self) -> Self
    {
        self.order_componentwise(other, false)
    }

    fn order_componentwise(&self, other: Self, min: bool) -> Self
    {
        let ordered = |this_chunk: i32, this_local, other_chunk: i32, other_local|
        {
            let this_smaller = match this_chunk.cmp(&other_chunk)
            {
                Ordering::Less => true,
                Ordering::Equal =>
                {
                    this_local < other_local
                },
                Ordering::Greater => false
            };

            let this_order = !(this_smaller ^ min);

            if this_order
            {
                (this_chunk, this_local)
            } else
            {
                (other_chunk, other_local)
            }
        };

        macro_rules! ordered_by
        {
            ($c:ident) =>
            {
                ordered(self.chunk.0.$c, self.local.pos().$c, other.chunk.0.$c, other.local.pos().$c)
            }
        }

        let x = ordered_by!(x);
        let y = ordered_by!(y);
        let z = ordered_by!(z);

        Self{
            chunk: GlobalPos::new(x.0, y.0, z.0),
            local: ChunkLocal::new(x.1, y.1, z.1),
        }
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

    pub fn entity_position(&self) -> Vector3<f32>
    {
        (self.position() + Pos3::repeat(TILE_SIZE / 2.0)).into()
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

        Self{
            world_receiver,
            visual_overmap,
            chunks,
            chunk_ordering,
            indexer
        }
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

    pub fn debug_chunk(&self, pos: GlobalPos) -> String
    {
        self.to_local(pos).map(|local|
        {
            let s = format!("chunk: {:#?}\n", self.get(pos));

            s + &format!("visual chunk: {:#?}", self.visual_overmap.get(local))
        }).unwrap_or_default()
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

                self.visual_overmap.generate(&self.chunks, local);
                local.directions().flatten().for_each(|pos|
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
        caster: &OccludingCaster
    )
    {
        self.visual_overmap.update_buffers(info, visibility, caster);
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        shadow: ShaderId
    )
    {
        self.visual_overmap.draw_tiles(info);

        let original_pipeline = info.current_pipeline_id().unwrap();
        info.bind_pipeline(shadow);

        self.visual_overmap.draw_shadows(info, visibility);

        info.bind_pipeline(original_pipeline);

        info.set_depth_write(false);

        self.visual_overmap.draw_gradients(info);
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
