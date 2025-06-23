use std::{
    thread,
    time::Instant,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender}
    }
};

use parking_lot::RwLock;

use nalgebra::Vector2;

use yanyaengine::game_object::*;

use crate::{
    client::{VisibilityChecker as EntityVisibilityChecker, TilesFactory},
    common::OccludingCaster
};

use super::{
    chunk::{
        TILE_SIZE,
        CHUNK_SIZE,
        CHUNK_VISUAL_SIZE,
        Pos3,
        Chunk,
        ChunkLocal,
        MaybeGroup,
        GlobalPos,
        LocalPos,
        tile::Tile
    },
    overmap::{
        OvermapIndexing,
        CommonIndexing,
        ChunksContainer,
        visual_chunk::{VisualChunk, VisualChunkInfo}
    }
};


struct VisualGenerated
{
    chunk_info: VisualChunkInfo,
    position: GlobalPos,
    timestamp: Instant
}

#[derive(Debug, Clone)]
struct VisibilityChecker
{
    pub size: Pos3<usize>,
    pub camera_size: Vector2<f32>,
    pub player_position: Arc<RwLock<Pos3<f32>>>
}

impl VisibilityChecker
{
    pub fn new(
        size: Pos3<usize>,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let player_position = Arc::new(RwLock::new(player_position));

        Self{size, camera_size, player_position}
    }

    pub fn visible(&self, pos: LocalPos) -> bool
    {
        let player_offset = self.player_offset();

        let offset_position = Pos3::from(pos) - Pos3::from(self.size / 2);

        let chunk_offset = offset_position * CHUNK_VISUAL_SIZE - player_offset;

        let in_range = |value: f32, limit: f32| -> bool
        {
            let limit = limit / 2.0;

            ((-limit - CHUNK_VISUAL_SIZE)..=limit).contains(&value)
        };

        in_range(chunk_offset.x, self.camera_size.x)
            && in_range(chunk_offset.y, self.camera_size.y)
    }

    fn player_offset(&self) -> Pos3<f32>
    {
        self.player_position.read().modulo(CHUNK_VISUAL_SIZE)
    }

    fn player_height(&self) -> usize
    {
        let z = ((self.player_position.read().z % CHUNK_VISUAL_SIZE) / TILE_SIZE).floor() as i32;

        if z < 0
        {
            (CHUNK_SIZE as i32 + z) as usize
        } else
        {
            z as usize
        }
    }

    pub fn height(&self, pos: LocalPos) -> usize
    {
        self.maybe_height(pos).unwrap_or(CHUNK_SIZE - 1)
    }

    pub fn maybe_height(&self, pos: LocalPos) -> Option<usize>
    {
        let middle = self.size.z / 2;

        if pos.pos.z == middle
        {
            Some(self.player_height())
        } else
        {
            None
        }
    }

    fn visible_z(
        &self,
        chunks: &ChunksContainer<(Instant, VisualChunk)>,
        pos: LocalPos
    ) -> impl DoubleEndedIterator<Item=LocalPos>
    {
        let top = (self.size.z / 2) + 1;
        let positions = pos.with_z_range(0..top);

        let draw_amount = positions.clone().rev().take_while(|pos|
        {
            chunks[*pos].1.draw_next(self.height(*pos))
        }).count() + 1;

        positions.rev().take(draw_amount)
    }
}

pub struct TileReader
{
    group: MaybeGroup<Arc<Chunk>>
}

impl TileReader
{
    pub fn creatable(
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        local_pos: LocalPos
    ) -> bool
    {
        let mut missing = false;
        local_pos.maybe_group().map(|position|
        {
            if chunks[position].is_none()
            {
                missing = true;
            }
        });

        !missing
    }

    pub fn new(
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        local_pos: LocalPos
    ) -> Self
    {
        let group = local_pos.maybe_group().map(|position|
        {
            chunks[position].clone().unwrap()
        });

        Self{group}
    }

