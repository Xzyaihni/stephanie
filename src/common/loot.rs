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

        let creator = RefCell::new(Lisp::new(&[&standard, &code])
            .map_err(|err| ParseError::new_named(PathBuf::from(filename), err))?);

        Ok(Self{info, creator})
    }

    fn parse_item(&self, value: OutputWrapperRef) -> Result<Item, lisp::Error>
    {
        let name = value.as_symbol().map(|name|
        {
            name.chars().map(|c| if c == '_' { ' ' } else { c }).collect::<String>()
        }).or_else(|_|
        {
            value.as_string()
        })?;

        let id = self.info.get_id(&name).ok_or_else(||
        {
            lisp::Error::Custom(format!("item named {name} not found"))
        })?;

        Ok(Item::new(&self.info, id))
    }

    pub fn create(&self, name: &str) -> Vec<Item>
    {
        {
            let mut creator = self.creator.borrow_mut();
            let memory = creator.memory_mut();

            let symbol = memory.new_symbol(name);
            if let Err(err) = memory.define("name", symbol)
            {
                eprintln!("{name}: {err}");

                return Vec::new();
            }
        }

        let (memory, value) = match self.creator.borrow().run()
        {
            Ok(x) => x.destructure(),
            Err(err) =>
            {
                eprintln!("{name}: {err}");

                return Vec::new();
            }
        };

        let items = match value.as_pairs_list(&memory)
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("{name}: {err}");

                return Vec::new();
            }
        };

        items.into_iter().filter_map(move |item|
        {
            let item = self.parse_item(OutputWrapperRef::new(&memory, item));

            match item
            {
                Ok(x) => Some(x),
                Err(err) =>
                {
                    eprintln!("{name}: {err}");

                    None
                }
            }
        }).collect()
    }
}
