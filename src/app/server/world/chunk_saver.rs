use std::{
	io,
    rc::Rc,
    cmp::Ordering,
    time::Duration,
    path::PathBuf,
	fs::{self, File},
    collections::BinaryHeap
};

use serde::{Serialize, de::DeserializeOwned};

use crate::common::world::{GlobalPos, Pos3};


pub trait Saver<T>
{
	fn save(&mut self, pos: GlobalPos, chunk: T);
	fn load(&self, pos: GlobalPos) -> Option<T>;
}

#[derive(Debug)]
struct InnerValue<T>
{
    // parent_path: Rc<PathBuf>,
    pub pos: GlobalPos,
    pub value: T
}

impl<T: Serialize> InnerValue<T>
{
    pub fn save(self)
    {
		let file = File::create(self.chunk_path(self.pos)).unwrap();

		bincode::serialize_into(file, &self.value).unwrap();
    }

	fn chunk_path(&self, pos: GlobalPos) -> PathBuf
	{
        todo!();
        // self.parent_path.join(Self::encode_position(pos))
	}

    fn encode_position(pos: GlobalPos) -> String
    {
        let GlobalPos(Pos3{x, y, z}) = pos;

        format!("{x}_{y}_{z}")
    }
}

#[derive(Debug)]
struct CachedValue<T>
{
    age: Duration,
    pub value: InnerValue<T>
}

impl<T> PartialEq for CachedValue<T>
{
    fn eq(&self, other: &Self) -> bool
    {
        self.age.eq(&other.age)
    }
}

impl<T> Eq for CachedValue<T> {}

impl<T> PartialOrd for CachedValue<T>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        self.age.partial_cmp(&other.age)
    }
}

impl<T> Ord for CachedValue<T>
{
    fn cmp(&self, other: &Self) -> Ordering
    {
        self.age.cmp(&other.age)
    }
}

#[derive(Debug)]
pub struct ChunkSaver<T: Serialize + DeserializeOwned>
{
	parent_path: PathBuf,
    dropping_vec: Vec<InnerValue<T>>,
    cache_amount: usize,
    cache: BinaryHeap<CachedValue<T>>
}

impl<T: Serialize + DeserializeOwned> ChunkSaver<T>
{
	pub fn new(parent_path: impl Into<PathBuf>, cache_amount: usize) -> Self
	{
        let parent_path = parent_path.into();

		fs::create_dir_all(&parent_path).unwrap();

		Self{parent_path, dropping_vec: Vec::new(), cache_amount, cache: BinaryHeap::new()}
	}
}

impl<T: Serialize + DeserializeOwned> Saver<T> for ChunkSaver<T>
{
	fn load(&self, pos: GlobalPos) -> Option<T>
	{
		/*match File::open(self.chunk_path(pos))
		{
			Ok(file) =>
			{
				Some(bincode::deserialize_from(file).unwrap())
			},
			Err(ref err) if err.kind() == io::ErrorKind::NotFound =>
			{
				None
			},
			Err(err) => panic!("error loading chunk from file: {:?}", err)
		}*/
        todo!();
	}

	fn save(&mut self, pos: GlobalPos, chunk: T)
	{
        todo!();
	}
}