    pub fn tile(&self, pos: ChunkLocal) -> MaybeGroup<Tile>
    {
        pos.maybe_group().remap(|value|
        {
            self.group.this[value]
        }, |direction, value|
        {
            value.map(|pos|
            {
                Some(self.group.this[pos])
            }).unwrap_or_else(||
            {
                self.group[direction].as_ref().map(|chunk|
                {
                    chunk[pos.overflow(direction)]
                })
            })
        })
    }
}

fn for_visible_2d<'a>(
    chunks: &ChunksContainer<(Instant, VisualChunk)>,
    visibility: &'a VisibilityChecker
) -> impl Iterator<Item=LocalPos> + use<'a>
{
    chunks.positions_2d().filter(|pos| visibility.visible(*pos))
}

pub struct VisualOvermap
{
    tiles_factory: TilesFactory,
    chunks: ChunksContainer<(Instant, VisualChunk)>,
    visibility_checker: VisibilityChecker,
    receiver: Receiver<VisualGenerated>,
    sender: Sender<VisualGenerated>
}

impl VisualOvermap
{
    pub fn new(
        tiles_factory: TilesFactory,
        size: Pos3<usize>,
        camera_size: Vector2<f32>,
        player_position: Pos3<f32>
    ) -> Self
    {
        let visibility_checker = VisibilityChecker::new(size, camera_size, player_position);

        let chunks = ChunksContainer::new_with(size, |_| (Instant::now(), VisualChunk::new()));

        let (sender, receiver) = mpsc::channel();

        Self{tiles_factory, chunks, visibility_checker, receiver, sender}
    }

    pub fn try_generate(
        &mut self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        if self.is_generated(pos)
        {
            return;
        }

        self.force_generate(chunks, pos);
    }

    pub fn try_force_generate(
        &mut self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        if chunks[pos].is_none()
        {
            return;
        }

        if !TileReader::creatable(chunks, pos)
        {
            return;
        }

        self.force_generate(chunks, pos);
    }

    pub fn force_generate(
        &mut self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        self.mark_generating(pos);

        let tile_reader = TileReader::new(chunks, pos);

        let chunk_pos = self.to_global(pos);

        let sender = self.sender.clone();

        let (info_map, model_builder) =
            (self.tiles_factory.tilemap().clone(), self.tiles_factory.builder());

        let timestamp = Instant::now();

        thread::spawn(move ||
        {
            let chunk_info = VisualChunk::create(
                info_map,
                model_builder,
                chunk_pos,
                tile_reader
            );

            let generated = VisualGenerated{
                chunk_info,
                position: chunk_pos,
                timestamp
            };

            sender.send(generated).unwrap();
        });
    }

    pub fn update(&mut self, _dt: f32)
    {
        self.process_message();
    }

    pub fn process_message(&mut self)
    {
        if let Ok(generated) = self.receiver.try_recv()
        {
            self.handle_generated(generated);
        }
    }

    fn handle_generated(&mut self, generated: VisualGenerated)
    {
        let VisualGenerated{chunk_info, position, timestamp} = generated;

        if let Some(local_pos) = self.to_local(position)
        {
            let current_chunk = &mut self.chunks[local_pos];

            if current_chunk.0 <= timestamp
            {
                let chunk = VisualChunk::build(&mut self.tiles_factory, chunk_info);

                *current_chunk = (timestamp, chunk);
            }
        }
    }

    pub fn rescale(&mut self, camera_size: Vector2<f32>)
    {
        self.visibility_checker.camera_size = camera_size;
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>)
    {
        *self.visibility_checker.player_position.write() = position;
    }

    pub fn mark_generating(&mut self, pos: LocalPos)
    {
        self.chunks[pos].1.mark_generating();
    }

    pub fn mark_ungenerated(&mut self, pos: LocalPos)
    {
        self.chunks[pos].1.mark_ungenerated();
    }

    #[allow(dead_code)]
    pub fn mark_all_ungenerated(&mut self)
    {
        self.chunks.iter_mut().for_each(|(_, (_, chunk))|
        {
            chunk.mark_ungenerated();
        });
    }

    pub fn get(&self, pos: LocalPos) -> &VisualChunk
    {
        &self.chunks[pos].1
    }

