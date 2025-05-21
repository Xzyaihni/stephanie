use std::{
    iter,
    convert,
    ops::RangeInclusive,
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
    horizontal: bool,
    vertical: bool
}

struct OccluderInfoRaw
{
    position: Vector2<usize>,
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

        OccluderInfo{
            position: chunk_position + tile_position,
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
    fn into_global(self, chunk_position: Vector3<f32>, z: usize) -> VerticalOccluder
    {
        let tile_position = Vector3::new(self.position.x, self.position.y, z).cast() * TILE_SIZE;

        // a little padding to hide seams
        let padding = TILE_SIZE * 0.01;

        let size = Vector3::new(self.size.x, self.size.y, 1).cast() * TILE_SIZE;
        let half_size = Vector3::new(size.x, size.y, 0.0) / 2.0;

        VerticalOccluder{
            position: chunk_position + tile_position + half_size,
            size: size + Vector3::repeat(padding)
        }
    }
}

pub struct VisualChunkInfo
{
    infos: ChunkSlice<Option<ChunkInfo>>,
    occluders: ChunkSlice<Box<[OccluderInfo]>>,
    vertical_occluders: ChunkSlice<Box<[VerticalOccluder]>>,
    draw_height: ChunkSlice<usize>,
    draw_next: ChunkSlice<bool>
}

#[derive(Debug)]
pub struct VisualChunk
{
    objects: ChunkSlice<Option<Object>>,
    occluders: ChunkSlice<Box<[OccludingPlane]>>,
    vertical_occluders: ChunkSlice<Box<[SolidObject]>>,
    draw_height: ChunkSlice<usize>,
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
            vertical_occluders: Self::create_empty(),
            draw_height: [0; CHUNK_SIZE],
            draw_next: [true; CHUNK_SIZE],
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

        for (z, slice_occlusions) in occlusions.iter_mut().enumerate()
        {
            for y in 0..CHUNK_SIZE
            {
                for x in 0..CHUNK_SIZE
                {
                    let pos = ChunkLocal::new(x, y, z);
                    let tiles = tiles.tile(pos);

                    let occluded = Self::create_tile(
                        &tilemap,
                        &mut model_builder,
                        pos,
                        tiles
                    );

                    slice_occlusions[y * CHUNK_SIZE + x] = occluded;
                }
            }
        }

        let vertical_occluders = Self::create_vertical_occluders(&occlusions, pos);

        let infos = model_builder.build(pos);

        let (draw_next, draw_height) = Self::from_occlusions(&occlusions);

        VisualChunkInfo{
            infos,
            occluders,
            vertical_occluders,
            draw_height,
            draw_next
        }
    }

