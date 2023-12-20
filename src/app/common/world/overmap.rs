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

	fn generate_missing(&mut self);

	fn position_offset(&mut self, offset: GlobalPos)
	{
		self.shift_chunks(offset);
		self.generate_missing();
	}

	fn shift_chunks(&mut self, offset: GlobalPos)
	{
		let size = self.size();

		let maybe_reverse = |reverse, value, max| if reverse {max - value - 1} else {value};
		for z in 0..size.z
		{
			let z = maybe_reverse(offset.0.z < 0, z, size.z);

			for y in 0..size.y
			{
				let y = maybe_reverse(offset.0.y < 0, y, size.y);

				for x in 0..size.x
				{
					let x = maybe_reverse(offset.0.x < 0, x, size.x);

					let old_local = LocalPos::new(Pos3::new(x, y, z), size);
					self.shift_chunk(offset, old_local);
				}
			}
		}
	}

	fn shift_chunk(&mut self, offset: GlobalPos, old_local: LocalPos)
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

			let is_edge_chunk =
			{
				let is_edge = |pos, offset, limit|
				{
                    // wut r u smoking clippy?
                    #[allow(clippy::comparison_chain)]
					if offset == 0
					{
						false
					} else if offset < 0
					{
						(pos as i32 + offset) == 0
					} else
					{
						(pos as i32 + offset) == (limit as i32 - 1)
					}
				};

				let size = self.size();
				let x_edge = is_edge(local_pos.pos.x, offset.0.x, size.x);
				let y_edge = is_edge(local_pos.pos.y, offset.0.y, size.y);
				let z_edge = is_edge(local_pos.pos.z, offset.0.z, size.z);

				x_edge || y_edge || z_edge
			};

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

    fn over_bounds(&self, pos: GlobalPos, margin: Pos3<i32>) -> GlobalPos
    {
        self.over_bounds_with_padding(pos, margin, Pos3::repeat(0))
    }

    fn over_bounds_with_padding(
        &self,
        pos: GlobalPos,
        margin: Pos3<i32>,
        padding: Pos3<i32>
    ) -> GlobalPos
    {
        let pos = self.to_local_unconverted(pos).0;

        let size = self.size();

        let over_bounds = |value, limit, margin, padding| -> i32
        {
            let value_difference = value - padding;
            let limit_difference = value + padding - limit as i32 + 1;

            if value_difference < 0
            {
                // under lower bound
                value_difference - margin
            } else if limit_difference > 0
            {
                // above upper bound
                limit_difference + margin
            } else
            {
                0
            }
        };

        GlobalPos::new(
            over_bounds(pos.x, size.x, margin.x, padding.x),
            over_bounds(pos.y, size.y, margin.y, padding.y),
            over_bounds(pos.z, size.z, margin.z, padding.z)
        )
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
            Pos3::new(3, 4, 5)
        }

        fn player_position(&self) -> GlobalPos
        {
            self.0
        }
    }

    #[test]
    fn over_bounds()
    {
        let overmap = TestOvermap(GlobalPos::new(1, 2, 2));

        let test = GlobalPos::new(1, 2, -5);
        assert_eq!(test, overmap.to_local_unconverted(test));

        assert_eq!(
            GlobalPos::new(2, -2, 3),
            overmap.over_bounds_with_padding(
                GlobalPos::new(2, -1, 4),
                Pos3::new(1, 1, 1),
                Pos3::new(1, 0, 2)
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
