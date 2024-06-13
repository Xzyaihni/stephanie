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

use crate::client::TilesFactory;

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

#[derive(Debug)]
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
        if pos.pos.z > (self.size.z / 2)
        {
            return false;
        }

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
        (self.player_position.read().z / TILE_SIZE) as usize
    }

    pub fn height(&self, pos: LocalPos) -> usize
    {
        let middle = self.size.z / 2;

        if pos.pos.z == middle
        {
            self.player_height()
        } else
        {
            CHUNK_SIZE - 1
        }
    }
}

pub struct TileReader
{
    group: MaybeGroup<Arc<Chunk>>
}

impl TileReader
{
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

#[derive(Debug)]
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

    pub fn generate(
        &self,
        chunks: &ChunksContainer<Option<Arc<Chunk>>>,
        pos: LocalPos
    )
    {
        let tile_reader = TileReader::new(chunks, pos);

        let chunk_pos = self.to_global(pos);

        let sender = self.sender.clone();

        let (info_map, model_builder) =
            (self.tiles_factory.tilemap().clone(), self.tiles_factory.builder());

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
                timestamp: Instant::now()
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

            if current_chunk.0 < timestamp
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

    pub fn visible(&self, pos: LocalPos) -> bool
    {
        self.visibility_checker.visible(pos)
    }

    pub fn camera_moved(&mut self, position: Pos3<f32>)
    {
        *self.visibility_checker.player_position.write() = position;
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

    pub fn is_generated(&self, pos: LocalPos) -> bool
    {
        self.chunks[pos].1.is_generated()
    }

    pub fn remove(&mut self, pos: LocalPos)
    {
        self.chunks[pos] = (Instant::now(), VisualChunk::new());
    }

    pub fn swap(&mut self, a: LocalPos, b: LocalPos)
    {
        self.chunks.swap(a, b);
    }
}

impl OvermapIndexing for VisualOvermap
{
    fn size(&self) -> Pos3<usize>
    {
        self.visibility_checker.size
    }

    fn player_position(&self) -> GlobalPos
    {
        self.visibility_checker.player_position.read().rounded()
    }
}

impl GameObject for VisualOvermap
{
    fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        self.chunks.iter_mut().filter(|(pos, _)|
        {
            self.visibility_checker.visible(*pos)
        }).for_each(|(pos, chunk)|
        {
            chunk.1.update_buffers(info, self.visibility_checker.height(pos))
        });
    }

    fn draw(&self, info: &mut DrawInfo)
    {
        self.chunks.iter().filter(|(pos, _)|
        {
            self.visible(*pos)
        }).for_each(|(pos, chunk)|
        {
            chunk.1.draw(info, self.visibility_checker.height(pos))
        });
    }
}
