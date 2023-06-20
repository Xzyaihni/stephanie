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
			CHUNK_SIZE,
			PosDirection,
			visual_overmap::{
				TileInfo,
				TileReader
			}
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
		pos: GlobalPos,
		tiles: TileReader
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
				&tiles
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
		tiles: &TileReader
	)
	{
		for TileInfo{pos, chunk_height, tiles} in tiles.line(x, y)
		{
			if tiles.this.is_none()
			{
				continue;
			}

			model_builder.create(chunk_height, pos, tiles.this);

			PosDirection::iter().for_each(|direction|
			{
				tiles[direction].map(|gradient_tile|
				{
					if !info_map[gradient_tile].transparent && gradient_tile != tiles.this
					{
						model_builder.create_direction(
							direction,
							chunk_height,
							pos,
							gradient_tile
						);
					}
				});
			});

			let draw_next = info_map[tiles.this].transparent;

			if !draw_next
			{
				return;
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