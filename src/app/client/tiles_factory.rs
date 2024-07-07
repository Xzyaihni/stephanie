use std::{
    f32,
    iter,
    array,
    rc::Rc,
    sync::Arc,
    ops::{Index, IndexMut}
};

use image::error::ImageError;

use strum::IntoEnumIterator;

use parking_lot::RwLock;

use nalgebra::Vector3;

use yanyaengine::{
    Object,
    ObjectInfo,
    Transform,
    ObjectFactory,
    ShaderId,
    object::{
        Texture,
        Model
    },
    game_object::*
};

use crate::common::{
    OccludingPlane,
    TileMap,
    TileMapWithTextures,
    tilemap::{PADDING, GradientMask, TileInfo},
    world::{
        CHUNK_SIZE,
        TILE_SIZE,
        Chunk,
        PosDirection,
        GlobalPos,
        Pos3,
        Tile,
        chunk::ChunkLocal,
    }
};


pub type ChunkSlice<T> = [T; CHUNK_SIZE];

#[derive(Debug)]
pub struct ChunkObjects<T>
{
    pub normal: T,
    pub gradients: [T; 2]
}

impl<T> ChunkObjects<T>
{
    pub fn repeat(value: T) -> Self
    where
        T: Clone
    {
        Self::repeat_with(|| value.clone())
    }

    pub fn repeat_with(mut value: impl FnMut() -> T) -> Self
    {
        Self{
            gradients: [value(), value()],
            normal: value()
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item=&mut T>
    {
        iter::once(&mut self.normal).chain(self.gradients.iter_mut())
    }
}

impl<T> IntoIterator for ChunkObjects<T>
{
    type Item = T;
    type IntoIter = iter::Chain<iter::Once<T>, array::IntoIter<T, 2>>;

    fn into_iter(self) -> Self::IntoIter
    {
        iter::once(self.normal).chain(self.gradients)
    }
}

impl<T> FromIterator<T> for ChunkObjects<T>
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item=T>
    {
        let mut iter = iter.into_iter();

        let value = ChunkObjects{
            normal: iter.next().unwrap(),
            gradients: [iter.next().unwrap(), iter.next().unwrap()]
        };

        assert!(iter.next().is_none());

        value
    }
}

impl<T> Index<Option<usize>> for ChunkObjects<T>
{
    type Output = T;

    fn index(&self, index: Option<usize>) -> &Self::Output
    {
        if let Some(index) = index
        {
            &self.gradients[index]
        } else
        {
            &self.normal
        }
    }
}

impl<T> IndexMut<Option<usize>> for ChunkObjects<T>
{
    fn index_mut(&mut self, index: Option<usize>) -> &mut Self::Output
    {
        if let Some(index) = index
        {
            &mut self.gradients[index]
        } else
        {
            &mut self.normal
        }
    }
}

pub struct OccluderInfo
{
    pub position: Vector3<f32>,
    pub horizontal: bool,
    pub length: f32
}

pub struct ChunkInfo
{
    model: Arc<RwLock<Model>>,
    transform: Transform,
    texture_index: usize
}

pub struct ChunkModelBuilder
{
    models: ChunkSlice<ChunkObjects<Model>>,
    tilemap: Arc<TileMap>
}

impl ChunkModelBuilder
{
    pub fn new(
        tilemap: Arc<TileMap>
    ) -> Self
    {
        let models = (0..CHUNK_SIZE).map(|_|
        {
            ChunkObjects::repeat_with(Model::new)
        }).collect::<Vec<_>>().try_into().unwrap();

        Self{models, tilemap}
    }

    pub fn create(&mut self, pos: ChunkLocal, tile: Tile)
    {
        self.create_inner(None, pos, tile);
    }

    pub fn create_direction(
        &mut self,
        direction: PosDirection,
        pos: ChunkLocal,
        tile: Tile
    )
    {
        self.create_inner(Some(direction), pos, tile);
    }

    fn create_inner(
        &mut self,
        direction: Option<PosDirection>,
        chunk_pos: ChunkLocal,
        tile: Tile
    )
    {
        let mut pos = Pos3::<f32>::from(*chunk_pos.pos()) * TILE_SIZE;
        pos.z += TILE_SIZE;

        let chunk_height = chunk_pos.pos().z;

        let id = direction.map(|d| Self::direction_texture_index(d) - 1);

        {
            let flip_axes = match direction
            {
                Some(PosDirection::Up) | Some(PosDirection::Down) => true,
                _ => false
            };

            let uvs = self.tile_uvs(tile, flip_axes);

            self.models[chunk_height][id].uvs.extend(uvs);
        }

        {
            if direction.is_some()
            {
                pos.z += 0.0001;
            }

            let vertices = self.tile_vertices(pos);

            self.models[chunk_height][id].vertices.extend(vertices);
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
        let (x, y, z) = (pos.x, pos.y, pos.z - TILE_SIZE);
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

    pub fn build(
        self,
        pos: GlobalPos
    ) -> ChunkSlice<ChunkObjects<Option<ChunkInfo>>>
    {
        let transform = Chunk::transform_of_chunk(pos);

        self.models.map(|models|
        {
            models.into_iter().enumerate()
                .map(|(texture_index, model)|
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
        })
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
    object_factory: Rc<ObjectFactory>,
    tilemap: Arc<TileMap>,
    textures: Vec<Arc<RwLock<Texture>>>
}

#[allow(dead_code)]
impl TilesFactory
{
    pub fn new(
        init_info: &mut InitInfo,
        shader: ShaderId,
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
                shader,
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

    pub fn build(
        &mut self,
        chunk_info: ChunkSlice<ChunkObjects<Option<ChunkInfo>>>
    ) -> ChunkSlice<ChunkObjects<Option<Object>>>
    {
        chunk_info.map(|chunk_info|
        {
            chunk_info.into_iter().map(|chunk_info|
            {
                chunk_info.map(|ChunkInfo{model, transform, texture_index}|
                {
                    let object_info = ObjectInfo{
                        model,
                        texture: self.textures[texture_index].clone(),
                        transform
                    };

                    self.object_factory.create(object_info)
                })
            }).collect()
        })
    }

    pub fn build_occluders(
        &mut self,
        occluders: ChunkSlice<Box<[OccluderInfo]>>
    ) -> ChunkSlice<Box<[OccludingPlane]>>
    {
        occluders.map(|occluders|
        {
            occluders.iter().map(|occluder|
            {
                let transform = Transform{
                    position: occluder.position,
                    scale: Vector3::repeat(occluder.length),
                    rotation: if occluder.horizontal { 0.0 } else { f32::consts::FRAC_PI_2 },
                    ..Default::default()
                };

                let occluding = self.object_factory.create_occluding(transform);

                OccludingPlane::new(occluding)
            }).collect()
        })
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