    pub fn is_generated(&self, pos: LocalPos) -> bool
    {
        self.get(pos).is_generated()
    }

    pub fn remove(&mut self, pos: LocalPos)
    {
        self.chunks[pos] = (Instant::now(), VisualChunk::new());
    }

    pub fn swap(&mut self, a: LocalPos, b: LocalPos)
    {
        self.chunks.swap(a, b);
    }

    fn for_sky_occluders(
        visibility_checker: &VisibilityChecker,
        pos: LocalPos,
        f: impl FnMut(LocalPos)
    )
    {
        let size_z = visibility_checker.size.z;
        let top = size_z / 2;
        pos.with_z_range(top..size_z).for_each(f);
    }

    fn sky_draw_height(height: Option<usize>) -> usize
    {
        height.map(|x| (x + 1).min(CHUNK_SIZE - 1)).unwrap_or(0)
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            self.visibility_checker.visible_z(&self.chunks, pos).for_each(|pos|
            {
                self.chunks[pos].1.update_buffers(
                    info,
                    self.visibility_checker.height(pos)
                )
            });

            Self::for_sky_occluders(&self.visibility_checker, pos, |pos|
            {
                self.chunks[pos].1.update_sky_buffers(
                    info,
                    Self::sky_draw_height(self.visibility_checker.maybe_height(pos))
                );
            });
        });
    }

    fn for_each_visible(&self, mut f: impl FnMut(&VisualChunk, LocalPos))
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            self.visibility_checker.visible_z(&self.chunks, pos).rev().for_each(|pos|
            {
                f(&self.chunks[pos].1, pos)
            });
        });
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo
    )
    {
        self.for_each_visible(|chunk, pos|
        {
            chunk.draw_tiles(
                info,
                self.visibility_checker.height(pos)
            )
        });
    }

    pub fn update_buffers_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &EntityVisibilityChecker,
        caster: &OccludingCaster
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if let Some(pos) = self.visibility_checker.visible_z(&self.chunks, pos).next()
            {
                self.chunks[pos].1.update_buffers_shadows(
                    info,
                    visibility,
                    caster,
                    self.visibility_checker.height(pos)
                )
            }
        });
    }

    pub fn update_buffers_light_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &EntityVisibilityChecker,
        caster: &OccludingCaster,
        id: usize
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if let Some(pos) = self.visibility_checker.visible_z(&self.chunks, pos).next()
            {
                self.chunks[pos].1.update_buffers_light_shadows(
                    info,
                    &mut self.tiles_factory,
                    visibility,
                    caster,
                    self.visibility_checker.height(pos),
                    id
                )
            }
        });
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &EntityVisibilityChecker
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if let Some(pos) = self.visibility_checker.visible_z(&self.chunks, pos).next()
            {
                self.chunks[pos].1.draw_shadows(
                    info,
                    visibility,
                    self.visibility_checker.height(pos)
                )
            }
        });
    }

    pub fn draw_light_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &EntityVisibilityChecker,
        id: usize
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            if let Some(pos) = self.visibility_checker.visible_z(&self.chunks, pos).next()
            {
                self.chunks[pos].1.draw_light_shadows(
                    info,
                    visibility,
                    self.visibility_checker.height(pos),
                    id
                )
            }
        });
    }

    pub fn draw_sky_occluders(
        &self,
        info: &mut DrawInfo
    )
    {
        for_visible_2d(&self.chunks, &self.visibility_checker).for_each(|pos|
        {
            Self::for_sky_occluders(&self.visibility_checker, pos, |pos|
            {
                self.chunks[pos].1.draw_sky_shadows(
                    info,
                    Self::sky_draw_height(self.visibility_checker.maybe_height(pos))
                );
            });
        });
    }
}

impl CommonIndexing for VisualOvermap
{
    fn size(&self) -> Pos3<usize>
    {
        self.visibility_checker.size
    }
}

impl OvermapIndexing for VisualOvermap
{
    fn player_position(&self) -> GlobalPos
    {
        self.visibility_checker.player_position.read().rounded()
    }
}
