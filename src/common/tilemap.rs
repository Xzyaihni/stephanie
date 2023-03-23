use std::{
	io,
	ops::Index,
	collections::HashMap,
	fs::File,
	path::{Path, PathBuf}
};

use serde::{Serialize, Deserialize};

use image::{
	imageops::FilterType,
	error::ImageError
};

use crate::{
	common::world::chunk::tile::Tile,
	client::game::object::{
		resource_uploader::ResourceUploader,
		texture::{Color, SimpleImage, Texture}
	}
};


const TILE_SIZE: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileInfo
{
	pub name: String,
	pub texture: PathBuf,
	pub transparent: bool
}

pub struct TileInfoMap<'a>
{
	tilemap: &'a TileMap
}

impl<'a> TileInfoMap<'a>
{
	pub fn new(tilemap: &'a TileMap) -> Self
	{
		Self{tilemap}
	}
}

impl<'a> Index<Tile> for TileInfoMap<'a>
{
	type Output = TileInfo;

	fn index(&self, tile: Tile) -> &Self::Output
	{
		self.tilemap.tiles.get(tile.id()).unwrap()
	}
}

#[derive(Debug)]
pub struct TileMap
{
	tiles: Vec<TileInfo>
}

#[allow(dead_code)]
impl TileMap
{
	pub fn parse(tiles_path: &str, textures_root: &str) -> Result<Self, io::Error>
	{
		let textures_root = Path::new(textures_root);

		let tiles = match serde_json::from_reader::<_, Vec<TileInfo>>(File::open(tiles_path)?)
		{
			Ok(mut tiles) =>
			{
				tiles.iter_mut().skip(1).for_each(|tile|
				{
					tile.texture = textures_root.join(&tile.texture);
				});

				tiles
			},
			Err(err) => return Err(err.into())
		};

		Ok(Self{tiles})
	}

	pub fn names_map(&self) -> HashMap<&str, Tile>
	{
		self.tiles.iter().enumerate().map(|(index, tile_info)|
		{
			(tile_info.name.as_str(), Tile::new(index))
		}).collect()
	}

	pub fn tile_named(&self, name: &str) -> Option<Tile>
	{
		self.tiles.iter().position(|tile_info|
		{
			tile_info.name == name
		}).map(Tile::new)
	}

	pub fn info(&self, tile: Tile) -> &TileInfo
	{
		self.tiles.get(tile.id()).unwrap()
	}

	pub fn info_map(&self) -> TileInfoMap
	{
		TileInfoMap::new(self)
	}

	pub fn texture_row_size(&self) -> usize
	{
		((self.tiles.len() - 1) as f64).sqrt().ceil() as usize
	}

	pub fn texture(
		&self,
		resource_uploader: &mut ResourceUploader
	) -> Result<Texture, ImageError>
	{
		let textures = self.tiles.iter().skip(1).map(|tile_info|
		{
			let image = image::open(&tile_info.texture)?
				.resize_exact(TILE_SIZE as u32, TILE_SIZE as u32, FilterType::Lanczos3);

			SimpleImage::try_from(image)
		}).collect::<Result<Vec<SimpleImage>, _>>()?;

		let side = self.texture_row_size();

		let row = side * TILE_SIZE;
		let combined_images = (0..(row * row)).map(|index|
		{
			let pixel_x = index % row;
			let pixel_y = index / row;

			let cell_x = pixel_x / TILE_SIZE;
			let cell_y = pixel_y / TILE_SIZE;

			if let Some(cell_texture) = textures.get(cell_y * side + cell_x)
			{
				cell_texture.get_pixel(pixel_x % TILE_SIZE, pixel_y % TILE_SIZE)
			} else
			{
				Color::new(0, 0, 0, 255)
			}
		}).collect();

		let tilemap = SimpleImage::new(combined_images, row, row);

		Ok(Texture::new(resource_uploader, tilemap.into()))
	}
}