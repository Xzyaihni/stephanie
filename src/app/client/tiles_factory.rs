use std::{
	iter,
	sync::Arc
};

use image::error::ImageError;

use strum::IntoEnumIterator;

use parking_lot::RwLock;

use yanyaengine::{
    Object,
    ObjectInfo,
    Transform,
    ObjectFactory,
    object::{
        Texture,
        Model
    },
    game_object::*
};

use crate::common::{
	TileMap,
    TileMapWithTextures,
	tilemap::{PADDING, GradientMask, TileInfo},
	world::{
        CHUNK_VISUAL_SIZE,
		TILE_SIZE,
		Chunk,
		PosDirection,
		GlobalPos,
		Pos3,
		Tile,
		chunk::ChunkLocal,
	}
};


pub struct ChunkInfo
{
	model: Arc<RwLock<Model>>,
	transform: Transform,
	texture_index: usize
}

const MODELS_AMOUNT: usize = GradientMask::COUNT + 1;
pub struct ChunkModelBuilder
{
	models: [Model; MODELS_AMOUNT],
	tilemap: Arc<TileMap>
}

impl ChunkModelBuilder
{
	pub fn new(
		tilemap: Arc<TileMap>
	) -> Self
	{
		let models = (0..MODELS_AMOUNT).map(|_| Model::new())
			.collect::<Vec<_>>().try_into().unwrap();

		Self{models, tilemap}
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
		chunk_pos: ChunkLocal,
		tile: Tile
	)
	{
		let pos = {
            let mut pos = Pos3::<f32>::from(*chunk_pos.pos()) * TILE_SIZE;

            pos.z -= chunk_depth as f32 * CHUNK_VISUAL_SIZE;

            pos
        };

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

        let to_uv = |value|
        {
            value as f32 / side as f32
        };

		let pixel_fraction = self.tilemap.pixel_fraction(PADDING as f32);

		let x_end = to_uv(x + 1) - pixel_fraction;
        let y_end = to_uv(y + 1) - pixel_fraction;

		let x = to_uv(x) + pixel_fraction;
        let y = to_uv(y) + pixel_fraction;

		if flip_xy
		{
			[
				[x, y], // 1
				[x_end, y], // 3
				[x, y_end], // 2
				[x_end, y], // 6
				[x_end, y_end], // 5
				[x, y_end] // 4
			]
		} else
		{
			[
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

	pub fn build(self, pos: GlobalPos) -> Box<[ChunkInfo]>
	{
		let transform = Chunk::transform_of_chunk(pos);

		let textures_indices = (iter::once(0)).chain(
			PosDirection::iter_non_z().map(Self::direction_texture_index)
		);

		self.models.into_iter().zip(textures_indices)
			.flat_map(|(model, texture_index)|
			{
				(!model.vertices.is_empty()).then(||
				{
					ChunkInfo{
						model: Arc::new(RwLock::new(model)),
						transform: transform.clone(),
						texture_index
					}
				})
			}).collect()
	}

	fn direction_texture_index(direction: PosDirection) -> usize
	{
		let mapped_mask = match direction
		{
			PosDirection::Up | PosDirection::Right => GradientMask::Outer,
			PosDirection::Down | PosDirection::Left => GradientMask::Inner,
            _ => unreachable!()
		};

		mapped_mask as usize + 1
	}
}

#[derive(Debug)]
pub struct TilesFactory
{
	object_factory: Arc<ObjectFactory>,
	tilemap: Arc<TileMap>,
    textures: Vec<Arc<RwLock<Texture>>>
}

#[allow(dead_code)]
impl TilesFactory
{
	pub fn new(
        init_info: &mut InitInfo,
		tilemap: TileMapWithTextures
	) -> Result<Self, ImageError>
	{
        let TileMapWithTextures{
            tilemap,
            gradient_mask: mask_texture,
            textures: base_textures
        } = tilemap;

		let mut make_tilemap = |textures: &[_]|
		{
			let tilemap = tilemap.generate_tilemap(
                init_info.object_info.partial.builder_wrapper.resource_uploader(),
                textures
            );

			Arc::new(RwLock::new(tilemap))
		};

		let mut textures = vec![make_tilemap(&base_textures)];
		textures.extend(GradientMask::iter().map(|mask_type|
		{
			let mut textures = base_textures.clone();

			TileMap::apply_texture_mask(mask_type, &mask_texture, textures.iter_mut());

			make_tilemap(&textures)
		}));

		let tilemap = Arc::new(tilemap);

		Ok(Self{
            object_factory: init_info.object_info.partial.object_factory.clone(),
            tilemap,
            textures
        })
	}

	pub fn build(&mut self, chunk_info: Box<[ChunkInfo]>) -> Box<[Object]>
	{
		chunk_info.into_vec().into_iter().map(|chunk_info|
		{
			let ChunkInfo{model, transform, texture_index} = chunk_info;

            let object_info = ObjectInfo{
                model,
                texture: self.textures[texture_index].clone(),
                transform
            };

            self.object_factory.create(object_info)
		}).collect()
	}

	pub fn builder(&self) -> ChunkModelBuilder
	{
		ChunkModelBuilder::new(self.tilemap.clone())
	}

	pub fn tilemap(&self) -> &Arc<TileMap>
	{
		&self.tilemap
	}

	pub fn info(&self, tile: Tile) -> &TileInfo
	{
		self.tilemap.info(tile)
	}
}
