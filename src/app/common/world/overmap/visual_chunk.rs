use std::{
    iter,
    convert,
    collections::HashMap,
    ops::ControlFlow,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{
    Object,
    SolidObject,
    game_object::*
};

use crate::{
    debug_config::*,
    client::{
        VisibilityChecker,
        tiles_factory::{
            ChunkSlice,
            TilesFactory,
            OccluderInfo,
            VerticalOccluder,
            ChunkInfo,
            ChunkModelBuilder
        }
    },
    common::{
        SkyOccludingVertex,
        SkyLightVertex,
        OccludingPlane,
        OccludingCaster,
        TileMap,
        world::{
            Pos3,
            ChunkLocal,
            LocalPos,
            GlobalPos,
            MaybeGroup,
            Chunk,
            Tile,
            TILE_SIZE,
            CHUNK_SIZE,
            overmap::FlatChunksContainer,
            visual_overmap::TileReader
        }
    }
};


#[derive(Default, Clone, Copy)]
struct OccludingState
{
    horizontal: Option<bool>,
    vertical: Option<bool>
}

struct OccluderInfoRaw
{
    position: Vector2<usize>,
    inside: bool,
    horizontal: bool,
    length: usize
}

impl OccluderInfoRaw
{
    fn into_global(self, chunk_position: Vector3<f32>, z: usize) -> OccluderInfo
    {
        let mut tile_position = Vector3::new(self.position.x, self.position.y, z).cast();

        if self.horizontal
        {
            tile_position.x += self.length as f32 * 0.5;
        } else
        {
            tile_position.y += self.length as f32 * 0.5;
        }

        let tile_position = tile_position * TILE_SIZE;

        // a little padding to hide seams
        let padding = TILE_SIZE * 0.01;

        let line_indices = {
            let start = self.position.y * CHUNK_SIZE + self.position.x;
            let step = if self.horizontal { 1 } else { CHUNK_SIZE };

            LineIndices{
                start,
                end: start + step * self.length,
                step
            }
        };

        OccluderInfo{
            line_indices,
            position: chunk_position + tile_position,
            inside: self.inside,
            horizontal: self.horizontal,
            length: self.length as f32 * TILE_SIZE + padding
        }
    }
}

struct VerticalOccluderRaw
{
    position: Vector2<usize>,
    size: Vector2<usize>
}

impl VerticalOccluderRaw
{
    fn into_global(self, chunk_position: Vector2<f32>) -> VerticalOccluder
    {
        let tile_position = Vector2::new(self.position.x, self.position.y).cast() * TILE_SIZE;

        // a little padding to hide seams
        let padding = TILE_SIZE * 0.01;

        let size = self.size.cast() * TILE_SIZE;
        let half_size = size / 2.0;

        VerticalOccluder{
            position: chunk_position + tile_position + half_size,
            size: size + Vector2::repeat(padding)
        }
    }
}

pub struct VisualChunkInfo
{
    infos: ChunkSlice<Option<ChunkInfo>>,
    occluders: ChunkSlice<Box<[OccluderInfo]>>,
    vertical_occluders: ChunkSlice<Box<[VerticalOccluder]>>,
    sky: ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>,
    total_sky: ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>,
    draw_indices: ChunkSlice<Box<[usize]>>,
    draw_next: ChunkSlice<bool>
}

#[derive(Debug, Clone, Copy)]
pub struct LineIndices
{
    start: usize,
    end: usize,
    step: usize
}

impl LineIndices
{
    pub fn iter(self) -> impl Iterator<Item=usize>
    {
        iter::successors(Some(self.start), move |state|
        {
            (*state != self.end).then_some(*state + self.step)
        })
    }
}

#[derive(Debug)]
pub struct OccluderCached
{
    pub occluder: OccludingPlane,
    pub indices: LineIndices,
    pub visible: bool
}

#[derive(Debug)]
pub struct VisualChunk
{
    objects: ChunkSlice<Option<Object>>,
    occluders: ChunkSlice<Box<[OccluderCached]>>,
    light_occluder_base: ChunkSlice<Box<[OccluderInfo]>>,
    light_occluders: HashMap<usize, ChunkSlice<Box<[OccluderCached]>>>,
    vertical_occluders: ChunkSlice<Box<[SolidObject<SkyOccludingVertex>]>>,
    sky_lights: ChunkSlice<Option<SolidObject<SkyLightVertex>>>,
    light_generated: bool,
    sky: ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>,
    total_sky: ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>,
    draw_indices: ChunkSlice<Box<[usize]>>,
    draw_next: ChunkSlice<bool>,
    generated: bool
}

impl VisualChunk
{
    pub fn new() -> Self
    {
        Self{
            objects: Self::create_empty_slice(Option::default),
            occluders: Self::create_empty(),
            light_occluder_base: Self::create_empty(),
            light_occluders: HashMap::new(),
            vertical_occluders: Self::create_empty(),
            sky_lights: Self::create_empty_slice(Option::default),
            light_generated: false,
            sky: Self::create_empty_slice(|| [false; CHUNK_SIZE * CHUNK_SIZE]),
            total_sky: Self::create_empty_slice(|| [false; CHUNK_SIZE * CHUNK_SIZE]),
            draw_indices: Self::create_empty_slice(|| Box::from([])),
            draw_next: [false; CHUNK_SIZE],
            generated: false
        }
    }

    fn create_empty_slice<T>(f: impl FnMut() -> T) -> ChunkSlice<T>
    {
        iter::repeat_with(f)
            .take(CHUNK_SIZE)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_else(|_| unreachable!())
    }

    fn create_empty<T>() -> ChunkSlice<Box<[T]>>
    {
        Self::create_empty_slice(||
        {
            let b: Box<[T]> = Box::new([]);

            b
        })
    }

    pub fn create(
        tilemap: Arc<TileMap>,
        mut model_builder: ChunkModelBuilder,
        pos: GlobalPos,
        tiles: TileReader
    ) -> VisualChunkInfo
    {
        let occluders = Self::create_occluders(
            &tilemap,
            pos,
            &tiles
        );

        let mut occlusions = [[false; CHUNK_SIZE * CHUNK_SIZE]; CHUNK_SIZE];
        let mut is_drawable = [[false; CHUNK_SIZE * CHUNK_SIZE]; CHUNK_SIZE];

        for (z, (slice_occlusions, is_drawable)) in occlusions.iter_mut().zip(is_drawable.iter_mut()).enumerate()
        {
            for y in 0..CHUNK_SIZE
            {
                for x in 0..CHUNK_SIZE
                {
                    let pos = ChunkLocal::new(x, y, z);
                    let tiles = tiles.tile(pos);

                    let this_drawable = tilemap[tiles.this].drawable;

                    let occluded = Self::create_tile(
                        &tilemap,
                        &mut model_builder,
                        pos,
                        tiles
                    );

                    let index = y * CHUNK_SIZE + x;
                    slice_occlusions[index] = occluded;
                    is_drawable[index] = this_drawable;
                }
            }
        }

        let vertical_occluders = Self::create_vertical_occluders(&occlusions, pos);

        let infos = model_builder.build(pos);

        let (draw_next, draw_indices) = Self::from_occlusions(&occlusions, &is_drawable);

        let total_sky = occlusions.into_iter().rev().scan(occlusions[CHUNK_SIZE - 1], |state, occluded|
        {
            state.iter_mut().zip(occluded).for_each(|(state, value)|
            {
                *state |= value;
            });

            Some(*state)
        }).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().try_into().unwrap();

        VisualChunkInfo{
            infos,
            occluders,
            vertical_occluders,
            sky: occlusions,
            total_sky,
            draw_indices,
            draw_next
        }
    }

    pub fn build(
        tiles_factory: &mut TilesFactory,
        chunk_info: VisualChunkInfo
    ) -> Self
    {
        let objects = tiles_factory.build(chunk_info.infos);
        let occluders = tiles_factory.build_occluders(chunk_info.occluders.clone());
        let vertical_occluders = tiles_factory.build_vertical_occluders(chunk_info.vertical_occluders);

        Self{
            objects,
            occluders,
            light_occluder_base: chunk_info.occluders,
            light_occluders: HashMap::new(),
            vertical_occluders,
            generated: true,
            sky_lights: Self::create_empty_slice(Option::default),
            light_generated: false,
            sky: chunk_info.sky,
            total_sky: chunk_info.total_sky,
            draw_indices: chunk_info.draw_indices,
            draw_next: chunk_info.draw_next
        }
    }

    pub fn draw_next(&self, height: usize) -> bool
    {
        self.draw_next[height]
    }

    fn create_vertical_occluders(
        occlusions: &[[bool; CHUNK_SIZE * CHUNK_SIZE]; CHUNK_SIZE],
        pos: GlobalPos
    ) -> ChunkSlice<Box<[VerticalOccluder]>>
    {
        let chunk_position = Chunk::position_of_chunk(pos).xy();

        let occlusions: Vec<_> = (0..CHUNK_SIZE).rev()
            .scan([false; CHUNK_SIZE * CHUNK_SIZE], |state, z|
            {
                let mut occluders = Vec::new();

                let mut occlusion = occlusions[z];

                state.iter_mut().zip(occlusion.iter_mut()).for_each(|(top, bottom)|
                {
                    *top |= *bottom;
                    *bottom = *top;
                });

                while let Some(occluder) = Self::create_vertical_occluder(&mut occlusion)
                {
                    occluders.push(occluder.into_global(chunk_position));
                }

                Some(occluders.into_boxed_slice())
            }).collect::<Vec<_>>().into_iter()
                .rev()
                .collect();

        occlusions.try_into().unwrap()
    }

    fn create_vertical_occluder(
        occlusions: &mut [bool; CHUNK_SIZE * CHUNK_SIZE]
    ) -> Option<VerticalOccluderRaw>
    {
        let start_index = Self::vertical_occluder_start(occlusions)?;
        let start_point = Vector2::new(start_index % CHUNK_SIZE, start_index / CHUNK_SIZE);

        let width = occlusions[start_index..(start_index + (CHUNK_SIZE - start_point.x))].iter_mut()
            .take_while(|x|
            {
                **x
            })
            .map(|x: &mut bool|
            {
                *x = false;
            })
            .count();

        let height = (1..(CHUNK_SIZE - start_point.y)).take_while(|y|
        {
            let index = start_index + y * CHUNK_SIZE;
            let r = index..(index + width);

            let include = occlusions[r.clone()].iter().all(|x| *x);

            if include
            {
                occlusions[r].iter_mut().for_each(|x| *x = false);
            }

            include
        }).count() + 1;

        Some(VerticalOccluderRaw{
            position: start_point,
            size: Vector2::new(width, height)
        })
    }

    fn vertical_occluder_start(
        occlusions: &[bool; CHUNK_SIZE * CHUNK_SIZE]
    ) -> Option<usize>
    {
        occlusions.iter().enumerate().find_map(|(index, occluded)|
        {
            occluded.then_some(index)
        })
    }

    fn create_occluders(
        tilemap: &TileMap,
        pos: GlobalPos,
        tiles: &TileReader
    ) -> ChunkSlice<Box<[OccluderInfo]>>
    {
        let chunk_position = Chunk::position_of_chunk(pos);

        type ContainerType = FlatChunksContainer<Option<OccludingState>>;

        let add_horizontal = |plane: &mut ContainerType, pos: Pos3<usize>, inside|
        {
            if let Some(occluding) = plane[pos].as_mut()
            {
                occluding.horizontal = Some(inside);
            } else
            {
                plane[pos] = Some(OccludingState{horizontal: Some(inside), vertical: None})
            }
        };

        let add_vertical = |plane: &mut ContainerType, pos: Pos3<usize>, inside|
        {
            if let Some(occluding) = plane[pos].as_mut()
            {
                occluding.vertical = Some(inside);
            } else
            {
                plane[pos] = Some(OccludingState{horizontal: None, vertical: Some(inside)})
            }
        };

        (0..CHUNK_SIZE).map(|z|
        {
            let mut plane: ContainerType = FlatChunksContainer::new(Pos3::repeat(CHUNK_SIZE));

            for y in 0..CHUNK_SIZE
            {
                for x in 0..CHUNK_SIZE
                {
                    let pos = Pos3{x, y, z: 0};
                    let tile = tiles.tile(ChunkLocal::new(x, y, z));

                    let is_transparent = |tile|
                    {
                        tilemap.info(tile).transparent
                    };

                    let this_transparent = is_transparent(tile.this);
                    if tile.other.left.map(|x| this_transparent ^ is_transparent(x)).unwrap_or(false)
                    {
                        add_vertical(&mut plane, pos, !this_transparent);
                    }

                    if tile.other.down.map(|x| this_transparent ^ is_transparent(x)).unwrap_or(false)
                    {
                        add_horizontal(&mut plane, pos, !this_transparent);
                    }
                }
            }

            Self::simplify_occluders(plane).map(|raw|
            {
                raw.into_global(chunk_position, z)
            }).collect::<Box<[_]>>()
        }).collect::<Vec<_>>().try_into().unwrap_or_else(|_| unreachable!())
    }

    fn simplify_occluders(
        mut occluders: FlatChunksContainer<Option<OccludingState>>
    ) -> impl Iterator<Item=OccluderInfoRaw>
    {
        let to_pos = |pos: LocalPos|
        {
            Vector3::<usize>::from(pos.pos).xy()
        };

        occluders.positions().flat_map(move |pos|
        {
            if let Some(occluding) = occluders[pos]
            {
                let mut occluder = |horizontal, inside|
                {
                    let positions = if horizontal
                    {
                        pos.pos.x..pos.size.x
                    } else
                    {
                        pos.pos.y..pos.size.y
                    };

                    let length = positions.map(|value|
                    {
                        let mut position = pos.pos;

                        if horizontal
                        {
                            position.x = value;
                        } else
                        {
                            position.y = value;
                        }

                        position
                    }).take_while(|&position|
                    {
                        if let Some(occluding) = &mut occluders[position]
                        {
                            let is_both = occluding.horizontal.is_some() && occluding.vertical.is_some();

                            if horizontal && occluding.horizontal.is_some()
                            {
                                if is_both
                                {
                                    occluding.horizontal = None;
                                } else
                                {
                                    occluders[position] = None;
                                }

                                true
                            } else if !horizontal && occluding.vertical.is_some()
                            {
                                if is_both
                                {
                                    occluding.vertical = None;
                                } else
                                {
                                    occluders[position] = None;
                                }

                                true
                            } else
                            {
                                false
                            }
                        } else
                        {
                            false
                        }
                    }).count();

                    OccluderInfoRaw{
                        position: to_pos(pos),
                        inside,
                        horizontal,
                        length
                    }
                };

                let horizontal = occluding.horizontal.map(|inside| occluder(true, inside));
                let vertical = occluding.vertical.map(|inside| occluder(false, inside));
                match (horizontal, vertical)
                {
                    (Some(a), Some(b)) => vec![a, b],
                    (Some(a), None) => vec![a],
                    (None, Some(b)) => vec![b],
                    (None, None) => vec![]
                }
            } else
            {
                vec![]
            }
        })
    }

    fn from_occlusions(
        occlusions: &ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>,
        is_drawable: &ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> (ChunkSlice<bool>, ChunkSlice<Box<[usize]>>)
    {
        let (next, indices): (Vec<_>, Vec<_>) = (0..CHUNK_SIZE).map(|index|
        {
            let draw_next = occlusions[0..index].iter().rev().try_fold(occlusions[index], |current, occlusions|
            {
                let combined_occlusions: [bool; CHUNK_SIZE * CHUNK_SIZE] =
                    current.into_iter().zip(occlusions.iter().copied()).map(|(current, x)|
                    {
                        current || x
                    }).collect::<Vec<_>>().try_into().unwrap();

                let fully_occluded = combined_occlusions.iter().copied().all(convert::identity);

                if fully_occluded
                {
                    ControlFlow::Break(())
                } else
                {
                    ControlFlow::Continue(combined_occlusions)
                }
            }).continue_value().map(|xs| !xs.into_iter().all(convert::identity)).unwrap_or(false);

            let indices = iter::once(index).chain(occlusions[0..index].iter().zip(is_drawable[0..index].iter()).enumerate().rev()
                .scan(occlusions[index], |current, (index, (occlusions, is_drawable))|
                {
                    let mut changed = false;
                    *current = current.iter().copied().zip(occlusions.iter().copied().zip(is_drawable.iter().copied()))
                        .map(|(current, (x, is_drawable))|
                        {
                            if !current
                            {
                                if is_drawable { changed = true; }

                                x
                            } else
                            {
                                true
                            }
                        }).collect::<Vec<_>>().try_into().unwrap();

                    Some(changed.then_some(index))
                }).flatten()).collect::<Vec<_>>().into_iter().rev().collect();

            (draw_next, indices)
        }).unzip();

        (next.try_into().unwrap(), indices.try_into().unwrap())
    }

    fn create_tile(
        tilemap: &TileMap,
        model_builder: &mut ChunkModelBuilder,
        pos: ChunkLocal,
        tiles: MaybeGroup<Tile>
    ) -> bool
    {
        if !tilemap[tiles.this].drawable
        {
            return false;
        }

        model_builder.create(pos, tiles.this.0.unwrap());

        #[allow(clippy::let_and_return)]
        let occluding = !tilemap[tiles.this].transparent;

        occluding
    }

    pub fn is_fully_generated(&self) -> bool
    {
        self.generated && self.light_generated
    }

    pub fn is_light_generated(&self) -> bool
    {
        self.light_generated
    }

    pub fn mark_light_generating(&mut self)
    {
        self.light_generated = true;
    }

    pub fn is_generated(&self) -> bool
    {
        self.generated
    }

    pub fn mark_generating(&mut self)
    {
        self.generated = true;
    }

    pub fn mark_ungenerated(&mut self)
    {
        self.generated = false;
        self.light_generated = false;
    }

    pub fn sky_occluded_between(
        &self,
        mut heights: impl Iterator<Item=usize>,
        top_left: Vector2<usize>,
        bottom_right: Vector2<usize>
    ) -> bool
    {
        heights.any(|z|
        {
            let sky = &self.sky[z];
            (top_left.y..=bottom_right.y).all(|y|
            {
                let index = y * CHUNK_SIZE;
                (top_left.x..=bottom_right.x).all(move |x|
                {
                    let index = index + x;

                    sky[index]
                })
            })
        })
    }

    pub fn sky_occluded(
        &self,
        height: usize,
        top_left: Vector2<usize>,
        bottom_right: Vector2<usize>
    ) -> bool
    {
        let total_sky = &self.total_sky[height];
        (top_left.y..=bottom_right.y).all(|y|
        {
            let index = y * CHUNK_SIZE;
            (top_left.x..=bottom_right.x).all(move |x|
            {
                let index = index + x;

                total_sky[index]
            })
        })
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        height: usize
    )
    {
        self.draw_indices[height].iter().copied().for_each(|index|
        {
            if let Some(object) = self.objects[index].as_mut()
            {
                object.update_buffers(info);
            }
        });
    }

    pub fn update_sky_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        height: usize
    )
    {
        self.sky_lights[height].iter_mut().for_each(|x|
        {
            x.update_buffers(info)
        });

        self.vertical_occluders[height].iter_mut().for_each(|x|
        {
            x.update_buffers(info)
        });
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        self.draw_indices[height].iter().copied().for_each(|index|
        {
            if let Some(object) = self.objects[index].as_ref()
            {
                object.draw(info);
            }
        });
    }

    pub fn set_occluder_visible(&mut self, height: usize, index: usize, value: bool)
    {
        self.occluders[height][index].visible = value;
    }

    fn update_buffers_shadows_with(
        occluders: &mut ChunkSlice<Box<[OccluderCached]>>,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        height: usize,
        f: &mut impl FnMut(&OccluderCached, usize)
    )
    {
        occluders[height].iter_mut().enumerate().for_each(|(index, x)|
        {
            if x.occluder.visible(visibility)
            {
                x.occluder.update_buffers(info, caster);

                let visible = x.occluder.is_visible();
                x.visible = visible;

                if visible
                {
                    f(&x, index);
                }
            } else
            {
                x.visible = false;
            }
        });
    }

    pub fn update_buffers_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        height: usize,
        mut f: impl FnMut(&OccluderCached, usize)
    )
    {
        Self::update_buffers_shadows_with(&mut self.occluders, info, visibility, caster, height, &mut f)
    }

    pub fn update_buffers_light_shadows(
        &mut self,
        info: &mut UpdateBuffersInfo,
        tiles_factory: &mut TilesFactory,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        height: usize,
        id: usize
    )
    {
        let occluders = self.light_occluders.entry(id)
            .or_insert_with(|| tiles_factory.build_occluders(self.light_occluder_base.clone()));

        Self::update_buffers_shadows_with(
            occluders,
            info,
            visibility,
            caster,
            height,
            &mut |_, _| { }
        );
    }

    fn draw_shadows_with(
        occluders: &ChunkSlice<Box<[OccluderCached]>>,
        info: &mut DrawInfo,
        height: usize,
        f: &mut Option<impl FnOnce(&mut DrawInfo)>
    )
    {
        if DebugConfig::is_enabled(DebugTool::NoWallOcclusion)
        {
            return;
        }

        occluders[height].iter().for_each(|x|
        {
            if x.visible
            {
                if let Some(f) = f.take()
                {
                    f(info);
                }

                x.occluder.draw(info)
            }
        });
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        Self::draw_shadows_with(&self.occluders, info, height, &mut None::<fn(&mut DrawInfo)>);
    }

    pub fn draw_light_shadows(
        &self,
        info: &mut DrawInfo,
        height: usize,
        id: usize,
        f: &mut Option<impl FnOnce(&mut DrawInfo)>
    )
    {
        Self::draw_shadows_with(&self.light_occluders[&id], info, height, f);
    }

    pub fn draw_sky_shadows(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        if DebugConfig::is_enabled(DebugTool::NoSkyOcclusion)
        {
            return;
        }

        self.vertical_occluders[height].iter().for_each(|x|
        {
            x.draw(info)
        });
    }

    pub fn draw_sky_lights(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        self.sky_lights[height].iter().for_each(|x|
        {
            x.draw(info)
        });
    }
}
