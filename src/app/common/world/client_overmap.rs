use std::sync::Arc;

use yanyaengine::game_object::*;

use crate::client::world_receiver::WorldReceiver;

use super::{
    Tile,
	visual_overmap::VisualOvermap,
	overmap::{
		ChunksContainer,
		Overmap,
		OvermapIndexing,
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

impl OvermapIndexing for Indexer
{
	fn size(&self) -> Pos3<usize>
	{
		self.size
	}

	fn player_position(&self) -> GlobalPos
	{
		self.player_position.rounded()
	}
}

#[derive(Debug, Clone, Copy)]
pub struct TilePos
{
    chunk: GlobalPos,
    local: ChunkLocal
}

impl TilePos
{
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

#[derive(Debug)]
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

		let chunk_ordering = indexer.default_ordering(chunks.iter().map(|(pos, _)| pos));

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

	pub fn rescale(&mut self, camera_size: (f32, f32))
	{
		self.visual_overmap.rescale(camera_size);
	}

	pub fn set(&mut self, pos: GlobalPos, chunk: Chunk)
	{
		if let Some(local_pos) = self.to_local(pos)
		{
			self.chunks[local_pos] = Some(Arc::new(chunk));

			self.check_neighbors_vertical(local_pos);
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

    pub fn player_tile(&self) -> TilePos
    {
        let pos = self.indexer.player_position;

        TilePos{
            chunk: pos.rounded(),
            local: ChunkLocal::from(pos.to_tile())
        }
    }

	pub fn camera_moved(&mut self, position: Pos3<f32>)
	{
		self.visual_overmap.camera_moved(position);

		let is_same_tile_height =
            position.tile_height() == self.indexer.player_position.tile_height();

		let rounded_position = position.rounded();
		let old_rounded_position = self.indexer.player_position.rounded();

        let position_difference = (rounded_position - old_rounded_position).0;

		self.indexer.player_position = position;

        let z_changed = !is_same_tile_height || position_difference.z != 0;
        if z_changed
        {
		    self.visual_overmap.mark_all_ungenerated();
        }

        if position_difference != Pos3::repeat(0)
		{
			self.position_offset(position_difference);
		}

        if z_changed
        {
            self.chunk_ordering.iter().for_each(|pos|
            {
                if pos.pos.z == 0
                {
                    self.check_vertical(*pos);
                }
            });
        }
	}

	fn request_chunk(&self, pos: GlobalPos)
	{
		self.world_receiver.request_chunk(pos);
	}

	fn line_exists(&self, pos: LocalPos) -> bool
	{
		(0..self.indexer.size.z).all(|z|
		{
			let pos = LocalPos::new(Pos3::new(pos.pos.x, pos.pos.y, z), self.indexer.size);

			self.chunks[pos].is_some()
		})
	}

	fn check_neighbors_vertical(&self, pos: LocalPos)
	{
		pos.directions_inclusive().flatten().for_each(|position|
			self.check_vertical(position)
		);
	}

	fn check_vertical(&self, pos: LocalPos)
	{
		let this_visual_exists = self.visual_overmap.is_generated(pos);

		if !this_visual_exists
		{
			let ready_to_draw = pos.directions_inclusive().flatten().all(|pos|
				self.line_exists(pos)
			);

			if ready_to_draw
			{
				self.visual_overmap.generate(&self.chunks, pos);
			}
		}
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

impl OvermapIndexing for ClientOvermap
{
	fn size(&self) -> Pos3<usize>
	{
		self.indexer.size()
	}

	fn player_position(&self) -> GlobalPos
	{
		self.indexer.player_position()
	}
}

impl GameObject for ClientOvermap
{
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
		self.visual_overmap.update_buffers(info);
    }

	fn draw(&self, info: &mut DrawInfo)
    {
		self.visual_overmap.draw(info);
    }
}
