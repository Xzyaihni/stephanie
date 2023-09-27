use std::{
	io,
	ops::Index,
	collections::HashMap,
	fs::File,
	sync::Arc,
	path::{Path, PathBuf}
};

use serde::{Serialize, Deserialize};

use image::{
	imageops::FilterType,
	error::ImageError
};

use strum_macros::EnumIter;

use enum_amount::EnumCount;

use crate::{
	common::world::Tile,
	client::game::object::{
		resource_uploader::ResourceUploader,
		texture::{Color, SimpleImage, Texture}
	}
};


pub const TEXTURE_TILE_SIZE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileInfo
{
	pub name: String,
	pub texture: PathBuf,
	pub transparent: bool
}

pub struct TileInfoMap
{
	tilemap: Arc<TileMap>
}

impl TileInfoMap
{
	pub fn new(tilemap: Arc<TileMap>) -> Self
	{
		Self{tilemap}
	}
}

impl Index<Tile> for TileInfoMap
{
	type Output = TileInfo;

	fn index(&self, tile: Tile) -> &Self::Output
	{
		self.tilemap.tiles.get(tile.id()).unwrap()
	}
}

#[derive(Debug, EnumIter, EnumCount)]
pub enum GradientMask
{
	Outer,
	Inner
}

#[derive(Debug)]
pub struct TileMap
{
	gradient_mask: PathBuf,
	tiles: Vec<TileInfo>
}

#[allow(dead_code)]
impl TileMap
{
	pub fn parse(tiles_path: &str, textures_root: &str) -> Result<Self, io::Error>
	{
		let textures_root = Path::new(textures_root);
		let gradient_mask = textures_root.join("gradient.png");

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

		Ok(Self{gradient_mask, tiles})
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

	pub fn len(&self) -> usize
	{
		self.tiles.len()
	}

	pub fn texture_row_size(&self) -> usize
	{
		((self.tiles.len() - 1) as f64).sqrt().ceil() as usize
	}

	pub fn pixel_fraction(&self, fraction: f32) -> f32
	{
		fraction / (self.texture_row_size() * TEXTURE_TILE_SIZE) as f32
	}

	pub fn load_mask(&self) -> Result<SimpleImage, ImageError>
	{
		let image = image::open(&self.gradient_mask)?;

        let width = TEXTURE_TILE_SIZE as u32 * 2;
        let height = TEXTURE_TILE_SIZE as u32;

        let image = if (image.width() == width) && (image.height() == height)
        {
            image
        } else
        {
			image.resize_exact(
				width,
				height,
				FilterType::Lanczos3
			)
        };

		SimpleImage::try_from(image)
	}

	pub fn load_textures(&self) -> Result<Vec<SimpleImage>, ImageError>
	{
		self.tiles.iter().skip(1).map(|tile_info|
		{
			let image = image::open(&tile_info.texture)?;

            let width = TEXTURE_TILE_SIZE as u32;
            let height = TEXTURE_TILE_SIZE as u32;

            let image = if (image.width() == width) && (image.height() == height)
            {
                image
            } else
            {
				image.resize_exact(
					width,
					height,
					FilterType::Lanczos3
				)
            };

			SimpleImage::try_from(image)
		}).collect::<Result<Vec<SimpleImage>, _>>()
	}

	pub fn apply_texture_mask<'a, I>(mask_type: GradientMask, mask: &SimpleImage, textures: I)
	where
		I: Iterator<Item=&'a mut SimpleImage>
	{
		textures.for_each(|texture|
		{
			for y in 0..TEXTURE_TILE_SIZE
			{
				for x in 0..TEXTURE_TILE_SIZE
				{
					let (mask_x, mask_y) = match mask_type
					{
						GradientMask::Outer => (x, y),
						GradientMask::Inner => (TEXTURE_TILE_SIZE + x, y)
					};

					let mask_pixel = mask.get_pixel(mask_x, mask_y);

					let mask = mask_pixel.r;
					let mask = match mask_type
					{
						GradientMask::Inner => mask,
						_ => u8::MAX - mask
					};

					let mut pixel = texture.get_pixel(x, y);

					pixel.a = mask;
					texture.set_pixel(pixel, x, y);
				}
			}
		});
	}

	pub fn generate_tilemap(
		&self,
		resource_uploader: &mut ResourceUploader,
		textures: &[SimpleImage]
	) -> Texture
	{
		let side = self.texture_row_size();

		let row = side * TEXTURE_TILE_SIZE;
		let combined_images = (0..(row * row)).map(|index|
		{
			let pixel_x = index % row;
			let pixel_y = index / row;

			let cell_x = pixel_x / TEXTURE_TILE_SIZE;
			let cell_y = pixel_y / TEXTURE_TILE_SIZE;

			if let Some(cell_texture) = textures.get(cell_y * side + cell_x)
			{
				cell_texture.get_pixel(pixel_x % TEXTURE_TILE_SIZE, pixel_y % TEXTURE_TILE_SIZE)
			} else
			{
				Color::new(0, 0, 0, 255)
			}
		}).collect();

		let tilemap = SimpleImage::new(combined_images, row, row);

		Texture::new(resource_uploader, tilemap.into())
	}
}
