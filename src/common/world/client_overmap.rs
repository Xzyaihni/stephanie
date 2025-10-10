use std::{
    fmt::{self, Display},
    cmp::Ordering,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use serde::{Serialize, Deserialize};

use yanyaengine::{game_object::*, Transform};

use crate::{
    client::{
        VisibilityChecker,
        ConnectionsHandler,
        world_receiver::ChunkWorldReceiver
    },
    common::{OccludingCaster, entity::ClientEntities}
};

use super::{
    Tile,
    visual_overmap::{VisualOvermap, OccludedChecker, OccludedCheckerInfo, VisualOvermapChunk, OccludedSlice},
    overmap::{
        ChunksContainer,
        Overmap,
        OvermapIndexing,
        CommonIndexing,
        chunk::{
            TILE_SIZE,
            CHUNK_SIZE,
            Axis,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TilePos
{
    pub chunk: GlobalPos,
    pub local: ChunkLocal
}

impl Display for TilePos
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "(chunk {}, tile {})", self.chunk.0, self.local.pos())
    }
}

impl From<Pos3<f32>> for TilePos
{
    fn from(position: Pos3<f32>) -> Self
    {
        TilePos{
            chunk: position.rounded(),
            local: ChunkLocal::from(position.to_tile())
        }
    }
}

impl From<Vector3<f32>> for TilePos
{
    fn from(position: Vector3<f32>) -> Self
    {
        Self::from(Pos3::from(position))
    }
}

impl TilePos
{
    pub fn position(&self) -> Pos3<f32>
    {
        let big_pos: Pos3<f32> = self.chunk.into();
        let small_pos: Pos3<f32> = self.local.into();

        big_pos + small_pos
    }

    pub fn center_position(&self) -> Pos3<f32>
    {
        self.position() + Pos3::repeat(TILE_SIZE * 0.5)
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

    pub fn between(&self, a: Self, b: Self) -> Pos3<bool>
    {
        self.less_or_equal_axis(b).zip(a.less_or_equal_axis(*self)).map(|(a, b)| a && b)
    }

    pub fn less_axis(&self, other: Self) -> Pos3<bool>
    {
        self.compare_componentwise(other, |ordering, _this, _other|
        {
            ordering.is_lt()
        })
    }

    pub fn less_or_equal_axis(&self, other: Self) -> Pos3<bool>
    {
        self.compare_componentwise(other, |ordering, _this, _other|
        {
            ordering.is_le()
        })
    }

    fn order_componentwise(&self, other: Self, min: bool) -> Self
    {
        let output = self.compare_componentwise(other, |ordering, this, other|
        {
            let smaller = ordering.is_lt();

            let this_order = !(smaller ^ min);

            if this_order
            {
                this
            } else
            {
                other
            }
        });

        let chunk = output.map(|(chunk, _)| chunk).into();
        let local = output.map(|(_, local)| local).into();

        Self{
            chunk,
            local
        }
    }

    fn compare_componentwise<T, F>(&self, other: Self, f: F) -> Pos3<T>
    where
        F: Fn(Ordering, (i32, usize), (i32, usize)) -> T
    {
        let ordered = |
            this_inner@(this_chunk, this_local): (i32, usize),
            other_inner@(other_chunk, other_local): (i32, usize)
        |
        {
            let this_cmp = match this_chunk.cmp(&other_chunk)
            {
                Ordering::Equal =>
                {
                    this_local.cmp(&other_local)
                },
                x => x
            };

            f(this_cmp, this_inner, other_inner)
        };

        macro_rules! compared_by
        {
            ($c:ident) =>
            {
                ordered((self.chunk.0.$c, self.local.pos().$c), (other.chunk.0.$c, other.local.pos().$c))
            }
        }

        let x = compared_by!(x);
        let y = compared_by!(y);
        let z = compared_by!(z);

        Pos3{x, y, z}
    }

    pub fn to_global(&self) -> Pos3<i32>
    {
        self.chunk.0 * CHUNK_SIZE as i32 + self.local.pos().map(|x| x as i32)
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
        self.center_position().into()
    }

    pub fn is_same_height(&self, other: &Self) -> bool
    {
        self.chunk.0.z == other.chunk.0.z
            && self.local.pos().z == other.local.pos().z
    }
}

pub struct ClientOvermap
{
    world_receiver: ChunkWorldReceiver,
    visual_overmap: VisualOvermap,
    chunks: ChunksContainer<Option<Arc<Chunk>>>,
    chunk_ordering: Box<[LocalPos]>,
    indexer: Indexer
}

impl ClientOvermap
{
    pub fn new(
        world_receiver: ChunkWorldReceiver,
        visual_overmap: VisualOvermap,
        size: Pos3<usize>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let indexer = Indexer::new(size, player_position);

        let chunks = ChunksContainer::new(size);

        let chunk_ordering = indexer.default_ordering();

        Self{
            world_receiver,
            visual_overmap,
            chunks,
            chunk_ordering,
            indexer
        }
    }

    pub fn exists_missing(&self) -> (u32, u32)
    {
        self.visual_overmap.exists_missing()
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

            local_pos.directions_inclusive().flatten().for_each(|pos|
            {
                self.visual_overmap.mark_ungenerated(pos);
            });

            self.visual_overmap.try_generate_sky_occlusion(&self.chunks, local_pos);
        }
    }

    pub fn debug_visual_overmap(&self)
    {
        self.visual_overmap.debug_visual_overmap();
    }

    pub fn debug_chunk(
        &self,
        pos: GlobalPos,
        visual: bool
    ) -> String
    {
        self.to_local(pos).map(|local|
        {
            let mut s = format!("global: {pos:?}, local: {local:?}");

            s += &format!("chunk: {:#?}\n", self.get(pos));

            if visual
            {
                s += &format!("visual chunk: {:#?}", self.visual_overmap.get(local));
            }

            s
        }).unwrap_or_default()
    }

    pub fn update(&mut self, passer: &mut ConnectionsHandler, dt: f32)
    {
        crate::frame_time_this!{
            [update, update_pre, world_update] -> world_receiver,
            self.world_receiver.update(passer, &self.indexer)
        };

        crate::frame_time_this!{
            [update, update_pre, world_update] -> visual_overmap,
            self.visual_overmap.update(dt)
        };
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
                let old_tile = chunk[pos.local];
                let new_chunk = chunk.with_set_tile(pos.local, tile);

                self.chunks[local] = Some(Arc::new(new_chunk));

                if old_tile.visual_eq(&tile)
                {
                    return;
                }

                self.visual_overmap.try_regenerate(&self.chunks, local);

                local.directions().flatten().for_each(|pos|
                {
                    self.visual_overmap.try_regenerate(&self.chunks, pos)
                });
            }
        }
    }

    pub fn camera_position(&self) -> Pos3<f32>
    {
        self.visual_overmap.camera_position()
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>, on_change: impl FnOnce())
    {
        self.visual_overmap.camera_moved(position);

        let rounded_position = position.rounded();
        let old_rounded_position = self.indexer.player_position.rounded();

        let position_difference = (rounded_position - old_rounded_position).0;
        let z_changed = position_difference.z != 0;

        self.indexer.player_position = position;

        if position_difference != Pos3::repeat(0)
        {
            on_change();
            self.position_offset(position_difference);
        }

        if z_changed
        {
            if position_difference.z < 0
            {
                self.visual_overmap.moved_down(&self.chunks);
            } else
            {
                self.visual_overmap.regenerate_sky_occlusions(&self.chunks);
            }
        }
    }

    pub fn debug_tile_field(&self, entities: &ClientEntities)
    {
        self.visual_overmap.debug_tile_field(entities)
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.visual_overmap.update_buffers(info);
    }

    pub fn update_buffers_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster
    )
    {
        self.visual_overmap.update_buffers_shadows(info, visibility, caster);
    }

    pub fn visual_chunks(&self) -> &ChunksContainer<VisualOvermapChunk>
    {
        self.visual_overmap.visual_chunks()
    }

    pub fn visual_occluded(&self) -> &ChunksContainer<[OccludedSlice; CHUNK_SIZE]>
    {
        self.visual_overmap.visual_occluded()
    }

    pub fn occluded_checker_info(&self) -> OccludedCheckerInfo
    {
        self.visual_overmap.occluded_checker_info()
    }

    pub fn occluded_checker(&self, transform: &Transform) -> OccludedChecker
    {
        self.visual_overmap.occluded_checker(transform)
    }

    pub fn update_buffers_light_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        id: usize
    )
    {
        self.visual_overmap.update_buffers_light_shadows(info, visibility, caster, id)
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo
    )
    {
        self.visual_overmap.draw_shadows(info);
    }

    pub fn draw_light_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        id: usize,
        f: impl FnOnce(&mut DrawInfo)
    )
    {
        self.visual_overmap.draw_light_shadows(info, visibility, id, f);
    }

    pub fn draw_sky_occluders(
        &self,
        info: &mut DrawInfo
    )
    {
        self.visual_overmap.draw_sky_occluders(info);
    }

    pub fn draw_sky_lights(
        &self,
        info: &mut DrawInfo
    )
    {
        self.visual_overmap.draw_sky_lights(info);
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        is_shaded: bool
    )
    {
        self.visual_overmap.draw_tiles(info, is_shaded);
    }
}

