use std::{env, sync::LazyLock};

use serde::{Serialize, Deserialize};

#[allow(unused_imports)]
use crate::{
    app::{
        SlowModeTrue,
        SlowModeFalse,
        SlowModeTrait,
    },
    client::game_state::{
        DebugVisibilityTrue,
        DebugVisibilityFalse,
        DebugVisibilityTrait
    }
};

use strum::{IntoEnumIterator, EnumIter, EnumCount, IntoStaticStr};


#[derive(Clone, Copy, EnumIter, EnumCount, IntoStaticStr)]
pub enum DebugTool
{
    Lisp,
    LispDisableChecks,
    RedundantWorldChecks,
    ShowMessages,
    DisplayRaycast,
    DisplayPathfind,
    CollisionWorldBounds,
    CollisionBounds,
    Contacts,
    Sleeping,
    Velocity,
    Spatial,
    SuperSpeed,
    SkipLoading,
    DamagePoints,
    LoadPosition,
    PrintDamage,
    PrintAnatomyColors,
    PrintListenerUpdates,
    DrawTileOcclusion,
    GpuDrawTimings,
    FrameTimings,
    NoOcclusion,
    NoWallOcclusion,
    NoSkyOcclusion,
    NoLighting,
    NoGravity,
    NoResolve,
    NoJoints,
    NoSpawns
}

pub trait DebugNameTrait
{
    fn new<I: Into<String>>(s: I) -> Self;

    fn name(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugNameTrue(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugNameFalse;

impl DebugNameTrait for DebugNameTrue
{
    fn new<I: Into<String>>(s: I) -> Self { Self(s.into()) }

    fn name(&self) -> &str { &self.0 }
}

impl DebugNameTrait for DebugNameFalse
{
    fn new<I: Into<String>>(_s: I) -> Self { Self }

    fn name(&self) -> &str { "undefined" }
}

pub trait DebugConfigTrait
{
    type DebugName: DebugNameTrait;
    type SlowMode: SlowModeTrait;
    type DebugVisibility: DebugVisibilityTrait;

    fn on_start();

    fn is_debug() -> bool;

    fn is_enabled(tool: DebugTool) -> bool;
    fn is_disabled(tool: DebugTool) -> bool
    {
        !Self::is_enabled(tool)
    }
}

pub struct DebugConfigTrue;
pub struct DebugConfigFalse;

impl DebugConfigTrait for DebugConfigTrue
{
    type DebugName = DebugNameTrue;
    type SlowMode = SlowModeTrue;
    type DebugVisibility = DebugVisibilityTrue;

    fn on_start()
    {
        let available = DebugTool::iter().map(|tool| -> String
        {
            let s: &str = tool.into();

            format!("STEPHANIE_{}", s.to_uppercase())
        }).reduce(|acc, x|
        {
            format!("{acc}\n{x}")
        }).unwrap_or_default();

        eprintln!("running in debug mode, available tools:\n{available}");
    }

    fn is_debug() -> bool { true }

    fn is_enabled(tool: DebugTool) -> bool
    {
        static STATES: LazyLock<[bool; DebugTool::COUNT]> = LazyLock::new(||
        {
            DebugTool::iter().map(|tool|
            {
                let s: &str = tool.into();
                env::var(format!("STEPHANIE_{}", s.to_uppercase())).map(|x|
                {
                    match x.to_lowercase().as_ref()
                    {
                        "0" | "false" => false,
                        "1" | "true" => true,
                        x =>
                        {
                            eprintln!("{s} is set to `{x}` which isnt a valid boolean");

                            false
                        }
                    }
                }).unwrap_or(false)
            }).collect::<Vec<_>>().try_into().unwrap()
        });

        STATES[tool as usize]
    }
}

impl DebugConfigTrait for DebugConfigFalse
{
    type DebugName = DebugNameFalse;
    type SlowMode = SlowModeFalse;
    type DebugVisibility = DebugVisibilityFalse;

    fn on_start() {}

    fn is_debug() -> bool { false }

    fn is_enabled(_tool: DebugTool) -> bool { false }
}

#[cfg(debug_assertions)]
pub type DebugConfig = DebugConfigTrue;

#[cfg(not(debug_assertions))]
pub type DebugConfig = DebugConfigFalse;
