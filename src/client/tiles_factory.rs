use std::{
	sync::Arc
};

use image::error::ImageError;

use parking_lot::RwLock;

use vulkano::memory::allocator::StandardMemoryAllocator;

use super::{
	Camera,
	game::{
		ObjectFactory,
		object::{
			Object,
			resource_uploader::ResourceUploader,
			model::Model
		}
	}
};

use crate::common::{
	TileMap,
	tilemap::{TileInfoMap, TileInfo},
	world::{
		OVERMAP_SIZE,
		chunk::{
			CHUNK_SIZE,
			TILE_SIZE,
			Chunk,
			PosDirection,
			Pos3,
			ChunkLocal,
			tile::Tile
		}
	}
};


pub struct ChunkModelBuilder<'a>
{
	models: [Model; PosDirection::COUNT + 1],
	object_factory: &'a mut ObjectFactory,
	tilemap: &'a TileMap
}

impl<'a> ChunkModelBuilder<'a>
{
	pub fn new(
		object_factory: &'a mut ObjectFactory,
		tilemap: &'a TileMap
	) -> Self
	{
		let models = (0..(PosDirection::COUNT + 1)).map(|_| Model::new())
			.collect::<Vec<_>>().try_into().unwrap();

		Self{models, object_factory, tilemap}
	}

	pub fn create(&mut self, chunk_depth: usize, pos: ChunkLocal, tile: Tile)
	{
		self.create_inner(PosDirection::COUNT, chunk_depth, pos, tile);
	}

	pub fn create_direction(
		&mut self,
		direction: PosDirection,
		chunk_depth: usize,
		pos: ChunkLocal,
		tile: Tile
	)
	{
		self.create_inner(direction as usize, chunk_depth, pos, tile);
	}

	fn create_inner(&mut self, id: usize, chunk_depth: usize, pos: ChunkLocal, tile: Tile)
	{
		let uvs = self.tile_uvs(tile);

		let depth_absolute = chunk_depth as f32 + pos.0.z as f32 / CHUNK_SIZE as f32;
		let depth = depth_absolute / OVERMAP_SIZE as f32;

		let pos = Pos3::new(
			pos.0.x as f32 * TILE_SIZE,
			pos.0.y as f32 * TILE_SIZE,
			1.0 - depth
		);

		let vertices = self.tile_vertices(pos);

		self.models[id].uvs.extend(uvs);
		self.models[id].vertices.extend(vertices);
	}

	fn tile_uvs(&self, tile: Tile) -> impl Iterator<Item=[f32; 2]>
	{
		let side = self.tilemap.texture_row_size();

		let id = tile.id() - 1;
		let x = id % side;
		let y = id / side;

		let to_uv = |x, y|
		{
			(
				x as f32 / side as f32,
				y as f32 / side as f32
			)
		};

		let half_pixel = self.tilemap.half_pixel();

		let (x_end, y_end) = to_uv(x + 1, y + 1);
		let (x_end, y_end) = (x_end - half_pixel, y_end - half_pixel);

		let (x, y) = to_uv(x, y);
		let (x, y) = (x + half_pixel, y + half_pixel);

		vec![
			[x, y],
			[x, y_end],
			[x_end, y],
			[x, y_end],
			[x_end, y_end],
			[x_end, y]
		].into_iter()
	}

	fn tile_vertices(&self, pos: Pos3<f32>) -> impl Iterator<Item=[f32; 3]>
	{
		let (x, y, z) = (pos.x, pos.y, pos.z);
		let (x_end, y_end) = (pos.x + TILE_SIZE, pos.y + TILE_SIZE);

		vec![
			[x, y, z],
			[x, y_end, z],
			[x_end, y, z],
			[x, y_end, z],
			[x_end, y_end, z],
			[x_end, y, z]
		].into_iter()
	}

	pub fn build(self, x: i32, y: i32) -> Box<[Object]>
	{
		let transform = Chunk::transform_of_chunk(x, y);

		self.models.into_iter().enumerate().flat_map(|(index, model)|
		{
			(model.vertices.len() != 0).then(||
				self.object_factory.create_id(Arc::new(model), transform.clone(), index)
			)
		}).collect::<Vec<_>>().into_boxed_slice()
	}
}

#[derive(Debug)]
pub struct TilesFactory
{
	object_factory: ObjectFactory,
	tilemap: TileMap
}


#[allow(dead_code)]
impl TilesFactory
{
	pub fn new(
		allocator: StandardMemoryAllocator,
		camera: Arc<RwLock<Camera>>,
		resource_uploader: &mut ResourceUploader,
		tilemap: TileMap
	) -> Result<Self, ImageError>
	{
		let mask_texture = tilemap.load_mask()?;
		let base_textures = tilemap.load_textures()?;

		let tilemaps = (0..PosDirection::COUNT + 1).map(|index|
		{
			let tilemap = if index != PosDirection::COUNT
			{
				let mut textures = base_textures.clone();

				let direction = PosDirection::try_from(index as u8).unwrap();

				TileMap::apply_texture_mask(direction, &mask_texture, textures.iter_mut());

				tilemap.generate_tilemap(resource_uploader, &textures)
			} else
			{
				tilemap.generate_tilemap(resource_uploader, &base_textures)
			};

			Arc::new(RwLock::new(tilemap))
		}).collect();

		let object_factory = ObjectFactory::new_with_ids(
			allocator,
			camera,
			tilemaps
		);

		Ok(Self{object_factory, tilemap})
	}

	pub fn builder(&mut self) -> ChunkModelBuilder
	{
		self.build_info().1
	}

	pub fn info_map(&self) -> TileInfoMap
	{
		self.tilemap.info_map()
	}

	pub fn build_info(&mut self) -> (TileInfoMap, ChunkModelBuilder)
	{
		(
			self.tilemap.info_map(),
			ChunkModelBuilder::new(&mut self.object_factory, &self.tilemap)
		)
	}

	pub fn info(&self, tile: Tile) -> &TileInfo
	{
		self.tilemap.info(tile)
	}
}