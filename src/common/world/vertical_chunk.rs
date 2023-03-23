use std::{
	sync::Arc,
	iter
};

use vulkano::memory::allocator::FastMemoryAllocator;

use crate::{
	client::{
		GameObject,
		BuilderType,
		tiles_factory::ChunkModelBuilder,
		game::object::Object
	},
	common::{
		tilemap::TileInfoMap,
		world::{
			LocalPos,
			GlobalPos,
			Chunk,
			chunk::CHUNK_SIZE
		}
	}
};


#[derive(Debug)]
pub struct VerticalChunk
{
	object: Option<Object>
}

impl VerticalChunk
{
	pub fn new() -> Self
	{
		Self{object: None}
	}

	pub fn regenerate<'a, I>(
		info_map: TileInfoMap,
		mut model_builder: ChunkModelBuilder,
		height: usize,
		pos: GlobalPos,
		chunks: I
	) -> Self
	where
		I: Iterator<Item=&'a Arc<Chunk>> + Clone
	{
		(0..CHUNK_SIZE).flat_map(|y|
		{
			(0..CHUNK_SIZE).map(move |x| (x, y))
		}).for_each(|(x, y)|
		{
			Self::tile_line(
				&info_map,
				&mut model_builder,
				x,
				y,
				height,
				pos.0.z,
				chunks.clone()
			)
		});

		Self{object: model_builder.build(pos.0.x, pos.0.y)}
	}

	fn tile_line<'a, I>(
		info_map: &TileInfoMap,
		model_builder: &mut ChunkModelBuilder,
		x: usize,
		y: usize,
		player_height: usize,
		chunk_height: i32,
		chunks: I
	)
	where
		I: Iterator<Item=&'a Arc<Chunk>>
	{
		let mut chunks = chunks.enumerate().map(move |(index, chunk)|
		{
			let chunk_height = chunk_height - index as i32;

			chunk.vertical_iter(x, y).enumerate().rev()
				.zip(iter::repeat(chunk_height))
		});

		if let Some(chunk) = chunks.next()
		{
			let mut previous = true;

			let skip_amount = CHUNK_SIZE - 1 - player_height;

			//skip the blocks above the player in the first chunk
			chunk.skip(skip_amount)
				.chain(chunks.flatten())
				.filter(|((_, tile), _)| !tile.is_none())
				.take_while(move |((_, tile), _)|
				{
					let transparent = info_map[**tile].transparent;
					let previous_save = previous;

					previous = transparent;

					previous_save
				})
				.for_each(move |((z, tile), chunk_height)|
				{
					let local_pos = LocalPos::new(x, y, z);
					model_builder.create(chunk_height, local_pos, *tile);
				});
		}
	}
}

impl GameObject for VerticalChunk
{
	fn update(&mut self, _dt: f32) {}

	fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.object.as_mut().map(|object| object.regenerate_buffers(allocator));
	}

	fn draw(&self, builder: BuilderType)
	{
		self.object.as_ref().map(|object| object.draw(builder));
	}
}