impl Overmap<Option<Arc<Chunk>>> for ClientOvermap
{
    fn get_local(&self, pos: LocalPos) -> &Option<Arc<Chunk>>
    {
        &self.chunks[pos]
    }

    fn is_empty(&self, pos: LocalPos) -> bool
    {
        self.get_local(pos).is_none()
    }

    fn get(&self, pos: GlobalPos) -> Option<&Option<Arc<Chunk>>>
    {
        self.to_local(pos).map(|local_pos| self.get_local(local_pos))
    }

    fn contains(&self, pos: GlobalPos) -> bool
    {
        self.get(pos).map(|x| x.is_some()).unwrap_or(false)
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

    fn generate_missing(&mut self, shift: Option<Pos3<i32>>)
    {
        self.chunk_ordering
            .iter()
            .filter(|pos| self.chunks[**pos].is_none())
            .for_each(|pos|
            {
                let global_pos = self.indexer.to_global(*pos);

                self.world_receiver.request_chunk(global_pos);
            });

        if let Some(offset) = shift
        {
            let size = self.size();
            offset.zip(size).map(|(x, size)|
            {
                match x.cmp(&0)
                {
                    Ordering::Equal => None,
                    Ordering::Less =>
                    {
                        let value = -x;

                        (value < size as i32).then_some(value as usize)
                    },
                    Ordering::Greater =>
                    {
                        let value = size as i32 - 1 - x;

                        (value >= 0).then_some(value as usize)
                    }
                }
            }).zip(Pos3::new_axis()).map(|(plane, axis)|
            {
                if let Axis::Z = axis
                {
                    return;
                }

                if let Some(plane) = plane
                {
                    size.positions_axis(axis, plane).for_each(|pos|
                    {
                        let pos = LocalPos{pos, size};

                        self.visual_overmap.mark_ungenerated(pos);
                    });
                }
            });
        }
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
