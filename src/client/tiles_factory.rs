use std::{
	iter,
	sync::Arc
};

use image::error::ImageError;

use strum::IntoEnumIterator;

use parking_lot::RwLock;

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
	tilemap::{GradientMask, TileInfoMap, TileInfo},
	world::{
		OVERMAP_HALF,
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


const MODELS_AMOUNT: usize = GradientMask::COUNT + 1;
pub struct ChunkModelBuilder<'a>
{
	models: [Model; MODELS_AMOUNT],
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
		let models = (0..MODELS_AMOUNT).map(|_| Model::new())
			.collect::<Vec<_>>().try_into().unwrap();

		Self{models, object_factory, tilemap}
	}

	pub fn create(&mut self, chunk_depth: usize, pos: ChunkLocal, tile: Tile)
	{
		self.create_inner(None, chunk_depth, pos, tile);
	}

	pub fn create_direction(
		&mut self,
		direction: PosDirection,
		chunk_depth: usize,
		pos: ChunkLocal,
		tile: Tile
	)
	{
		self.create_inner(Some(direction), chunk_depth, pos, tile);
	}

	fn create_inner(
		&mut self,
		direction: Option<PosDirection>,
		chunk_depth: usize,
		pos: ChunkLocal,
		tile: Tile
	)
	{
		const MAX_TILES: usize = 5;
		const MAX_DEPTH: f32 = OVERMAP_HALF as f32 * CHUNK_SIZE as f32;

		let depth_tiles = chunk_depth as f32 * CHUNK_SIZE as f32 + (pos.0.z + 1) as f32;
		let depth = (MAX_DEPTH - depth_tiles) / MAX_TILES as f32;

		let pos = Pos3::new(
			pos.0.x as f32 * TILE_SIZE,
			pos.0.y as f32 * TILE_SIZE,
			depth
		);

		let id = direction.map_or(0, Self::direction_texture_index);

		{
			let flip_axes = match direction
			{
				Some(PosDirection::Up) | Some(PosDirection::Down) => true,
				_ => false
			};

			let uvs = self.tile_uvs(tile, flip_axes);

			self.models[id].uvs.extend(uvs);
		}

		{
			let vertices = self.tile_vertices(pos);

			self.models[id].vertices.extend(vertices);
		}
	}

	fn tile_uvs(&self, tile: Tile, flip_xy: bool) -> impl Iterator<Item=[f32; 2]>
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

		if flip_xy
		{
			vec![
				[x, y], // 1
				[x_end, y], // 3
				[x, y_end], // 2
				[x_end, y], // 6
				[x_end, y_end], // 5
				[x, y_end] // 4
			]
		} else
		{
			vec![
				[x, y],
				[x, y_end],
				[x_end, y],
				[x, y_end],
				[x_end, y_end],
				[x_end, y]
			]
		}.into_iter()
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

		let textures_indices = (iter::once(0)).chain(
			PosDirection::iter().map(Self::direction_texture_index)
		);

		let objects = self.models.into_iter().zip(textures_indices)
			.flat_map(|(model, texture_index)|
			{
				(!model.vertices.is_empty()).then(||
					self.object_factory
						.create_id(Arc::new(RwLock::new(model)), transform.clone(), texture_index)
				)
			}).collect::<Vec<_>>();

		objects.into_boxed_slice()
	}

	fn direction_texture_index(direction: PosDirection) -> usize
	{
		let mapped_mask = match direction
		{
			PosDirection::Up | PosDirection::Right => GradientMask::Outer,
			PosDirection::Down | PosDirection::Left => GradientMask::Inner
		};

		mapped_mask as usize + 1
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
		camera: Arc<RwLock<Camera>>,
		resource_uploader: &mut ResourceUploader,
		tilemap: TileMap
	) -> Result<Self, ImageError>
	{
		let mask_texture = tilemap.load_mask()?;
		let base_textures = tilemap.load_textures()?;

		let mut make_tilemap = |textures: &[_]|
		{
			let tilemap = tilemap.generate_tilemap(resource_uploader, textures);

			Arc::new(RwLock::new(tilemap))
		};

		let mut tilemaps = vec![make_tilemap(&base_textures)];
		tilemaps.extend(GradientMask::iter().map(|mask_type|
		{
			let mut textures = base_textures.clone();

			TileMap::apply_texture_mask(mask_type, &mask_texture, textures.iter_mut());

			make_tilemap(&textures)
		}));

		let object_factory = ObjectFactory::new_with_ids(
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