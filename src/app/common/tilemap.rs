use std::{
	io,
    fmt,
    iter,
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

use strum_macros::EnumIter;

use enum_amount::EnumCount;

use yanyaengine::object::{
    resource_uploader::ResourceUploader,
    texture::{Color, SimpleImage, Texture}
};

use crate::common::world::Tile;


const TEXTURE_TILE_SIZE: usize = 32;

const PADDING: usize = 1;
const PADDED_TILE_SIZE: usize = TEXTURE_TILE_SIZE + PADDING * 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileInfoRaw
{
	pub name: String,
	pub texture: PathBuf
}

#[derive(Debug, Clone)]
pub struct TileInfo
{
	pub name: String,
	pub transparent: bool
}

impl TileInfo
{
    fn from_raw(texture: &SimpleImage, tile_raw: TileInfoRaw) -> Self
    {
        let transparent = texture.colors.iter().any(|color|
        {
            color.a != u8::MAX
        });

        TileInfo{
            name: tile_raw.name,
            transparent
        }
    }
}

#[derive(Debug, EnumIter, EnumCount)]
pub enum GradientMask
{
	Outer,
	Inner
}

pub struct TileMapWithTextures
{
    pub tilemap: TileMap,
	pub gradient_mask: SimpleImage,
    pub textures: Vec<SimpleImage>
}

#[derive(Debug)]
pub enum TileMapError
{
    Io(io::Error),
    Image{error: ImageError, path: Option<PathBuf>}
}

impl fmt::Display for TileMapError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::Io(x) => x.to_string(),
            Self::Image{error, path} =>
            {
                let err = error.to_string();

                if let Some(path) = path
                {
                    let path = path.display();

                    format!("error at {path}: {err}")
                } else
                {
                    err
                }
            }
        };

        write!(f, "{s}")
    }
}

impl From<serde_json::Error> for TileMapError
{
    fn from(value: serde_json::Error) -> Self
    {
        Self::Io(value.into())
    }
}

impl From<io::Error> for TileMapError
{
    fn from(value: io::Error) -> Self
    {
        Self::Io(value)
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
	pub fn parse(
        tiles_path: &str,
        textures_root: &str
    ) -> Result<TileMapWithTextures, TileMapError>
	{
		let textures_root = Path::new(textures_root);
		let gradient_mask = textures_root.join("gradient.png");

        let gradient_mask = Self::load_texture(
            TEXTURE_TILE_SIZE as u32 * 2,
            TEXTURE_TILE_SIZE as u32,
            gradient_mask
        )?;

		let tiles = serde_json::from_reader::<_, Vec<TileInfoRaw>>(File::open(tiles_path)?)?;

        let textures = tiles.iter().map(|tile_raw|
        {
            let texture = textures_root.join(&tile_raw.texture);

            Self::load_texture(
                TEXTURE_TILE_SIZE as u32,
                TEXTURE_TILE_SIZE as u32,
                texture
            )
        }).collect::<Result<Vec<SimpleImage>, _>>()?;

        let tiles = iter::once(TileInfo{
            name: "air".to_owned(),
            transparent: true
        }).chain(tiles.into_iter().zip(textures.iter()).map(|(tile_raw, texture)|
        {
            TileInfo::from_raw(texture, tile_raw)
        })).collect();

		Ok(TileMapWithTextures{
            tilemap: Self{tiles},
            gradient_mask,
            textures
        })
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
		fraction / (self.texture_row_size() * PADDED_TILE_SIZE) as f32
	}

    fn load_texture(
        width: u32,
        height: u32,
        path: PathBuf
    ) -> Result<SimpleImage, TileMapError>
    {
		let image = image::open(&path).map_err(|error| 
        {
            TileMapError::Image{error, path: Some(path)}
        })?;

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

		SimpleImage::try_from(image).map_err(|error| TileMapError::Image{error, path: None})
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

		let row = side * PADDED_TILE_SIZE;
		let mut tilemap = SimpleImage::new(vec![Color::new(0, 0, 0, 255); row * row], row, row);

        textures.iter().enumerate().for_each(|(index, texture)|
        {
            let x = (index % side) * PADDED_TILE_SIZE;
            let y = (index / side) * PADDED_TILE_SIZE;

            let nearest_pixel = |value: usize|
            {
                value.saturating_sub(PADDING).min(TEXTURE_TILE_SIZE - PADDING)
            };

            let pad = |tilemap: &mut SimpleImage, begin, offset|
            {
                for p_y in 0..PADDED_TILE_SIZE
                {
                    let border_color = texture.get_pixel(
                        begin,
                        nearest_pixel(p_y)
                    );

                    tilemap.set_pixel(border_color, x + begin + offset, y + p_y);
                }

                for p_x in 0..PADDED_TILE_SIZE
                {
                    let border_color = texture.get_pixel(
                        nearest_pixel(p_x),
                        begin
                    );

                    tilemap.set_pixel(border_color, x + p_x, y + begin + offset);
                }
            };

            pad(&mut tilemap, 0, 0);
            pad(&mut tilemap, TEXTURE_TILE_SIZE - 1, PADDING * 2);

            tilemap.blit(texture, x + PADDING, y + PADDING);
        });

		Texture::new(resource_uploader, tilemap.into())
	}
}

impl Index<Tile> for TileMap
{
	type Output = TileInfo;

	fn index(&self, tile: Tile) -> &Self::Output
	{
		self.tiles.get(tile.id()).unwrap()
	}
}
