use std::{
    rc::{Weak, Rc},
    cell::RefCell,
    sync::mpsc::Sender
};

use nalgebra::{vector, Vector3};

use crate::{
    client::game_state::GameState,
    server::world::World as ServerWorld,
    common::{
        some_or_value,
        lisp::{self, *},
        Pos3,
        Transform,
        Entity,
        EntityInfo,
        Message,
        Item,
        AnyEntities,
        ConnectionId,
        DataInfos,
        TileMap,
        world::{TilePos, GlobalPos, ChunkLocal, Tile},
        entity::{ServerEntities, ClientEntities},
        inventory::{inventory_remove_item, InventoryItem}
    }
};


fn parse_symbol_or_string(value: OutputWrapperRef) -> Result<String, lisp::Error>
{
    let s = if let Ok(x) = value.as_symbol()
    {
        x.replace('_', " ")
    } else if let Ok(x) = value.as_string()
    {
        x
    } else
    {
        return Err(lisp::Error::Custom("expected symbol or string".to_owned()));
    };

    Ok(s)
}

pub fn parse_entity(entities: &impl AnyEntities, value: OutputWrapperRef) -> Result<Entity, lisp::Error>
{
    fn err_if_none(value: Option<OutputWrapperRef>) -> Result<OutputWrapperRef, lisp::Error>
    {
        value.ok_or_else(|| lisp::Error::Custom("expected more values".to_owned()))
    }

    let mut values = value.as_pairs_list()?.into_iter();

    let tag = err_if_none(values.next())?.as_symbol()?;
    if tag != "entity"
    {
        let s = format!("(expected tag `entity` got `{tag}`)");

        return Err(lisp::Error::Custom(s));
    }

    let local = err_if_none(values.next())?.as_bool()?;
    let id = err_if_none(values.next())?.as_integer()?;

    let entity = entities.with_seed(Entity::from_raw(local, id as usize).no_seed());

    Ok(entity)
}

fn parse_position_with<T>(
    value: OutputWrapperRef,
    f: impl Fn(&OutputWrapperRef) -> Result<T, lisp::Error>
) -> Result<Vector3<T>, lisp::Error>
{
    let value: Vec<_> = value.as_pairs_list()?;

    if value.len() != 3
    {
        return Err(lisp::Error::Custom("expected list of length 3".to_owned()));
    }

    let f = |i: usize| f(&value[i]);

    Ok(vector![f(0)?, f(1)?, f(2)?])
}

pub fn parse_position(value: OutputWrapperRef) -> Result<Vector3<f32>, lisp::Error>
{
    parse_position_with(value, |x| x.as_float())
}

pub fn parse_tile_position(value: OutputWrapperRef) -> Result<TilePos, lisp::Error>
{
    let LispList{car: tile_pos, cdr: chunk_pos} = value.as_list()?;

    Ok(TilePos{
        local: ChunkLocal::from(Pos3::from(parse_position_with(tile_pos, |x| x.as_integer())?.map(|x| x as usize))),
        chunk: GlobalPos(parse_position_with(chunk_pos, |x| x.as_integer())?.into())
    })
}

pub fn push_entity(memory: &mut LispMemory, entity: Entity) -> Result<LispValue, lisp::Error>
{
    let tag = memory.new_symbol("entity");
    let local = LispValue::new_bool(entity.local());
    let id = LispValue::new_integer(entity.id() as i32);

    memory.cons_list([tag, local, id])
}

pub fn push_transform(memory: &mut LispMemory, transform: Transform) -> Result<LispValue, lisp::Error>
{
    let position = transform.position;
    let scale = transform.scale;
    let rotation = transform.rotation;

    let restore = memory.with_saved_registers([Register::Temporary]);

    {
        let position_list = memory.cons_list([position.x, position.y, position.z])?;
        memory.set_register(Register::Temporary, position_list);
    }

    let scale_list = memory.cons_list([scale.x, scale.y, scale.z])?;

    let transform_list = memory.cons_list([memory.get_register(Register::Temporary), scale_list, rotation.into()])?;

    restore(memory)?;

    Ok(transform_list)
}

