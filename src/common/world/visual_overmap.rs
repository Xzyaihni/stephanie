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
pub struct VisualOvermap<const SIZE: usize>
{
	tiles_factory: TilesFactory,
	chunks: FlatChunksContainer<SIZE, (Instant, VisualChunk)>,
	size: (f32, f32),
	player_position: Arc<RwLock<Pos3<f32>>>,
	receiver: Receiver<VisualGenerated>,
	sender: Sender<VisualGenerated>
}

impl<const SIZE: usize> VisualOvermap<SIZE>
{
	pub fn new(
		tiles_factory: TilesFactory,
		size: (f32, f32),
		player_position: Pos3<f32>
	) -> Self
	{
		let chunks = FlatChunksContainer::new(|_| (Instant::now(), VisualChunk::new()));

		let player_position = Arc::new(RwLock::new(player_position));

		let (sender, receiver) = mpsc::channel();

		Self{tiles_factory, chunks, size, player_position, receiver, sender}
	}

	pub fn generate(
		&self,
		chunks: &ChunksContainer<SIZE, Option<Arc<Chunk>>>,
		pos: LocalPos<SIZE>
	)
	{
		let LocalPos(Pos3{x, y, ..}) = pos;

		let chunks = (0..=(SIZE / 2)).rev().map(|z|
		{
			let local_pos = LocalPos::new(x, y, z);

			local_pos.maybe_group()
				.map(|position| chunks[position].clone().unwrap())
		}).collect::<Vec<_>>();

		let chunk_pos = self.to_global(pos);

		let player_height = self.player_position.read().tile_height();
		println!("player_height: {player_height}");

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

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.size = size;
	}

	pub fn visible(&self, pos: LocalPos<SIZE>) -> bool
	{
		Self::visible_associated(*self.player_position.read(), self.size, pos)
	}

	fn visible_associated(
		player_position: Pos3<f32>,
		size: (f32, f32),
		pos: LocalPos<SIZE>
	) -> bool
	{
		let player_offset = player_position.modulo(CHUNK_VISUAL_SIZE);

		let offset_position = Pos3::from(pos) - (SIZE / 2) as f32;
		let chunk_offset = offset_position * CHUNK_VISUAL_SIZE - player_offset;

		let in_range = |value: f32, limit: f32| -> bool
		{
			let limit = limit / 2.0;

			((-limit - CHUNK_VISUAL_SIZE)..limit).contains(&value)
		};

		in_range(chunk_offset.x, size.0) && in_range(chunk_offset.y, size.1)
	}

	pub fn camera_moved(&mut self, position: Pos3<f32>)
	{
		*self.player_position.write() = position;
	}

	pub fn mark_ungenerated(&mut self, pos: LocalPos<SIZE>)
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

	pub fn is_generated(&self, pos: LocalPos<SIZE>) -> bool
	{
		self.chunks[pos].1.is_generated()
	}

	pub fn remove(&mut self, pos: LocalPos<SIZE>)
	{
		if pos.0.z == 0
		{
			self.chunks[pos] = (Instant::now(), VisualChunk::new());
		}
	}

	pub fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>)
	{
		if a.0.z == 0 && b.0.z == 0
		{
			self.chunks.swap(a, b);
		}
	}
}

impl<const SIZE: usize> OvermapIndexing<SIZE> for VisualOvermap<SIZE>
{
	fn player_position(&self) -> GlobalPos
	{
		self.player_position.read().rounded()
	}
}

impl<const SIZE: usize> GameObject for VisualOvermap<SIZE>
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
			Self::visible_associated(*self.player_position.read(), self.size, *pos)
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