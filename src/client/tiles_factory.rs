use std::{
    f32,
    rc::Rc,
    sync::Arc
};

use image::error::ImageError;

use parking_lot::{RwLock, Mutex};

use nalgebra::{Vector2, Vector3};

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
    Side2d,
    SkyOccludingVertex,
    SkyLightVertex,
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

pub const OCCLUDER_PADDING: f32 = TILE_SIZE * 0.01;
pub const LIGHT_PADDING: f32 = OCCLUDER_PADDING / TILE_SIZE;

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
    pub position: Vector2<f32>,
    pub size: Vector2<f32>
}

#[derive(Debug)]
pub enum SkyLightKind
{
    Surround,
    Cap,
    OuterCorner,
    DoubleStraight,
    Straight
}

#[derive(Debug)]
pub struct SkyLightValue
{
    pub kind: SkyLightKind,
    pub rotation: Side2d
}

impl SkyLightValue
{
    pub fn build(&self) -> (Vec<[f32; 2]>, Vec<u16>, Vec<f32>)
    {
        const FRACTION: f32 = 0.8 * 0.5;

        const OVEREXTEND: f32 = FRACTION;

        const LOW: f32 = -LIGHT_PADDING;
        const HIGH: f32 = 1.0 + LIGHT_PADDING;

        let (vertices, indices, intensities) = match self.kind
        {
            SkyLightKind::Surround =>
            {
                (vec![
                    [LOW, LOW], [HIGH, LOW],
                    [FRACTION, FRACTION], [1.0 - FRACTION, FRACTION],
                    [FRACTION, 1.0 - FRACTION], [1.0 - FRACTION, 1.0 - FRACTION],
                    [LOW, HIGH], [HIGH, HIGH]
                ], vec![
                    0, 3, 1,
                    0, 2, 3,
                    0, 4, 2,
                    0, 6, 4,
                    3, 5, 7,
                    3, 7, 1,
                    4, 6, 7,
                    4, 7, 5
                ], vec![
                    1.0, 1.0,
                    0.0, 0.0,
                    0.0, 0.0,
                    1.0, 1.0
                ])
            },
            SkyLightKind::Cap =>
            {
                (vec![
                    [0.0, LOW], [HIGH, LOW],
                    [-OVEREXTEND, FRACTION], [1.0 - FRACTION, FRACTION],
                    [-OVEREXTEND, 1.0 - FRACTION], [1.0 - FRACTION, 1.0 - FRACTION],
                    [0.0, HIGH], [HIGH, HIGH]
                ], vec![
                    0, 3, 1,
                    0, 2, 3,
                    3, 7, 1,
                    3, 5, 7,
                    4, 6, 5,
                    5, 6, 7
                ], vec![
                    1.0, 1.0,
                    0.0, 0.0,
                    0.0, 0.0,
                    1.0, 1.0
                ])
            },
            SkyLightKind::OuterCorner =>
            {
                (vec![
                    [1.0 - FRACTION, -OVEREXTEND], [HIGH, 0.0],
                    [-OVEREXTEND, 1.0 - FRACTION], [1.0 - FRACTION, 1.0 - FRACTION],
                    [0.0, HIGH], [HIGH, HIGH]
                ], vec![
                    0, 3, 1,
                    2, 4, 3,
                    3, 4, 5,
                    3, 5, 1
                ], vec![
                    0.0, 1.0,
                    0.0, 0.0,
                    1.0, 1.0
                ])
            },
            SkyLightKind::DoubleStraight =>
            {
                (vec![
                    [0.0, LOW], [1.0, LOW],
                    [-OVEREXTEND, FRACTION], [1.0 + OVEREXTEND, FRACTION],
                    [-OVEREXTEND, 1.0 - FRACTION], [1.0 + OVEREXTEND, 1.0 - FRACTION],
                    [0.0, HIGH], [1.0, HIGH]
                ], vec![
                    0, 3, 1,
                    0, 2, 3,
                    4, 7, 5,
                    4, 6, 7
                ], vec![
                    1.0, 1.0,
                    0.0, 0.0,
                    0.0, 0.0,
                    1.0, 1.0
                ])
            },
            SkyLightKind::Straight =>
            {
                (vec![
                    [-OVEREXTEND, 1.0 - FRACTION], [1.0 + OVEREXTEND, 1.0 - FRACTION],
                    [0.0, HIGH], [1.0, HIGH]
                ], vec![
                    0, 3, 1,
                    0, 2, 3
                ], vec![
                    0.0, 0.0,
                    1.0, 1.0
                ])
            }
        };

        (vertices, indices, intensities)
    }
}

