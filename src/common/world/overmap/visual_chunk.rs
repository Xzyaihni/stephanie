use std::{
	sync::Arc
};

use strum::IntoEnumIterator;

use crate::{
	client::{
		GameObject,
		game_object_types::*,
		tiles_factory::{TilesFactory, ChunkInfo, ChunkModelBuilder},
		game::object::Object
	},
	common::{
		tilemap::TileInfoMap,
		world::{
			GlobalPos,
			Chunk,
			ChunkLocal,
			CHUNK_SIZE,
			PosDirection,
			MaybeGroup
		}
	}
};


#[derive(Debug)]
pub struct VisualChunk
{
	objects: Box<[Object]>,
	generated: bool
}

impl VisualChunk
{
	pub fn new() -> Self
	{
		Self{objects: Box::new([]), generated: false}
	}

	pub fn create(
		info_map: TileInfoMap,
		mut model_builder: ChunkModelBuilder,
		height: usize,
		pos: GlobalPos,
		chunks: &[MaybeGroup<Arc<Chunk>>]
	) -> Box<[ChunkInfo]>
	{
		// ignores pos.0.z!! dont pay attention to it

		(0..CHUNK_SIZE).flat_map(|y|
		{
			(0..CHUNK_SIZE).map(move |x| (x, y))
		}).for_each(|(x, y)|
		{
			Self::create_tile_line(
				&info_map,
				&mut model_builder,
				x,
				y,
				height,
				chunks
			)
		});

		model_builder.build(GlobalPos::new(pos.0.x, pos.0.y, 0))
	}

	pub fn build(tiles_factory: &mut TilesFactory, chunk_info: Box<[ChunkInfo]>) -> Self
	{
		let objects = tiles_factory.build(chunk_info);

		Self{objects, generated: true}
	}

	fn create_tile_line(
		info_map: &TileInfoMap,
		model_builder: &mut ChunkModelBuilder,
		x: usize,
		y: usize,
		player_height: usize,
		chunks: &[MaybeGroup<Arc<Chunk>>]
	)
	{
		for (chunk_depth, chunk_group) in chunks.iter().enumerate()
		{
			let chunk_height = chunks.len() - 1 - chunk_depth;

			dbg!("rework this");
			// the compiler better optimize this away >:(
			let skip_amount = if chunk_depth == 0
			{
				// skips all tiles if the player is at the bottom of the chunk
				CHUNK_SIZE - player_height
			} else
			{
				0
			};

			for z in (0..CHUNK_SIZE).rev().skip(skip_amount)
			{
				let chunk_local = ChunkLocal::new(x, y, z);
				let tile = chunk_group.this[chunk_local];

				if tile.is_none()
				{
					continue;
				}

				model_builder.create(chunk_height, chunk_local, tile);

				let mut draw_gradient = |chunk: &Arc<Chunk>, pos, other_pos, direction|
				{
					let gradient_tile = chunk[other_pos];

					if !info_map[gradient_tile].transparent && gradient_tile != tile
					{
						model_builder.create_direction(
							direction,
							chunk_height,
							pos,
							gradient_tile
						);
					}
				};

				PosDirection::iter().for_each(|direction|
				{
					if let Some(pos) = chunk_local.offset(direction)
					{
						draw_gradient(&chunk_group.this, chunk_local, pos, direction);
					} else
					{
						chunk_group[direction].as_ref().map(|chunk|
						{
							let other = chunk_local.overflow(direction);

							draw_gradient(chunk, chunk_local, other, direction)
						});
					}
				});

				let draw_next = info_map[tile].transparent;

				if !draw_next
				{
					return;
				}
			}
		}
	}

	pub fn is_generated(&self) -> bool
	{
		self.generated
	}

	pub fn mark_ungenerated(&mut self)
	{
		self.generated = false;
	}
}

impl GameObject for VisualChunk
{
	fn update(&mut self, _dt: f32) {}

	fn update_buffers(&mut self, builder: BuilderType, index: usize)
	{
		self.objects.iter_mut().for_each(|object| object.update_buffers(builder, index));
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType, index: usize)
	{
		self.objects.iter().for_each(|object| object.draw(builder, layout.clone(), index));
	}
}