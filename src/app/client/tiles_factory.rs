use std::{
    f32,
    rc::Rc,
    sync::Arc
};

use image::error::ImageError;

use parking_lot::{RwLock, Mutex};

use nalgebra::Vector3;

use yanyaengine::{
    Object,
    ObjectInfo,
    SolidObject,
    Transform,
    ObjectFactory,
    DefaultModel,
    object::{
        Texture,
        Model
    },
    game_object::*
};

use crate::common::{
    SkyOccludingVertex,
    OccludingPlane,
    TileMap,
    TileMapWithTextures,
    tilemap::{PADDING, TileInfo},
    world::{
        CHUNK_SIZE,
        TILE_SIZE,
        Chunk,
        GlobalPos,
        Pos3,
        Tile,
        TileExisting,
        TileRotation,
        chunk::ChunkLocal,
        overmap::visual_chunk::{OccluderCached, LineIndices}
    }
};


pub type ChunkSlice<T> = [T; CHUNK_SIZE];

#[derive(Debug, Clone, Copy)]
pub struct OccluderInfo
{
    pub line_indices: LineIndices,
    pub position: Vector3<f32>,
    pub inside: bool,
    pub horizontal: bool,
    pub length: f32
}

#[derive(Debug)]
pub struct VerticalOccluder
{
    pub position: Vector3<f32>,
    pub size: Vector3<f32>
}

#[derive(Debug)]
pub struct ChunkInfo
{
    model: Arc<RwLock<Model>>,
    transform: Transform
}

#[derive(Debug, Clone)]
pub struct ChunkModelBuilder
{
    model: ChunkSlice<Model>,
    tilemap: Arc<TileMap>
}

impl ChunkModelBuilder
{
    pub fn new(
        tilemap: Arc<TileMap>
    ) -> Self
    {
        let model = (0..CHUNK_SIZE).map(|_|
        {
            Model::new()
        }).collect::<Vec<_>>().try_into().unwrap();

        Self{model, tilemap}
    }

    pub fn create(&mut self, pos: ChunkLocal, tile: TileExisting)
    {
        self.create_inner(pos, tile);
    }

    fn create_inner(
        &mut self,
        chunk_pos: ChunkLocal,
        tile: TileExisting
    )
    {
        let mut pos = Pos3::<f32>::from(*chunk_pos.pos()) * TILE_SIZE;
        pos.z += TILE_SIZE;

        let chunk_height = chunk_pos.pos().z;

        {
            let uvs = self.tile_uvs(tile, false);

            self.model[chunk_height].uvs.extend(uvs);
        }

        {
            let (vertices, indices) = self.tile_vertices(pos);

            let model = &mut self.model[chunk_height];

            let start_index = model.vertices.len() as u16;

            model.vertices.extend(vertices);
            model.indices.extend(indices.into_iter().map(|index| start_index + index));
        }
    }

    fn tile_uvs(&self, tile: TileExisting, flip_xy: bool) -> impl Iterator<Item=[f32; 2]>
    {
        let side = self.tilemap.texture_row_size();

        let id = tile.id();
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

        let mut a = [x, y];
        let mut b = [x, y_end];
        let mut c = [x_end, y];
        let mut d = [x_end, y_end];

        match tile.rotation()
        {
            TileRotation::Up => (),
            TileRotation::Down =>
            {
                (a, b, c, d) = (d, c, b, a);
            },
            TileRotation::Right =>
            {
                (a, b, c, d) = (b, d, a, c);
            },
            TileRotation::Left =>
            {
                (a, b, c, d) = (c, a, d, b);
            }
        }

        if flip_xy
        {
            (b, c) = (c, b);
        }

        [a, b, c, d].into_iter()
    }

    fn tile_vertices(&self, pos: Pos3<f32>) -> ([[f32; 3]; 4], [u16; 6])
    {
        let (x, y, z) = (pos.x, pos.y, pos.z - TILE_SIZE);
        let (x_end, y_end) = (pos.x + TILE_SIZE, pos.y + TILE_SIZE);

        let vertices = [
            [x, y, z],
            [x, y_end, z],
            [x_end, y, z],
            [x_end, y_end, z]
        ];

        let indices = [0, 1, 2, 1, 3, 2];

        (vertices, indices)
    }

    pub fn build(
        self,
        pos: GlobalPos
    ) -> ChunkSlice<Option<ChunkInfo>>
    {
        let transform = Chunk::transform_of_chunk(pos);

        self.model.map(|model|
        {
            (!model.vertices.is_empty()).then(||
            {
                ChunkInfo{
                    model: Arc::new(RwLock::new(model)),
                    transform: transform.clone()
                }
            })
        })
    }
}

#[derive(Debug)]
pub struct TilesFactory
{
    object_factory: Rc<ObjectFactory>,
    square: Arc<RwLock<Model>>,
    tilemap: Arc<TileMap>,
    texture: Arc<Mutex<Texture>>
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
            textures: base_textures
        } = tilemap;

        let mut make_tilemap = |textures: &[_]|
        {
            let tilemap = tilemap.generate_tilemap(
                init_info.partial.builder_wrapper.resource_uploader_mut(),
                textures
            );

            Arc::new(Mutex::new(tilemap))
        };

        let texture = make_tilemap(&base_textures);

        let tilemap = Arc::new(tilemap);

        let square = {
            let assets = init_info.partial.assets.lock();

            let id = assets.default_model(DefaultModel::Square);
            assets.model(id).clone()
        };

        Ok(Self{
            object_factory: init_info.partial.object_factory.clone(),
            square,
            tilemap,
            texture
        })
    }

    pub fn build(
        &mut self,
        chunk_info: ChunkSlice<Option<ChunkInfo>>
    ) -> ChunkSlice<Option<Object>>
    {
        chunk_info.map(|chunk_info|
        {
            chunk_info.map(|ChunkInfo{model, transform}|
            {
                let object_info = ObjectInfo{
                    model,
                    texture: self.texture.clone(),
                    transform
                };

                self.object_factory.create(object_info)
            })
        })
    }

    pub fn build_occluders(
        &mut self,
        occluders: ChunkSlice<Box<[OccluderInfo]>>
    ) -> ChunkSlice<Box<[OccluderCached]>>
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

                let occluding = self.object_factory.create_occluding(transform, occluder.inside ^ occluder.horizontal);

                OccluderCached{
                    occluder: OccludingPlane::new(occluding),
                    indices: occluder.line_indices,
                    visible: true
                }
            }).collect()
        })
    }

    pub fn build_vertical_occluders(
        &mut self,
        occluders: ChunkSlice<Box<[VerticalOccluder]>>
    ) -> ChunkSlice<Box<[SolidObject<SkyOccludingVertex>]>>
    {
        occluders.map(|occluders|
        {
            occluders.iter().map(|occluder|
            {
                let transform = Transform{
                    position: occluder.position,
                    scale: occluder.size,
                    ..Default::default()
                };

                self.object_factory.create_solid(self.square.clone(), transform)
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
