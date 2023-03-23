use std::{
	sync::Arc
};

use nalgebra::Vector3;

use super::{
	game::{
		ObjectFactory,
		object::{
			Object,
			model::Model
		}
	}
};

use crate::common::{
	Transform,
	TileMap,
	tilemap::{TileInfoMap, TileInfo},
	world::{
		OVERMAP_SIZE,
		OVERMAP_HALF,
		chunk::{
			CHUNK_SIZE,
			Pos3,
			LocalPos,
			tile::Tile
		}
	}
};


pub const TILE_SIZE: f32 = 0.1;

pub struct ChunkModelBuilder<'a>
{
	model: Model,
	object_factory: &'a mut ObjectFactory,
	tilemap: &'a TileMap,
	player_chunk_z: i32
}

impl<'a> ChunkModelBuilder<'a>
{
	pub fn new(
		object_factory: &'a mut ObjectFactory,
		tilemap: &'a TileMap,
		player_chunk_z: i32
	) -> Self
	{
		Self{model: Model::new(), object_factory, tilemap, player_chunk_z}
	}

	pub fn create(&mut self, chunk_height: i32, pos: LocalPos, tile: Tile)
	{
		let uvs = self.tile_uvs(tile);

		let depth_offset = (chunk_height - self.player_chunk_z + OVERMAP_HALF) as f32;
		let depth_absolute = depth_offset + pos.0.z as f32 / CHUNK_SIZE as f32;
		let depth = depth_absolute / OVERMAP_SIZE as f32;

		let pos = Pos3::new(
			pos.0.x as f32 * TILE_SIZE,
			pos.0.y as f32 * TILE_SIZE,
			depth
		);

		let vertices = self.tile_vertices(pos);

		self.model.uvs.extend(uvs);
		self.model.vertices.extend(vertices);
	}

	fn tile_uvs(&self, tile: Tile) -> impl Iterator<Item=[f32; 2]>
	{
		let side = self.tilemap.texture_row_size();

		let id = tile.id() - 1;
		let x = id % side;
		let y = id / side;

		let to_uv = |x, y| (x as f32 / side as f32, y as f32 / side as f32);

		let (x_end, y_end) = to_uv(x + 1, y + 1);
		let (x, y) = to_uv(x, y);

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

	pub fn build(self, x: i32, y: i32) -> Option<Object>
	{
		println!("sleeping 1 secs ({x}, {y})");
		std::thread::sleep(std::time::Duration::from_secs(1));
		let mut transform = Transform::new();
		transform.position = Vector3::new(
			x as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
			y as f32 * CHUNK_SIZE as f32 * TILE_SIZE,
			0.0
		);

		(self.model.vertices.len() != 0).then(||
		{
			self.object_factory.create(Arc::new(self.model), transform, 0)
		})
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
	pub fn new(object_factory: ObjectFactory, tilemap: TileMap) -> Self
	{
		Self{object_factory, tilemap}
	}

	pub fn builder(&mut self, player_height: i32) -> ChunkModelBuilder
	{
		self.build_info(player_height).1
	}

	pub fn info_map(&self) -> TileInfoMap
	{
		self.tilemap.info_map()
	}

	pub fn build_info(&mut self, player_height: i32) -> (TileInfoMap, ChunkModelBuilder)
	{
		(
			self.tilemap.info_map(),
			ChunkModelBuilder::new(&mut self.object_factory, &self.tilemap, player_height)
		)
	}

	pub fn info(&self, tile: Tile) -> &TileInfo
	{
		self.tilemap.info(tile)
	}
}