use std::{
	sync::Arc
};

use vulkano::memory::allocator::StandardMemoryAllocator;

use crate::{
	client::{
		GameObject,
		BuilderType,
		LayoutType,
		tiles_factory::ChunkModelBuilder,
		game::object::Object
	},
	common::{
		tilemap::TileInfoMap,
		world::{
			LocalPos,
			GlobalPos,
			Chunk,
			chunk::{CHUNK_SIZE, PosDirection, InclusiveGroup}
		}
	}
};


#[derive(Debug)]
pub struct VerticalChunk
{
	objects: Box<[Object]>
}

impl VerticalChunk
{
	pub fn new() -> Self
	{
		Self{objects: Box::new([])}
	}

	pub fn regenerate(
		info_map: TileInfoMap,
		mut model_builder: ChunkModelBuilder,
		height: usize,
		pos: GlobalPos,
		chunks: &[InclusiveGroup<Arc<Chunk>>]
	) -> Self
	{
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

		Self{objects: model_builder.build(pos.0.x, pos.0.y)}
	}

	fn create_tile_line(
		info_map: &TileInfoMap,
		model_builder: &mut ChunkModelBuilder,
		x: usize,
		y: usize,
		player_height: usize,
		chunks: &[InclusiveGroup<Arc<Chunk>>]
	)
	{
		for (chunk_depth, chunk_group) in chunks.iter().enumerate()
		{
			//the compiler better optimize this away >:(
			let skip_amount = if chunk_depth == 0
			{
				CHUNK_SIZE - 1 - player_height
			} else
			{
				0
			};

			for z in (0..CHUNK_SIZE).rev().skip(skip_amount)
			{
				let local_pos = LocalPos::new(x, y, z);
				let tile = chunk_group.this[local_pos];

				if tile.is_none()
				{
					continue;
				}

				model_builder.create(chunk_depth, local_pos, tile);

				let mut draw_gradient = |chunk: &Arc<Chunk>, pos, other_pos, direction|
				{
					let gradient_tile = chunk[other_pos];

					if !info_map[gradient_tile].transparent && gradient_tile != tile
					{
						model_builder.create_direction(direction, chunk_depth, pos, gradient_tile);
					}
				};

				PosDirection::all_iter().for_each(|direction|
				{
					if let Some(pos) = local_pos.offset(direction)
					{
						draw_gradient(&chunk_group.this, local_pos, pos, direction);
					} else
					{
						chunk_group[direction].as_ref().map(|chunk|
						{
							let other = local_pos.overflow(direction);

							draw_gradient(chunk, local_pos, other, direction)
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
}

impl GameObject for VerticalChunk
{
	fn update(&mut self, _dt: f32) {}

	fn regenerate_buffers(&mut self, allocator: &StandardMemoryAllocator)
	{
		self.objects.iter_mut().for_each(|object| object.regenerate_buffers(allocator));
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType)
	{
		self.objects.iter().rev().for_each(|object| object.draw(builder, layout.clone()));
	}
}