fn entity_transform_common(entities: &impl AnyEntities, mut args: PrimitiveArgs) -> Result<LispValue, lisp::Error>
{
    let entity = parse_entity(entities, args.next_value().unwrap())?;

    let transform = entities.transform(entity)
        .ok_or_else(|| lisp::Error::Custom("entity doesnt have a transform".to_owned()))?;

    push_transform(args.memory, transform.clone())
}

fn world_set_tile_common(
    tilemap: &TileMap,
    mut args: PrimitiveArgs,
    handler: impl FnOnce(TilePos, Tile)
) -> Result<LispValue, lisp::Error>
{
    let pos = parse_tile_position(args.next_value().unwrap())?;
    let tile = parse_symbol_or_string(args.next_value().unwrap())?;

    let id = tilemap.tile_named(&tile).ok_or_else(||
    {
        lisp::Error::Custom(format!("tile named `{tile}` not found"))
    })?;

    handler(pos, id);

    Ok(().into())
}

type MessageSenderType = Rc<RefCell<Option<Sender<(ConnectionId, Message, Option<Entity>)>>>>;

fn upgraded<T>(
    x: &Rc<RefCell<Weak<RefCell<T>>>>,
    name: &str
) -> Result<Rc<RefCell<T>>, lisp::Error>
{
    let x = x.borrow();

    x.upgrade().ok_or_else(|| lisp::Error::Custom(format!("{name} must exist when calling script")))
}

pub fn server_info_primitives(
    tilemap: Rc<TileMap>,
    world: Rc<RefCell<Weak<RefCell<Option<ServerWorld>>>>>,
    entities: Rc<RefCell<Weak<RefCell<ServerEntities>>>>,
    data_infos: DataInfos,
    sender: MessageSenderType
) -> Primitives
{
    let mut primitives = Primitives::default();

    fn with_entities<T>(
        entities: &Rc<RefCell<Weak<RefCell<ServerEntities>>>>,
        f: impl FnOnce(&mut ServerEntities) -> Result<T, lisp::Error>
    ) -> Result<T, lisp::Error>
    {
        let entities = upgraded(entities, "entities")?;

        let mut entities = entities.borrow_mut();

        f(&mut entities)
    }

    {
        let entities = entities.clone();

        primitives.add("entity-transform", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |args|
        {
            with_entities(&entities, |entities|
            {
                entity_transform_common(entities, args)
            })
        }));
    }

    {
        let tilemap = tilemap.clone();

        primitives.add("set-tile", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |args|
        {
            let world = upgraded(&world, "world")?;
            let mut world = world.borrow_mut();
            let world = world.as_mut().expect("must always exist");

            world_set_tile_common(&tilemap, args, |pos, id| world.set_tile_lazy(pos, id))
        }));
    }

    fn make_sender(sender: MessageSenderType) -> impl Fn(Message)
    {
        move |message|
        {
            let mut sender = sender.borrow_mut();

            if let Err(err) = sender.as_mut().expect("must be initialized before primitive calls").send((ConnectionId(0), message, None))
            {
                eprintln!("error sending message: {err}");
            }
        }
    }

    {
        let messager = make_sender(sender.clone());

        primitives.add("spawn-enemy", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
        {
            let name = parse_symbol_or_string(args.next_value().unwrap())?;
            let pos = parse_position(args.next_value().unwrap())?;

            let id = data_infos.enemies_info.get_id(&name).ok_or_else(||
            {
                lisp::Error::Custom(format!("enemy named `{name}` not found"))
            })?;

            messager(Message::SpawnEnemy{id, pos});

            Ok(().into())
        }));
    }

    primitives
}

