use std::{
    iter,
    convert,
    ops::RangeInclusive,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{
    Object,
    game_object::*
};

use crate::{
    client::{
        VisibilityChecker,
        tiles_factory::{
            ChunkSlice,
            ChunkObjects,
            TilesFactory,
            OccluderInfo,
            ChunkInfo,
            ChunkModelBuilder
        }
    },
    common::{
        OccludingPlane,
        OccludingCasters,
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
            PosDirection,
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

pub struct VisualChunkInfo
{
    infos: ChunkSlice<ChunkObjects<Option<ChunkInfo>>>,
    occluders: ChunkSlice<Box<[OccluderInfo]>>,
    draw_height: ChunkSlice<usize>,
    draw_next: ChunkSlice<bool>
}

pub struct VisualChunk
{
    objects: ChunkSlice<ChunkObjects<Option<Object>>>,
    occluders: ChunkSlice<Box<[OccludingPlane]>>,
    draw_height: ChunkSlice<usize>,
    draw_next: ChunkSlice<bool>,
    generated: bool
}

impl VisualChunk
{
    pub fn new() -> Self
    {
        Self{
            objects: Self::create_empty_slice(|| ChunkObjects::repeat_with(|| None)),
            occluders: Self::create_empty(),
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

        let infos = model_builder.build(pos);

        let (draw_next, draw_height) = Self::from_occlusions(&occlusions);

        VisualChunkInfo{
            infos,
            occluders,
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

        Self{
            objects,
            occluders,
            generated: true,
            draw_height: chunk_info.draw_height,
            draw_next: chunk_info.draw_next
        }
    }

    pub fn draw_next(&self, height: usize) -> bool
    {
        self.draw_next[height]
    }

    fn create_occluders(
        tilemap: &TileMap,
        pos: GlobalPos,
        tiles: &TileReader
    ) -> ChunkSlice<Box<[OccluderInfo]>>
    {
        let chunk_position = Chunk::transform_of_chunk(pos).position;

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

            Self::simplify_occluders(plane).map(|info: OccluderInfoRaw|
            {
                let mut tile_position = Vector3::new(info.position.x, info.position.y, z).cast();

                if info.horizontal
                {
                    tile_position.x += info.length as f32 * 0.5;
                } else
                {
                    tile_position.y += info.length as f32 * 0.5;
                }

                let tile_position = tile_position * TILE_SIZE;

                // a little padding to hide seams
                let padding = TILE_SIZE * 0.01;

                OccluderInfo{
                    position: chunk_position + tile_position,
                    horizontal: info.horizontal,
                    length: info.length as f32 * TILE_SIZE + padding
                }
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
        if tiles.this.is_none()
        {
            return false;
        }

        model_builder.create(pos, tiles.this);

        PosDirection::iter_non_z().for_each(|direction|
        {
            if let Some(gradient_tile) = tiles[direction]
            {
                if !tilemap[gradient_tile].transparent && gradient_tile != tiles.this
                {
                    model_builder.create_direction(
                        direction,
                        pos,
                        gradient_tile
                    );
                }
            }
        });

        #[allow(clippy::let_and_return)]
        let occluding = !tilemap[tiles.this].transparent;

        occluding
    }

    pub fn is_generated(&self) -> bool
    {
        self.generated
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
        casters: &OccludingCasters,
        height: usize
    )
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range.clone()].iter_mut().for_each(|objects|
        {
            objects.iter_mut().filter_map(Option::as_mut)
                .for_each(|object| object.update_buffers(info));
        });

        self.occluders[draw_range].iter_mut().for_each(|occluders|
        {
            occluders.iter_mut().for_each(|x| x.update_buffers(visibility, info, casters));
        });
    }

    pub fn draw_tiles(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range].iter().filter_map(|x| x.normal.as_ref()).for_each(|object|
        {
            object.draw(info);
        });
    }

    pub fn draw_gradients(
        &self,
        info: &mut DrawInfo,
        height: usize
    )
    {
        let draw_range = self.draw_range(height);

        self.objects[draw_range].iter().for_each(|objects|
        {
            objects.gradients.iter().flatten().for_each(|object| object.draw(info));
        });
    }

    pub fn draw_shadows(
        &self,
        info: &mut DrawInfo,
        visibility: &VisibilityChecker,
        height: usize
    )
    {
        let draw_range = self.draw_range(height);

        self.occluders[draw_range].iter().for_each(|occluders|
        {
            occluders.iter().for_each(|x| x.draw(visibility, info));
        });
    }
}
