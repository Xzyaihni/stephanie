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
	chunk_ordering: Box<[LocalPos<SIZE>]>,
	player_position: Pos3<f32>
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

		let chunk_ordering = Self::default_ordering(chunks.iter().map(|(pos, _)| pos));

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

		let tile_height_same = position.tile_height() == self.player_position.tile_height();

		let rounded_position = position.rounded();
		let old_rounded_position = self.player_position.rounded();

		if !tile_height_same
		{
			self.force_regenerate();
		}

		self.player_position = position;

		if rounded_position != old_rounded_position
		{
			let chunk_height_same = rounded_position.0.z == old_rounded_position.0.z;
			if !chunk_height_same
			{
				self.force_regenerate();
			}

			self.position_offset(rounded_position - old_rounded_position);
		}
	}

	fn force_regenerate(&mut self)
	{
		self.visual_overmap.mark_all_ungenerated();
		self.chunk_ordering.iter().for_each(|pos|
		{
			if pos.0.z == 0
			{
				self.check_vertical(*pos);
			}
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
			.filter(|pos| self.get_local(**pos).is_none())
			.for_each(|pos|
			{
				let global_pos = Self::to_global_associated(*pos, player_pos);

				self.request_chunk(global_pos);
			});
	}
}

impl<const SIZE: usize> OvermapIndexing<SIZE> for ClientOvermap<SIZE>
{
	fn player_position(&self) -> GlobalPos
	{
		self.player_position.rounded()
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