pub fn add_info_primitives(primitives: &mut Primitives, game_state: Rc<RefCell<Weak<RefCell<GameState>>>>)
{
    fn with_game_state<T>(
        game_state: &Rc<RefCell<Weak<RefCell<GameState>>>>,
        f: impl FnOnce(&mut GameState) -> Result<T, lisp::Error>
    ) -> Result<T, lisp::Error>
    {
        let game_state = upgraded(game_state, "game_state")?;

        let mut game_state = game_state.borrow_mut();

        f(&mut game_state)
    }

    {
        let game_state = game_state.clone();

        primitives.add("entity-transform", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                entity_transform_common(entities, args)
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("remove-inventory-item", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                let entity = parse_entity(entities, args.next_value().unwrap())?;

                let item_index = args.next().unwrap().as_integer()?;

                inventory_remove_item(entities, entity, InventoryItem::from_raw(item_index as usize));

                Ok(().into())
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("add-item", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                let entity = parse_entity(entities, args.next_value().unwrap())?;

                let name = parse_symbol_or_string(args.next_value().unwrap())?;

                let mut inventory = entities.inventory_mut(entity).ok_or_else(||
                {
                    lisp::Error::Custom("entity doesnt have an inventory".to_owned())
                })?;

                let id = game_state.data_infos.items_info.get_id(&name).ok_or_else(||
                {
                    lisp::Error::Custom(format!("item named {name} doesnt exist"))
                })?;

                let items_info = &game_state.data_infos.items_info;
                inventory.push(items_info, Item::new(items_info, id));

                Ok(().into())
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("spawn-enemy", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let name = parse_symbol_or_string(args.next_value().unwrap())?;
                let pos = parse_position(args.next_value().unwrap())?;

                let id = game_state.data_infos.enemies_info.get_id(&name).ok_or_else(||
                {
                    lisp::Error::Custom(format!("enemy named `{name}` not found"))
                })?;

                game_state.send_message(Message::SpawnEnemy{id, pos});

                Ok(().into())
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("set-tile", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |args|
        {
            with_game_state(&game_state, |game_state|
            {
                let tilemap = game_state.world.tilemap_clone();

                world_set_tile_common(&tilemap, args, |pos, id| game_state.world.set_tile_lazy(pos, id))
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("entity-collided", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                let entity = parse_entity(entities, args.next_value().unwrap())?;
                let collided = entities.collider(entity)
                    .map(|x| x.collided().to_vec()).into_iter().flatten()
                    .next();

                if let Some(collided) = collided
                {
                    push_entity(args.memory, collided)
                } else
                {
                    Ok(().into())
                }
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("children-of", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                let entity = parse_entity(entities, args.next_value().unwrap())?;

                args.memory.cons_list_with(|memory|
                {
                    let mut count = 0;
                    entities.children_of(entity).try_for_each(|x|
                    {
                        count += 1;
                        let value = push_entity(memory, x)?;

                        memory.push_stack(value)?;

                        Ok(())
                    })?;

                    Ok(count)
                })
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("spawn-entity-raw", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                let info: EntityInfo = serde_json::from_str(&args.next_value().unwrap().as_string()?)
                    .map_err(|err| lisp::Error::Custom(format!("error spawning entity: {err}")))?;

                let entity = entities.push(true, info);

                push_entity(args.memory, entity)
            })
        }));
    }

    {
        let game_state = game_state.clone();

        primitives.add("add-screenshake-offset", PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entity = parse_entity(game_state.entities(), args.next_value().unwrap())?;

                let list = args.next_value().unwrap().as_list()?;

                let x = list.car.as_float()?;
                let y = list.cdr.as_list()?.car.as_float()?;

                let mut player = game_state.entities().player_mut(entity).ok_or_else(||
                {
                    lisp::Error::Custom("entity doesnt have a player component".to_owned())
                })?;

                player.screenshake.add_offset(vector![x, y]);

                Ok(().into())
            })
        }));
    }

    fn add_simple_setter<F>(
        primitives: &mut Primitives,
        game_state: &Rc<RefCell<Weak<RefCell<GameState>>>>,
        name: &str,
        f: F
    )
    where
        F: Fn(
            &mut ClientEntities,
            Entity,
            OutputWrapperRef
        ) -> Result<(), lisp::Error> + 'static
    {
        let game_state = game_state.clone();

        primitives.add(name, PrimitiveProcedureInfo::new_simple(2, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities_mut();

                let entity = parse_entity(entities, args.next_value().unwrap())?;

                let value = args.next().unwrap();
                let value = OutputWrapperRef::new(args.memory, value);

                f(entities, entity, value)?;

                Ok(().into())
            })
        }));
    }

    macro_rules! get_component_mut
    {
        ($name:ident, $entities:expr, $entity:expr) =>
        {
            {
                let name = stringify!($name).trim_end_matches("_mut");

                some_or_value!(
                    $entities.$name($entity),
                    Err(lisp::Error::Custom(format!("component {name} is missing")))
                )
            }
        }
    }

    add_simple_setter(primitives, &game_state, "set-floating", |entities, entity, value|
    {
        let state = value.as_bool()?;

        get_component_mut!(physical_mut, entities, entity).set_floating(state);

        Ok(())
    });

    add_simple_setter(primitives, &game_state, "set-speed", |entities, entity, value|
    {
        let speed = value.as_float()?;

        get_component_mut!(anatomy_mut, entities, entity).set_speed(speed);

        Ok(())
    });

    add_simple_setter(primitives, &game_state, "set-ghost", |entities, entity, value|
    {
        let state = value.as_bool()?;

        get_component_mut!(collider_mut, entities, entity).ghost = state;

        Ok(())
    });

    add_simple_setter(primitives, &game_state, "set-position", |entities, entity, value|
    {
        let position = parse_position(value)?;

        get_component_mut!(target, entities, entity).position = position;

        Ok(())
    });

    add_simple_setter(primitives, &game_state, "set-rotation", |entities, entity, value|
    {
        get_component_mut!(target, entities, entity).rotation = value.as_float()?;

        Ok(())
    });

    add_simple_setter(primitives, &game_state, "set-faction", |entities, entity, value|
    {
        let faction = value.as_symbol()?;
        let faction: String = faction.to_lowercase().chars().enumerate().map(|(i, c)|
        {
            if i == 0
            {
                c.to_ascii_uppercase()
            } else
            {
                c
            }
        }).collect();

        let faction = format!("\"{faction}\"");
        let faction = serde_json::from_str(&faction).map_err(|_|
        {
            lisp::Error::Custom(format!("cant deserialize {faction} as Faction"))
        })?;

        get_component_mut!(character_mut, entities, entity).faction = faction;

        Ok(())
    });
}