#[derive(Debug)]
pub struct SkyLight
{
    pub position: Vector2<f32>,
    pub value: SkyLightValue
}

impl SkyLight
{
    pub fn build(&self) -> (Vec<[f32; 2]>, Vec<u16>, Vec<f32>)
    {
        let (vertices, indices, intensities) = self.value.build();

        (vertices.into_iter().map(|v|
        {
            const SIZE: f32 = 1.0;

            let vertex = Vector2::from(v);
            let rotated_vertex = match self.value.rotation
            {
                Side2d::Right => vertex,
                Side2d::Back => Vector2::new(SIZE - vertex.y, vertex.x),
                Side2d::Left => Vector2::new(SIZE - vertex.x, SIZE - vertex.y),
                Side2d::Front => Vector2::new(vertex.y, SIZE - vertex.x)
            };

            (rotated_vertex * TILE_SIZE + self.position).into()
        }).collect(), indices, intensities)
    }
}

#[derive(Debug)]
pub struct ChunkInfo
{
    model: Arc<RwLock<Model>>,
    transform: Transform
}

#[derive(Debug, Clone)]
struct ExtendableModel(pub Model);

impl ExtendableModel
{
    fn extend(&mut self, vertices: impl IntoIterator<Item=[f32; 3]>, indices: impl IntoIterator<Item=u16>)
    {
        let start_index = self.0.vertices.len() as u16;

        self.0.vertices.extend(vertices);
        self.0.indices.extend(indices.into_iter().map(|index| start_index + index));
    }
}

#[derive(Debug, Clone)]
pub struct ChunkModelBuilder
{
    model: ChunkSlice<ExtendableModel>,
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
            ExtendableModel(Model::new())
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
            let id = if let Some(x) = self.tilemap.info_existing(tile).get_weighted_texture()
            {
                x
            } else
            {
                eprintln!("tried to get textures of tile {tile:?}, got none");
                return;
            };

            let uvs = self.tile_uvs(tile, id as usize);

            self.model[chunk_height].0.uvs.extend(uvs);
        }

        {
            let (vertices, indices) = self.tile_vertices(pos);

            self.model[chunk_height].extend(vertices, indices);
        }
    }

    fn tile_uvs(&self, tile: TileExisting, id: usize) -> [[f32; 2]; 4]
    {
        let side = self.tilemap.texture_row_size();

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

        let a = [x, y];
        let b = [x, y_end];
        let c = [x_end, y];
        let d = [x_end, y_end];

        match tile.rotation()
        {
            TileRotation::Up => [a, b, c, d],
            TileRotation::Down =>
            {
                [d, c, b, a]
            },
            TileRotation::Right =>
            {
                [b, d, a, c]
            },
            TileRotation::Left =>
            {
                [c, a, d, b]
            }
        }
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

        self.model.map(|ExtendableModel(model)|
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

#[derive(Debug, Clone)]
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

        let texture = {
            let tilemap = tilemap.generate_tilemap(
                init_info.partial.builder_wrapper.resource_uploader_mut(),
                base_textures.into_iter().filter(|x| !x.is_empty())
            );

            Arc::new(Mutex::new(tilemap))
        };

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
        &self,
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
        &self,
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
        &self,
        occluders: ChunkSlice<Box<[VerticalOccluder]>>
    ) -> ChunkSlice<Box<[SolidObject<SkyOccludingVertex>]>>
    {
        occluders.map(|occluders|
        {
            occluders.iter().map(|occluder|
            {
                let transform = Transform{
                    position: Vector3::new(occluder.position.x, occluder.position.y, 0.0),
                    scale: Vector3::new(occluder.size.x, occluder.size.y, 1.0),
                    ..Default::default()
                };

                self.object_factory.create_solid(self.square.clone(), transform)
            }).collect()
        })
    }

    pub fn build_sky_lights(
        &self,
        pos: GlobalPos,
        lights: ChunkSlice<Box<[SkyLight]>>
    ) -> ChunkSlice<Option<SolidObject<SkyLightVertex>>>
    {
        let position = Chunk::position_of_chunk(pos);

        lights.map(|lights|
        {
            if lights.is_empty()
            {
                return None;
            }

            let mut model = ExtendableModel(Model::new());

            lights.iter().for_each(|light|
            {
                let (vertices, indices, intensities) = light.build();

                model.0.uvs.extend(intensities.into_iter().map(|x| [x, 0.0]));
                model.extend(vertices.into_iter().map(|[x, y]| [x, y, 0.0]), indices);
            });

            let transform = Transform{
                position,
                ..Default::default()
            };

            Some(self.object_factory.create_solid(Arc::new(RwLock::new(model.0)), transform))
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
