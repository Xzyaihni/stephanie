use chunk::{
	Pos3,
	GlobalPos,
	LocalPos
};

pub use chunks_container::{ChunkIndexing, ChunksContainer, FlatChunksContainer};

pub mod chunk;
pub mod visual_chunk;

pub mod chunks_container;


pub trait Overmap<T>: OvermapIndexing
{
	fn remove(&mut self, pos: LocalPos);

	fn swap(&mut self, a: LocalPos, b: LocalPos);

	fn get_local(&self, pos: LocalPos) -> &Option<T>;

	fn mark_ungenerated(&mut self, pos: LocalPos);

	fn get(&self, pos: GlobalPos) -> Option<&T>
	{
		self.to_local(pos).and_then(|local_pos| self.get_local(local_pos).as_ref())
	}

    fn contains(&self, pos: GlobalPos) -> bool
    {
        self.get(pos).is_some()
    }

    fn inbounds(&self, pos: GlobalPos) -> bool
    {
        self.to_local(pos).is_some()
    }

	fn generate_missing(&mut self);

	fn force_regenerate(&mut self)
    {
        let size = self.size();

        for z in 0..size.z
        {
            for y in 0..size.y
            {
                for x in 0..size.x
                {
                    self.remove(LocalPos::new(Pos3{x, y, z}, size));
                }
            }
        }

        self.generate_missing();
    }

	fn position_offset(&mut self, offset: Pos3<i32>)
	{
		self.shift_chunks(offset);
		self.generate_missing();
	}

	fn shift_chunks(&mut self, offset: Pos3<i32>)
	{
		let size = self.size();

		let maybe_reverse = |reverse, value, max| if reverse {max - value - 1} else {value};
		for z in 0..size.z
		{
			let z = maybe_reverse(offset.z < 0, z, size.z);

			for y in 0..size.y
			{
				let y = maybe_reverse(offset.y < 0, y, size.y);

				for x in 0..size.x
				{
					let x = maybe_reverse(offset.x < 0, x, size.x);

					let old_local = LocalPos::new(Pos3::new(x, y, z), size);
					self.shift_chunk(offset, old_local);
				}
			}
		}
	}

	fn shift_chunk(&mut self, offset: Pos3<i32>, old_local: LocalPos)
	{
		//early return if the chunk is empty
		if self.get_local(old_local).is_none()
		{
			return;
		}

		let old_position = self.to_global(old_local);
		let position = old_position - offset;

		if let Some(local_pos) = self.to_local(position)
		{
			//move the chunk to the new position
			self.swap(old_local, local_pos);

			let is_edge_chunk = old_local.pos.zip(self.size()).any(|(pos, size)|
            {
                pos == 0 || pos == (size - 1)
            });

			if is_edge_chunk
			{
				self.mark_ungenerated(local_pos);
			}
		} else
		{
			//chunk now outside the player range, remove it
			self.remove(old_local);
		}
	}
}

pub trait OvermapIndexing
{
	fn size(&self) -> Pos3<usize>;
	fn player_position(&self) -> GlobalPos;

	fn default_ordering(
		&self,
		positions: impl Iterator<Item=LocalPos>
	) -> Box<[LocalPos]>
	{
		let mut ordering = positions.collect::<Vec<_>>();

		ordering.sort_unstable_by(move |a, b|
		{
			let distance = |local_pos| -> f32
			{
				let GlobalPos(pos) = self.player_offset(local_pos);

				((pos.x.pow(2) + pos.y.pow(2) + pos.z.pow(2)) as f32).sqrt()
			};

			distance(*a).total_cmp(&distance(*b))
		});

		ordering.into_boxed_slice()
	}

	fn to_local(&self, pos: GlobalPos) -> Option<LocalPos>
	{
		let pos = self.to_local_unconverted(pos);

		LocalPos::from_global(pos, self.size())
	}

	fn to_local_unconverted(&self, pos: GlobalPos) -> GlobalPos
	{
		let player_distance = pos - self.player_position();

		player_distance + GlobalPos::from(self.size()) / 2
	}

	fn to_global(&self, pos: LocalPos) -> GlobalPos
	{
		self.player_offset(pos) + self.player_position()
	}

    fn to_global_z(&self, z: usize) -> i32
    {
        (z as i32 - self.size().z as i32 / 2) + self.player_position().0.z
    }

    fn over_bounds(&self, pos: GlobalPos, margin: Pos3<i32>) -> Pos3<i32>
    {
        self.over_bounds_with_padding(pos, margin, Pos3::repeat(0))
    }

    fn over_bounds_with_padding(
        &self,
        pos: GlobalPos,
        margin: Pos3<i32>,
        padding: Pos3<i32>
    ) -> Pos3<i32>
    {
        let values = self.to_local_unconverted(pos).0.zip(self.size()).zip(margin).zip(padding);

        values.map(|(((value, limit), margin), padding)| -> i32
        {
            let lower_diff = value - padding;
            let upper_diff = value - (limit as i32 - 1 - padding);

            if lower_diff < 0
            {
                // under lower bound
                lower_diff - margin
            } else if upper_diff > 0
            {
                // above upper bound
                upper_diff + margin
            } else
            {
                0
            }
        })
    }

	fn player_offset(&self, pos: LocalPos) -> GlobalPos
	{
		GlobalPos::from(pos) - GlobalPos::from(self.size()) / 2
	}
}

#[cfg(test)]
mod tests
{
    use super::*;

    struct TestOvermap(GlobalPos);

    impl OvermapIndexing for TestOvermap
    {
        fn size(&self) -> Pos3<usize>
        {
            Pos3::new(9, 4, 2)
        }

        fn player_position(&self) -> GlobalPos
        {
            self.0
        }
    }

    #[test]
    fn over_bounds()
    {
        let overmap = TestOvermap(GlobalPos::new(-8, 2, -10));

        let test = GlobalPos::new(6, -8, 0);
        assert_eq!(GlobalPos::new(18, -8, 11), overmap.to_local_unconverted(test));

        // how much over/under:
        // (13, -9, 11)

        assert_eq!(
            Pos3::new(14, -11, 12),
            overmap.over_bounds_with_padding(
                test,
                Pos3::new(1, 2, 1), // margin
                Pos3::new(3, 1, 1) // padding
            )
        )
    }

    #[test]
    fn local_global_inverse()
    {
        for _ in 0..5
        {
            let overmap = TestOvermap(GlobalPos::new(
                fastrand::i32(0..10) - 5,
                fastrand::i32(0..10) - 5,
                fastrand::i32(0..10) - 5
            ));

            let size = overmap.size();
            let value = LocalPos::new(
                Pos3::new(
                    fastrand::usize(0..size.x),
                    fastrand::usize(0..size.y),
                    fastrand::usize(0..size.z)
                ),
                size
            );

            assert_eq!(
                value, overmap.to_local(overmap.to_global(value)).unwrap_or_else(||
                {
                    panic!(
                        "size: {size:?}, value: {value:?}, player_position: {:?}",
                        overmap.player_position()
                    );
                }),
                "size: {size:?}, value: {value:?}, player_position: {:?}",
                overmap.player_position()
            );
        }
    }
}
