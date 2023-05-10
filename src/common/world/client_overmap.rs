use std::{
	sync::Arc
};

use crate::{
	client::{
		GameObject,
		game_object_types::*,
		world_receiver::WorldReceiver
	}
};

use super::{
	visual_overmap::VisualOvermap,
	overmap::{
		ChunksContainer,
		Overmap,
		OvermapIndexing,
		ChunkIndexing,
		chunk::{
			Pos3,
			Chunk,
			GlobalPos,
			LocalPos
		}
	}
};


#[derive(Debug)]
pub struct ClientOvermap<const SIZE: usize>
{
	world_receiver: WorldReceiver,
	visual_overmap: VisualOvermap<SIZE>,
	chunks: ChunksContainer<SIZE, Option<Arc<Chunk>>>,
	chunk_ordering: Box<[usize]>,
	player_position: GlobalPos
}

impl<const SIZE: usize> ClientOvermap<SIZE>
{
	pub fn new(
		world_receiver: WorldReceiver,
		visual_overmap: VisualOvermap<SIZE>,
		player_position: Pos3<f32>
	) -> Self
	{
		let chunks = ChunksContainer::new(|_| None);

		let player_position = player_position.rounded();

		let chunk_ordering = Self::default_ordering();

		let mut this = Self{
			world_receiver,
			visual_overmap,
			chunks,
			chunk_ordering,
			player_position
		};

		this.generate_missing();

		this
	}

	pub fn rescale(&mut self, size: (f32, f32))
	{
		self.visual_overmap.rescale(size);
	}

	pub fn set(&mut self, pos: GlobalPos, chunk: Chunk)
	{
		if let Some(local_pos) = self.to_local(pos)
		{
			self.chunks[local_pos] = Some(Arc::new(chunk));

			self.check_neighbors_vertical(local_pos);
		}
	}

	pub fn camera_moved(&mut self, position: Pos3<f32>)
	{
		self.visual_overmap.camera_moved(position);

		let player_position = position.rounded();

		let old_position = self.player_position;
		if player_position != old_position
		{
			if player_position.0.z != old_position.0.z
			{
				self.force_regenerate();
			}

			self.player_position = player_position;

			self.position_offset(player_position - old_position);
		}
	}

	fn force_regenerate(&mut self)
	{
		self.visual_overmap.mark_all_ungenerated();
		self.chunk_ordering.iter().for_each(|index|
		{
			let pos = ChunksContainer::<SIZE, Option<Arc<Chunk>>>::index_to_pos(*index);
			self.check_vertical(pos);
		});
	}

	fn request_chunk(&self, pos: GlobalPos)
	{
		self.world_receiver.request_chunk(pos);
	}

	fn line_exists(&self, pos: LocalPos<SIZE>) -> bool
	{
		(0..SIZE).all(|z|
		{
			let pos = LocalPos::new(pos.0.x, pos.0.y, z);

			self.chunks[pos].is_some()
		})
	}

	fn check_neighbors_vertical(&self, pos: LocalPos<SIZE>)
	{
		pos.directions_inclusive().flatten().for_each(|position|
			self.check_vertical(position)
		);
	}

	fn check_vertical(&self, pos: LocalPos<SIZE>)
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

impl<const SIZE: usize> Overmap<SIZE, Arc<Chunk>> for ClientOvermap<SIZE>
{
	type Container = ChunksContainer<SIZE, Option<Arc<Chunk>>>;

	fn get_local(&self, pos: LocalPos<SIZE>) -> &Option<Arc<Chunk>>
	{
		&self.chunks[pos]
	}

	fn remove(&mut self, pos: LocalPos<SIZE>)
	{
		self.chunks[pos] = None;

		self.visual_overmap.remove(pos);
	}

	fn swap(&mut self, a: LocalPos<SIZE>, b: LocalPos<SIZE>)
	{
		self.chunks.swap(a, b);
		self.visual_overmap.swap(a, b);
	}

	fn mark_ungenerated(&mut self, pos: LocalPos<SIZE>)
	{
		self.visual_overmap.mark_ungenerated(pos);
	}

	fn generate_missing(&mut self)
	{
		let player_pos = self.player_position();

		self.chunk_ordering
			.iter()
			.map(|index| Self::Container::index_to_pos(*index))
			.filter(|pos| self.get_local(*pos).is_none())
			.for_each(|pos|
			{
				let global_pos = Self::to_global_associated(pos, player_pos);

				self.request_chunk(global_pos);
			});
	}
}

impl<const SIZE: usize> OvermapIndexing<SIZE> for ClientOvermap<SIZE>
{
	fn player_position(&self) -> GlobalPos
	{
		self.player_position
	}
}

impl<const SIZE: usize> GameObject for ClientOvermap<SIZE>
{
	fn update(&mut self, dt: f32)
	{
		self.visual_overmap.update(dt);
	}

	fn update_buffers(&mut self, builder: BuilderType, index: usize)
	{
		self.visual_overmap.update_buffers(builder, index);
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
	{
		self.visual_overmap.draw(builder, layout.clone(), index);
	}
}