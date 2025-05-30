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

use yanyaengine::object::{
    resource_uploader::ResourceUploader,
    texture::{Color, SimpleImage, Texture}
};

use crate::common::world::Tile;


const TEXTURE_TILE_SIZE: usize = 16;

// this makes the texture size always a power of 2
pub const PADDING: usize = TEXTURE_TILE_SIZE / 2;
const PADDED_TILE_SIZE: usize = TEXTURE_TILE_SIZE + PADDING * 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecialTile
{
    StairsUp,
    StairsDown
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileInfoRaw
{
    pub name: String,
    pub health: Option<f32>,
    pub drawable: Option<bool>,
    pub special: Option<SpecialTile>,
    pub colliding: Option<bool>,
    pub transparent: Option<bool>,
    pub texture: Option<PathBuf>
}

impl TileInfoRaw
{
    fn has_texture(&self) -> bool
    {
        self.drawable.unwrap_or(true)
    }
}

#[derive(Debug, Clone)]
pub struct TileInfo
{
    pub name: String,
    pub health: f32,
    pub drawable: bool,
    pub special: Option<SpecialTile>,
    pub colliding: bool,
    pub transparent: bool
}

impl TileInfo
{
    fn from_raw(texture: &Option<SimpleImage>, tile_raw: TileInfoRaw) -> Self
    {
        let mut this = TileInfo{
            name: tile_raw.name,
            health: tile_raw.health.unwrap_or(1.0),
            drawable: tile_raw.drawable.unwrap_or(true),
            special: tile_raw.special,
            colliding: tile_raw.colliding.unwrap_or(true),
            transparent: tile_raw.transparent.unwrap_or_else(||
            {
                texture.as_ref().map(|texture| texture.colors.iter().any(|color|
                {
                    color.a != u8::MAX
                })).unwrap_or(true)
            })
        };

        #[allow(clippy::collapsible_match, clippy::single_match)]
        if let Some(special) = this.special.as_mut()
        {
            match special
            {
                SpecialTile::StairsUp =>
                {
                    this.colliding = false;
                    this.transparent = true;
                },
                _ => ()
            }
        }

        if !this.drawable
        {
            this.transparent = true;
        }

        this
    }
}

pub struct TileMapWithTextures
{
    pub tilemap: TileMap,
    pub textures: Vec<Option<SimpleImage>>
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
    air: TileInfo,
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

        let tiles = serde_json::from_reader::<_, Vec<TileInfoRaw>>(File::open(tiles_path)?)?;

        let textures = tiles.iter().map(|tile_raw|
        {
            if tile_raw.has_texture()
            {
                let default_texture = format!("{}.png", tile_raw.name).into();

                let texture = textures_root.join(tile_raw.texture.as_ref()
                    .unwrap_or(&default_texture));

                Self::load_texture(
                    TEXTURE_TILE_SIZE as u32,
                    TEXTURE_TILE_SIZE as u32,
                    texture
                ).map(Option::Some)
            } else
            {
                Ok(None)
            }
        }).collect::<Result<Vec<Option<SimpleImage>>, _>>()?;

        let tiles = tiles.into_iter().zip(textures.iter()).map(|(tile_raw, texture)|
        {
            TileInfo::from_raw(texture, tile_raw)
        }).collect();

        let air = TileInfo{
            name: "air".to_owned(),
            health: 0.0,
            drawable: false,
            special: None,
            colliding: false,
            transparent: true
        };

        Ok(TileMapWithTextures{
            tilemap: Self{air, tiles},
            textures
        })
    }

    pub fn names_map(&self) -> HashMap<&str, Tile>
    {
        self.names_iter().collect()
    }

    pub fn names_owned_map(&self) -> HashMap<String, Tile>
    {
        self.names_iter().map(|(key, value)| (key.to_owned(), value)).collect()
    }

    fn names_iter(&self) -> impl Iterator<Item=(&str, Tile)>
    {
        self.tiles.iter().enumerate().map(|(index, tile_info)|
        {
            (tile_info.name.as_str(), Tile::new(index))
        }).chain(iter::once((self.air.name.as_str(), Tile::none())))
    }

    pub fn tile_named(&self, name: &str) -> Option<Tile>
    {
        if self.air.name == name
        {
            return Some(Tile::none());
        }

        self.tiles.iter().position(|tile_info|
        {
            tile_info.name == name
        }).map(Tile::new)
    }

    pub fn info(&self, tile: Tile) -> &TileInfo
    {
        tile.id().map(|id| self.tiles.get(id).unwrap()).unwrap_or(&self.air)
    }

    pub fn len(&self) -> usize
    {
        self.tiles.len()
    }

    fn visible_tiles(&self) -> usize
    {
        self.tiles.iter().filter(|x| x.drawable).count()
    }

    pub fn texture_row_size(&self) -> usize
    {
        (((self.visible_tiles()) as f64).sqrt().ceil() as usize).max(2)
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

        Ok(SimpleImage::from(image))
    }

    pub fn generate_tilemap(
        &self,
        resource_uploader: &mut ResourceUploader,
        textures: &[Option<SimpleImage>]
    ) -> Texture
    {
        let side = self.texture_row_size();

        let row = side * PADDED_TILE_SIZE;
        let mut tilemap = SimpleImage::new(vec![Color::new(0, 0, 0, 255); row * row], row, row);

        textures.iter().enumerate().filter_map(|(index, texture)|
        {
            texture.as_ref().map(|texture| (index, texture))
        }).for_each(|(index, texture)|
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

            for p in 0..PADDING
            {
                pad(&mut tilemap, 0, p);
                pad(&mut tilemap, TEXTURE_TILE_SIZE - 1, PADDING + 1 + p);
            }

            tilemap.blit(texture, x + PADDING, y + PADDING);
        });

        Texture::new(
            resource_uploader,
            tilemap.into()
        )
    }
}

impl Index<Tile> for TileMap
{
    type Output = TileInfo;

    fn index(&self, tile: Tile) -> &Self::Output
    {
        self.info(tile)
    }
}
