use std::{
	io,
	sync::Mutex,
	fs::{self, File},
	marker::PhantomData
};

use serde::{Serialize, de::DeserializeOwned};

use crate::common::{
	world::{
		GlobalPos
	}
};


#[derive(Debug)]
pub struct ChunkSaver<T: Serialize + DeserializeOwned>
{
	parent_path: String,
	file_access: Mutex<()>,
	//i still dont get variance (does it need to be invariant or not??)
	phantom: PhantomData<fn() -> T>
}

impl<T: Serialize + DeserializeOwned> ChunkSaver<T>
{
	pub fn new(parent_path: String) -> Self
	{
		fs::create_dir_all(&parent_path).unwrap();

		Self{parent_path, file_access: Mutex::new(()), phantom: PhantomData}
	}

	pub fn load(&self, pos: GlobalPos) -> Option<T>
	{
		let _lock = self.file_access.lock();

		match File::open(self.chunk_path(pos))
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
		}
	}

	pub fn save(&self, pos: GlobalPos, chunk: &T)
	{
		let _lock = self.file_access.lock();

		let file = File::create(self.chunk_path(pos)).unwrap();

		bincode::serialize_into(file, chunk).unwrap();
	}

	fn chunk_path(&self, pos: GlobalPos) -> String
	{
		let parent_path = &self.parent_path;
		format!("{parent_path}/{}", Self::encode_position(pos))
	}

	fn encode_position(pos: GlobalPos) -> String
	{
		format!("{}_{}_{}", pos.0.x, pos.0.y, pos.0.z)
	}
}