    pub fn build(
        tiles_factory: &mut TilesFactory,
        chunk_info: VisualChunkInfo
    ) -> Self
    {
        let objects = tiles_factory.build(chunk_info.infos);
        let occluders = tiles_factory.build_occluders(chunk_info.occluders);
        let vertical_occluders = tiles_factory.build_vertical_occluders(chunk_info.vertical_occluders);

        Self{
            objects,
            occluders,
            vertical_occluders,
            generated: true,
            draw_height: chunk_info.draw_height,
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
        let chunk_position = Chunk::position_of_chunk(pos);

        (0..CHUNK_SIZE).scan([false; CHUNK_SIZE * CHUNK_SIZE], |state, z|
        {
            let mut occluders = Vec::new();

            state.iter_mut().zip(occlusions[z]).for_each(|(top, bottom)|
            {
                *top = !*top && bottom;
            });

            while let Some(occluder) = Self::create_vertical_occluder(state)
            {
                occluders.push(occluder.into_global(chunk_position, z));
            }

            Some(occluders.into_boxed_slice())
        }).collect::<Vec<_>>().try_into().unwrap()
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

        let height = (1..(CHUNK_SIZE - start_point.y - 1)).take_while(|y|
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

        let add_horizontal = |plane: &mut ContainerType, pos: Pos3<usize>|
        {
            if let Some(occluding) = plane[pos].as_mut()
            {
                occluding.horizontal = true;
            } else
            {
                plane[pos] = Some(OccludingState{horizontal: true, vertical: false})
            }
        };

        let add_vertical = |plane: &mut ContainerType, pos: Pos3<usize>|
        {
            if let Some(occluding) = plane[pos].as_mut()
            {
                occluding.vertical = true;
            } else
            {
                plane[pos] = Some(OccludingState{horizontal: false, vertical: true})
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

                    if is_transparent(tile.this)
                    {
                        if let Some(left) = tile.other.left
                        {
                            if !is_transparent(left)
                            {
                                add_vertical(&mut plane, pos);
                            }
                        }

                        if let Some(down) = tile.other.down
                        {
                            if !is_transparent(down)
                            {
                                add_horizontal(&mut plane, pos);
                            }
                        }
                    } else
                    {
                        if let Some(left) = tile.other.left
                        {
                            if is_transparent(left)
                            {
                                add_vertical(&mut plane, pos);
                            }
                        }

                        if let Some(down) = tile.other.down
                        {
                            if is_transparent(down)
                            {
                                add_horizontal(&mut plane, pos);
                            }
                        }
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
                let mut occluder = |horizontal|
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
                            let is_both = occluding.horizontal && occluding.vertical;

                            if horizontal && occluding.horizontal
                            {
                                if is_both
                                {
                                    occluding.horizontal = false;
                                } else
                                {
                                    occluders[position] = None;
                                }

                                true
                            } else if !horizontal && occluding.vertical
                            {
                                if is_both
                                {
                                    occluding.vertical = false;
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
                        horizontal,
                        length
                    }
                };

                let horizontal = occluding.horizontal.then(|| occluder(true));
                let vertical = occluding.vertical.then(|| occluder(false));
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
        occlusions: &ChunkSlice<[bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> (ChunkSlice<bool>, ChunkSlice<usize>)
    {
        let (next, height): (Vec<_>, Vec<_>) = (0..CHUNK_SIZE).map(|index|
        {
            let amount = Self::unoccluded_amount(occlusions[0..=index].iter().rev());

            let draw_next = amount > (index + 1);

            (draw_next, amount.min(index + 1))
        }).unzip();

        (next.try_into().unwrap(), height.try_into().unwrap())
    }

    fn unoccluded_amount<'a>(
        mut occlusions: impl Iterator<Item=&'a [bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> usize
    {
        let mut current = if let Some(x) = occlusions.next()
        {
            x.to_vec()
        } else
        {
            return 0;
        };

        Self::unoccluded_amount_inner(&mut current, &mut occlusions)
    }

    fn unoccluded_amount_inner<'a>(
        current: &mut Vec<bool>,
        occlusions: &mut impl Iterator<Item=&'a [bool; CHUNK_SIZE * CHUNK_SIZE]>
    ) -> usize
    {
        let fully_occluded = current.iter().copied().all(convert::identity);

        if fully_occluded
        {
            1
        } else if let Some(occlusion) = occlusions.next()
        {
            *current = current.iter().zip(occlusion.iter()).map(|(a, b)|
            {
                *a || *b
            }).collect();

            1 + Self::unoccluded_amount_inner(current, occlusions)
        } else
        {
            2
        }
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
    }

    fn draw_range(&self, height: usize) -> RangeInclusive<usize>
    {
        let draw_amount = self.draw_height[height];

        (height + 1 - draw_amount)..=height
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        visibility: &VisibilityChecker,
        caster: &OccludingCaster,
        height: usize
    )
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range.clone()].iter_mut().for_each(|objects|
        {
            if let Some(object) = objects.as_mut()
            {
                object.update_buffers(info);
            }
        });

        self.occluders[height].iter_mut().for_each(|x|
        {
            if x.visible(visibility)
            {
                x.update_buffers(info, caster)
            }
        });
    }

    pub fn update_sky_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo,
        height: Option<usize>
    )
    {
        let start = height.map(|height| height + 1).unwrap_or(0);
        self.vertical_occluders[start..].iter_mut().for_each(|x|
        {
            x.iter_mut().for_each(|x| x.update_buffers(info));
        });
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range].iter().filter_map(|x| x.as_ref()).for_each(|object|
        {
            object.draw(info);
        });
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        height: usize
    )
    {
        self.occluders[height].iter().for_each(|x|
        {
            if x.visible(visibility)
            {
                x.draw(info)
            }
        });
    }

    pub fn draw_sky_shadows(
        &self,
        info: &mut DrawInfo,
        height: Option<usize>
    )
    {
        if DebugConfig::is_enabled(DebugTool::NoSkyOcclusion)
        {
            return;
        }

        let start = height.map(|height| height + 1).unwrap_or(0);
        self.vertical_occluders[start..].iter().for_each(|x|
        {
            x.iter().for_each(|x| x.draw(info));
        });
    }
}