#[derive(Debug, Clone, Copy)]
pub struct ScriptIndex(usize);

pub struct ScriptsContainer
{
    pub item_primitives: Rc<Primitives>,
    game_state: Rc<RefCell<Weak<RefCell<GameState>>>>,
    scripts: Vec<Lisp>
}

impl ScriptsContainer
{
    pub fn new() -> Self
    {
        let game_state: Rc<RefCell<Weak<RefCell<GameState>>>> = Rc::new(RefCell::new(Weak::new()));

        let item_primitives = {
            let mut primitives = Primitives::default();

            add_info_primitives(&mut primitives, game_state.clone());

            Rc::new(primitives)
        };

        Self{
            item_primitives,
            game_state,
            scripts: Vec::new()
        }
    }

    pub fn new_empty() -> Self
    {
        Self{
            item_primitives: Rc::new(Primitives::default()),
            game_state: Rc::new(RefCell::new(Weak::new())),
            scripts: Vec::new()
        }
    }

    pub fn set_game_state(&self, game_state: Weak<RefCell<GameState>>)
    {
        *self.game_state.borrow_mut() = game_state;
    }

    pub fn push(&mut self, value: Lisp) -> ScriptIndex
    {
        let id = self.scripts.len();

        self.scripts.push(value);

        ScriptIndex(id)
    }

    pub fn get(&self, id: ScriptIndex) -> &Lisp
    {
        &self.scripts[id.0]
    }
}
