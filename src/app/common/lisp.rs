use std::fmt::{self, Display};


#[derive(Debug, Clone)]
pub enum Error
{
}

impl Display for Error
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "")
    }
}

pub struct Lisp
{
    fill_tile: crate::common::world::Tile
}

impl Lisp
{
    pub fn new(code: &str) -> Self
    {
        let available = [
            ("soil", 5),
            ("concrete",3 ),
            ("wood", 4),
            ("asphalt", 1),
            ("grass", 2)
        ];

        let found = available.into_iter().find_map(|t|
        {
            code.contains(t.0).then(|| t.1)
        }).unwrap();

        let fill_tile = crate::common::world::Tile::new(found);

        Self{fill_tile}
    }

    // obviously temporary
    pub fn run(&self) -> Vec<crate::common::world::Tile>
    {
        vec![
            self.fill_tile;
            crate::server::world::world_generator::WORLD_CHUNK_SIZE.product()
        ]
    }
}
