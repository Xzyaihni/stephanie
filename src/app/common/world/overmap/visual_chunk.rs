use std::sync::Arc;

use strum::IntoEnumIterator;

use yanyaengine::{
    Object,
    game_object::*
};

use crate::{
	client::tiles_factory::{TilesFactory, ChunkInfo, ChunkModelBuilder},
	common::{
        TileMap,
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
		tilemap: Arc<TileMap>,
		mut model_builder: ChunkModelBuilder,
		pos: GlobalPos,
		tiles: TileReader
	) -> Box<[ChunkInfo]>
	{
		(0..CHUNK_SIZE).flat_map(|y|
		{
			(0..CHUNK_SIZE).map(move |x| (x, y))
		}).for_each(|(x, y)|
		{
			Self::create_tile_line(
				&tilemap,
				&mut model_builder,
				x,
				y,
				&tiles
			)
		});

		model_builder.build(pos)
	}

	pub fn build(tiles_factory: &mut TilesFactory, chunk_info: Box<[ChunkInfo]>) -> Self
	{
		let objects = tiles_factory.build(chunk_info);

		Self{objects, generated: true}
	}

	fn create_tile_line(
		tilemap: &TileMap,
		model_builder: &mut ChunkModelBuilder,
		x: usize,
		y: usize,
		tiles: &TileReader
	)
	{
		for TileInfo{pos, chunk_depth, tiles} in tiles.line(x, y)
		{
			if tiles.this.is_none()
			{
				continue;
			}

			model_builder.create(chunk_depth, pos, tiles.this);

			PosDirection::iter().for_each(|direction|
			{
				if let Some(gradient_tile) = tiles[direction]
				{
					if !tilemap[gradient_tile].transparent && gradient_tile != tiles.this
					{
						model_builder.create_direction(
							direction,
							chunk_depth,
							pos,
							gradient_tile
						);
					}
				}
			});

			let draw_next = tilemap[tiles.this].transparent;

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
	fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
		self.objects.iter_mut().for_each(|object| object.update_buffers(info));
    }

	fn draw(&self, info: &mut DrawInfo)
    {
		self.objects.iter().for_each(|object| object.draw(info));
    }
}
