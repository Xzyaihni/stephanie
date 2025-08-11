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

use crate::common::{
    WeightedPicker,
    generic_info::load_texture_path,
    world::{Tile, TileExisting}
};


pub const PADDING: f32 = 0.01;

const TEXTURE_TILE_SIZE: usize = 16;

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
    pub inherit: Option<String>,
    pub textures: Option<Vec<f32>>,
    pub health: Option<f32>,
    pub drawable: Option<bool>,
    pub special: Option<SpecialTile>,
    pub colliding: Option<bool>,
    pub transparent: Option<bool>,
    pub texture: Option<String>
}

impl TileInfoRaw
{
    fn has_texture(&self) -> bool
    {
        self.drawable.unwrap_or(true)
    }

    fn combine(&self, other: &Self) -> Self
    {
        let mut this = self.clone();

        this.name = other.name.clone();

        macro_rules! with_fields
        {
            ($($name:ident),+) =>
            {
                $(
                    if other.$name.is_some()
                    {
                        this.$name = other.$name.clone();
                    }
                )+
            }
        }

        with_fields!(health, drawable, special, colliding, transparent);

        this
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TileTexture
{
    pub weight: f32,
    pub id: u32
}

#[derive(Debug, Clone)]
pub struct TileInfo
{
    pub name: String,
    pub textures: Vec<TileTexture>,
    pub health: f32,
    pub drawable: bool,
    pub special: Option<SpecialTile>,
    pub colliding: bool,
    pub transparent: bool
}

impl TileInfo
{
    fn from_raw(
        textures: &Vec<(u32, SimpleImage)>,
        tile_raw: TileInfoRaw
    ) -> Self
    {
        let mut this = TileInfo{
            name: tile_raw.name,
            textures: {
                let total_textures_weight: f32 = tile_raw.textures.iter().flatten().copied().sum();

                if textures.len() == 1
                {
                    let id = textures.first().unwrap().0;
                    vec![TileTexture{weight: 1.0, id}]
                } else
                {
                    tile_raw.textures.into_iter().flatten().zip(textures.iter()).map(|(weight, (id, _))|
                    {
                        TileTexture{weight: weight / total_textures_weight, id: *id}
                    }).collect()
                }
            },
            health: tile_raw.health.unwrap_or(1.0),
            drawable: tile_raw.drawable.unwrap_or(true),
            special: tile_raw.special,
            colliding: tile_raw.colliding.unwrap_or(true),
            transparent: tile_raw.transparent.unwrap_or_else(||
            {
                textures.first().as_ref().map(|(_, texture)| texture.colors.iter().any(|color|
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

    pub fn get_weighted_texture(&self) -> Option<u32>
    {
        WeightedPicker::new(1.0, self.textures.iter()).pick_by(|x| x.weight as f64).map(|x| x.id)
    }
}

pub struct TileMapWithTextures
{
    pub tilemap: TileMap,
    pub textures: Vec<Vec<(u32, SimpleImage)>>
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

        let mut tiles = serde_json::from_reader::<_, Vec<TileInfoRaw>>(File::open(tiles_path)?)?;

        (0..tiles.len()).for_each(|index|
        {
            if tiles[index].inherit.is_none()
            {
                return;
            }

            if let Some(inherit_index) = tiles.iter().position(|x| x.name == *tiles[index].inherit.as_ref().unwrap())
            {
                tiles[index] = tiles[inherit_index].combine(&tiles[index]);
            } else
            {
                eprintln!("inherit tile named `{}` not found", tiles[index].inherit.as_ref().unwrap());
            }
        });

        let textures = tiles.iter().scan(0, |current_id: &mut u32, tile_raw: &TileInfoRaw| -> Option<_>
        {
            let value = (|tile_raw: &TileInfoRaw|
            {
                if tile_raw.has_texture()
                {
                    let texture_name = tile_raw.texture.as_ref().unwrap_or(&tile_raw.name);

                    let loader = |name: &str| -> SimpleImage
                    {
                        Self::load_texture(
                            TEXTURE_TILE_SIZE as u32,
                            TEXTURE_TILE_SIZE as u32,
                            PathBuf::from(load_texture_path(textures_root, name))
                        ).unwrap_or_else(|err|
                        {
                            eprintln!("{err}, using empty image");
                            SimpleImage::filled(Color::new(0, 0, 0, 0), TEXTURE_TILE_SIZE, TEXTURE_TILE_SIZE)
                        })
                    };

                    let textures = if tile_raw.textures.as_ref().map(|x| x.is_empty()).unwrap_or(true)
                    {
                        vec![(*current_id, loader(texture_name))]
                    } else
                    {
                        tile_raw.textures.iter().flatten().enumerate().map(|(index, _)|
                        {
                            (*current_id + index as u32, format!("{texture_name}{}", index + 1))
                        }).map(|(id, x)|
                        {
                            (id, loader(x.as_ref()))
                        }).collect::<Vec<_>>()
                    };

                    *current_id += textures.len() as u32;

                    textures
                } else
                {
                    Vec::new()
                }
            })(tile_raw);

            Some(value)
        }).collect::<Vec<Vec<(u32, SimpleImage)>>>();

        let tiles = tiles.into_iter().zip(textures.iter()).map(|(tile_raw, textures)|
        {
            TileInfo::from_raw(textures, tile_raw)
        }).collect();

        let air = TileInfo{
            name: "air".to_owned(),
            textures: Vec::new(),
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

    pub fn info_existing(&self, tile: TileExisting) -> &TileInfo
    {
        self.tiles.get(tile.id()).unwrap()
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
        fraction / (self.texture_row_size() * TEXTURE_TILE_SIZE) as f32
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
        textures: impl Iterator<Item=Vec<(u32, SimpleImage)>>
    ) -> Texture
    {
        let side = self.texture_row_size();

        let row = side * TEXTURE_TILE_SIZE;
        let mut tilemap = SimpleImage::new(vec![Color::new(0, 0, 0, 255); row * row], row, row);

        textures.flatten().for_each(|(id, texture)|
        {
            let id = id as usize;
            let x = (id % side) * TEXTURE_TILE_SIZE;
            let y = (id / side) * TEXTURE_TILE_SIZE;

            tilemap.blit(&texture, x, y);
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
