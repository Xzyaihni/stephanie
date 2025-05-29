use std::{
    fs,
    cell::RefCell,
    path::PathBuf,
    sync::Arc
};

use crate::{
    server::world::ParseError,
    common::{
        lisp::{self, *},
        Item,
        ItemsInfo
    }
};


pub struct Loot
{
    info: Arc<ItemsInfo>,
    creator: RefCell<Lisp>
}

impl Loot
{
    pub fn new(info: Arc<ItemsInfo>, filename: &str) -> Result<Self, ParseError>
    {
        let load = |filename|
        {
            fs::read_to_string(filename)
                .map_err(|err| ParseError::new_named(PathBuf::from(filename), err))
        };

        let standard = load("lisp/standard.scm")?;
        let code = load(filename)?;

        let creator = RefCell::new(Lisp::new(&(standard + &code))
            .map_err(|err| ParseError::new_named(PathBuf::from(filename), err))?);

        Ok(Self{info, creator})
    }

    fn parse_item(&self, value: OutputWrapperRef) -> Result<Item, lisp::Error>
    {
        let name = value.as_symbol()?;
        let name: String = name.chars().map(|c| if c == '_' { ' ' } else { c }).collect();

        let id = self.info.get_id(&name).ok_or_else(||
        {
            lisp::Error::Custom(format!("item named {name} not found"))
        })?;

        Ok(Item{id})
    }

    pub fn create(&self, name: &str) -> impl Iterator<Item=Item> + use<'_>
    {
        {
            let mut creator = self.creator.borrow_mut();
            let memory = creator.memory_mut();

            let name = memory.new_symbol(name);

            memory.define("name", name).unwrap_or_else(|err|
            {
                panic!("{err}")
            })
        }

        let (memory, value) = self.creator.borrow().run().unwrap_or_else(|err|
        {
            panic!("{err}")
        }).destructure();

        let items = value.as_pairs_list(&memory).unwrap_or_else(|err|
        {
            panic!("{err}")
        });

        items.into_iter().map(move |item|
        {
            self.parse_item(OutputWrapperRef::new(&memory, item)).unwrap_or_else(|err|
            {
                panic!("{err}")
            })
        })
    }
}
