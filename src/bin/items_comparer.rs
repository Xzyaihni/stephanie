#![allow(clippy::suspicious_else_formatting)]

use std::{
    env,
    process
};

use strum::{IntoEnumIterator, EnumIter, IntoStaticStr};

use stephanie::common::{
    anatomy::*,
    Side2d,
    DamageHeight,
    Damageable,
    DamageDirection,
    Damage,
    ItemInfo,
    ItemsInfo
};


const MAX_HITS: f32 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, IntoStaticStr)]
enum RunMode
{
    Hits,
    Oxygen
}

fn parse_mode(s: &str) -> Option<RunMode>
{
    RunMode::iter().map(|x| (x, <&str>::from(x))).find(|(_x, check)| *check == s).map(|(a, _)| a)
}

fn print_info(ItemStats{
    name,
    kill_hits,
    kill_oxygen
}: &ItemStats)
{
    println!("{name} - hits: {kill_hits:.1}, oxygen: {kill_oxygen:.1}");
}

fn print_infos(infos: &[ItemStats])
{
    infos.iter().for_each(|x|
    {
        print_info(x);
    });
}

struct AnatomyStats
{
    kill_hits: f32,
    kill_oxygen: f32
}

impl AnatomyStats
{
    fn combine(self, other: Self) -> Self
    {
        Self{
            kill_hits: (self.kill_hits + other.kill_hits) / 2.0,
            kill_oxygen: (self.kill_oxygen + other.kill_oxygen) / 2.0
        }
    }
}

fn anatomy_stats_single(
    mut anatomy: Anatomy,
    item: &ItemInfo
) -> AnatomyStats
{
    let mut stats = AnatomyStats{
        kill_hits: 0.0,
        kill_oxygen: 0.0
    };

    while !anatomy.take_killed()
    {
        let damage = Damage{
            data: (if fastrand::bool() { item.bash_damage() } else { item.poke_damage() }) * 30.0,
            direction: DamageDirection{
                side: Side2d::random(),
                height: DamageHeight::random()
            }
        };

        stats.kill_hits += 1.0;
        stats.kill_oxygen += item.oxygen_cost(30.0);

        if stats.kill_hits > MAX_HITS
        {
            break;
        }

        anatomy.damage(damage);
    }

    stats
}

fn anatomy_stats(
    anatomy: Anatomy,
    item: &ItemInfo
) -> AnatomyStats
{
    (0..5).map(|_| anatomy_stats_single(anatomy.clone(), item)).reduce(|acc, x| acc.combine(x)).unwrap()
}

struct ItemStats
{
    name: String,
    kill_hits: f32,
    kill_oxygen: f32
}

fn main()
{
    let mode = if let Some(x) = env::args().nth(1)
    {
        if let Some(x) = parse_mode(&x)
        {
            x
        } else
        {
            let modes = RunMode::iter().map(<&str>::from).fold(String::new(), |acc, x|
            {
                acc + ", " + x
            });

            eprintln!("{x} isnt a valid mode, try: {modes}");

            process::exit(1)
        }
    } else
    {
        RunMode::Oxygen
    };

    let mut infos = ItemsInfo::parse(None, "".into(), "items/items.json".into()).items().iter().map(|info: &ItemInfo| -> ItemStats
    {
        let name = info.name.clone();

        let anatomy = Anatomy::Human(HumanAnatomy::new(HumanAnatomyInfo::default()));

        let AnatomyStats{
            kill_hits,
            kill_oxygen
        } = anatomy_stats(anatomy, info);

        ItemStats{
            name,
            kill_hits,
            kill_oxygen
        }
    }).collect::<Vec<_>>();

    match mode
    {
        RunMode::Hits =>
        {
            infos.sort_unstable_by(|a, b| a.kill_hits.partial_cmp(&b.kill_hits).unwrap());
        },
        RunMode::Oxygen =>
        {
            infos.sort_unstable_by(|a, b| a.kill_oxygen.partial_cmp(&b.kill_oxygen).unwrap());
        }
    }

    print_infos(&infos);
}
