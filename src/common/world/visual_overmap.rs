use std::{
	thread,
	time::Instant,
	sync::{
		Arc,
		mpsc::{self, Receiver, Sender}
	}
};

use parking_lot::RwLock;

use crate::{
	client::{
		ChunkInfo,
		TilesFactory,
		GameObject,
		game_object_types::*
	}
};

use super::{
	chunk::{
		CHUNK_VISUAL_SIZE,
		Pos3,
		Chunk,
		GlobalPos,
		LocalPos
	},
	overmap::{
		OvermapIndexing,
		ChunksContainer,
		FlatChunksContainer,
		visual_chunk::VisualChunk
	}
};


struct VisualGenerated
{
	chunk_info: Box<[ChunkInfo]>,
	position: GlobalPos,
	timestamp: Instant
}

#[derive(Debug)]
struct VisibilityChecker
{
	pub size: Pos3<usize>,
	pub camera_size: (f32, f32),
	pub player_position: Arc<RwLock<Pos3<f32>>>
}

impl VisibilityChecker
{
	pub fn new(size: Pos3<usize>, camera_size: (f32, f32), player_position: Pos3<f32>) -> Self
	{
		let player_position = Arc::new(RwLock::new(player_position));

		Self{size, camera_size, player_position}
	}

	pub fn visible(&self, pos: LocalPos) -> bool
	{
		let player_offset = self.player_position.read().modulo(CHUNK_VISUAL_SIZE);

		let offset_position =
			Pos3::from(pos) - Pos3::from(GlobalPos::from(Pos3::<i32>::from(self.size)) / 2);

		let chunk_offset = offset_position * CHUNK_VISUAL_SIZE - player_offset;

		let in_range = |value: f32, limit: f32| -> bool
		{
			let limit = limit / 2.0;

			((-limit - CHUNK_VISUAL_SIZE)..limit).contains(&value)
		};

		in_range(chunk_offset.x, self.camera_size.0)
		&& in_range(chunk_offset.y, self.camera_size.1)
	}
}

#[derive(Debug)]
pub struct VisualOvermap
{
	tiles_factory: TilesFactory,
	chunks: FlatChunksContainer<(Instant, VisualChunk)>,
	visibility_checker: VisibilityChecker,
	receiver: Receiver<VisualGenerated>,
	sender: Sender<VisualGenerated>
}

impl VisualOvermap
{
	pub fn new(
		tiles_factory: TilesFactory,
		size: Pos3<usize>,
		camera_size: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		let visibility_checker = VisibilityChecker::new(size, camera_size, player_position);

		let chunks = FlatChunksContainer::new(size, |_| (Instant::now(), VisualChunk::new()));

		let (sender, receiver) = mpsc::channel();

		Self{tiles_factory, chunks, visibility_checker, receiver, sender}
	}

	pub fn generate(
		&self,
		chunks: &ChunksContainer<Option<Arc<Chunk>>>,
		pos: LocalPos
	)
	{
		let Pos3{x, y, ..} = pos.pos;

		let chunks = (0..=(self.visibility_checker.size.z / 2)).rev().map(|z|
		{
			let local_pos = LocalPos::new(Pos3::new(x, y, z), self.visibility_checker.size);

			local_pos.maybe_group()
				.map(|position| chunks[position].clone().unwrap())
		}).collect::<Vec<_>>();

		let chunk_pos = self.to_global(pos);

		let player_height = self.visibility_checker.player_position.read().tile_height();

		let sender = self.sender.clone();

		let (info_map, model_builder) =
			(self.tiles_factory.info_map(), self.tiles_factory.builder());

		thread::spawn(move ||
		{
			let chunk_info = VisualChunk::create(
				info_map,
				model_builder,
				player_height,
				chunk_pos,
				&chunks
			);

			let generated = VisualGenerated{
				chunk_info,
				position: chunk_pos,
				timestamp: Instant::now()
			};

			sender.send(generated).unwrap();
		});
	}

	pub fn process_message(&mut self)
	{
		match self.receiver.try_recv()
		{
			Ok(generated) =>
			{
				self.handle_generated(generated);
			},
			Err(_) =>
			{
				return;
			}
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

	pub fn rescale(&mut self, camera_size: (f32, f32))
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
		if pos.pos.z == 0
		{
			self.chunks[pos] = (Instant::now(), VisualChunk::new());
		}
	}

	pub fn swap(&mut self, a: LocalPos, b: LocalPos)
	{
		if a.pos.z == 0 && b.pos.z == 0
		{
			self.chunks.swap(a, b);
		}
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
	fn update(&mut self, dt: f32)
	{
		self.process_message();

		self.chunks.iter_mut().for_each(|(_, chunk)| chunk.1.update(dt));
	}

	fn update_buffers(&mut self, builder: BuilderType, index: usize)
	{
		self.chunks.iter_mut().filter(|(pos, _)|
		{
			self.visibility_checker.visible(*pos)
		}).for_each(|(_, chunk)| chunk.1.update_buffers(builder, index));
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
	{
		self.chunks.iter().filter(|(pos, _)|
		{
			self.visible(*pos)
		}).for_each(|(_, chunk)| chunk.1.draw(builder, layout.clone(), index));
	}
}