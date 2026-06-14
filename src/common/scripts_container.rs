use std::{
    rc::{Weak, Rc},
    cell::RefCell
};

use nalgebra::{vector, Vector3};

use crate::{
    client::game_state::GameState,
    common::{
        some_or_value,
        lisp::{self, *},
        Entity,
        Message,
        Item,
        AnyEntities,
        entity::ClientEntities,
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

pub fn parse_entity(entities: &ClientEntities, value: OutputWrapperRef) -> Result<Entity, lisp::Error>
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

pub fn parse_position(value: OutputWrapperRef) -> Result<Vector3<f32>, lisp::Error>
{
    let value: Vec<_> = value.as_pairs_list()?;

    if value.len() != 3
    {
        return Err(lisp::Error::Custom("expected list of length 3".to_owned()));
    }

    let f = |i: usize| value[i].as_float();

    Ok(vector![f(0)?, f(1)?, f(2)?])
}

pub fn push_entity(memory: &mut LispMemory, entity: Entity) -> Result<LispValue, lisp::Error>
{
    let tag = memory.new_symbol("entity");
    let local = LispValue::new_bool(entity.local());
    let id = LispValue::new_integer(entity.id() as i32);

    memory.cons_list([tag, local, id])
}

pub fn add_info_primitives(primitives: &mut Primitives, game_state: Rc<RefCell<Weak<RefCell<GameState>>>>)
{
    fn upgrade_game_state(
        game_state: &Rc<RefCell<Weak<RefCell<GameState>>>>
    ) -> Result<Rc<RefCell<GameState>>, lisp::Error>
    {
        let game_state = game_state.borrow();

        game_state.upgrade().ok_or_else(||
        {
            lisp::Error::Custom("game_state must exist when calling script".to_owned())
        })
    }

    fn with_game_state<T>(
        game_state: &Rc<RefCell<Weak<RefCell<GameState>>>>,
        f: impl FnOnce(&mut GameState) -> Result<T, lisp::Error>
    ) -> Result<T, lisp::Error>
    {
        let game_state = upgrade_game_state(game_state)?;
        let mut game_state = game_state.borrow_mut();

        f(&mut game_state)
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

    {
        let game_state = game_state.clone();

        primitives.add("entity-transform", PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
        {
            with_game_state(&game_state, |game_state|
            {
                let entities = game_state.entities();

                let entity = parse_entity(entities, args.next_value().unwrap())?;

                let transform = entities.transform(entity)
                    .ok_or_else(|| lisp::Error::Custom("entity doesnt have a transform".to_owned()))?;

                let position = transform.position;
                let scale = transform.scale;
                let rotation = transform.rotation;

                let restore = args.memory.with_saved_registers([Register::Temporary]);

                {
                    let position_list = args.memory.cons_list([position.x, position.y, position.z])?;
                    args.memory.set_register(Register::Temporary, position_list);
                }

                let scale_list = args.memory.cons_list([scale.x, scale.y, scale.z])?;

                let transform_list = args.memory.cons_list([args.memory.get_register(Register::Temporary), scale_list, rotation.into()])?;

                restore(args.memory)?;

                Ok(transform_list)
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

                let mut inventory = entities.inventory_mut(entity).unwrap();

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
