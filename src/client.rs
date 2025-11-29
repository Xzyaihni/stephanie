use std::{
    fs,
    io,
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    net::TcpStream,
    collections::HashMap
};

use nalgebra::{Vector2, Vector3};

use parking_lot::RwLock;

use image::error::ImageError;

use yanyaengine::{
    Transform,
    ElementState,
    SolidObject,
    DefaultModel,
    camera::Camera,
    game_object::*
};

use game::Game;

use game_state::{UiAnatomyLocations, GameState, GameStateInfo};

use crate::{
    LOG_PATH,
    debug_config::*,
    app::{AppInfo, TimestampQuery},
    common::{
        some_or_value,
        some_or_return,
        Loot,
        DataInfos,
        MessagePasser,
        tilemap::TileMapWithTextures
    }
};

pub use visibility_checker::VisibilityChecker;

pub use game_state::{
    Ui,
    Control,
    ControlState,
    KeyMapping,
    ChangedKey,
    CommonTextures,
    ui::element::{UiElement, UiElementShapeMask}
};

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::{TilesFactory, ChunkInfo};

pub use sliced_texture::{SlicedTexture, PartCreator};

pub mod visibility_checker;

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;

pub mod ui_common;

mod sliced_texture;


pub struct ClientInitInfo
{
    pub app_info: AppInfo,
    pub sliced_textures: Rc<HashMap<String, SlicedTexture>>,
    pub tilemap: TileMapWithTextures,
    pub data_infos: DataInfos
}

pub struct ClientInfo
{
    pub address: String,
    pub name: String,
    pub host: bool,
    pub debug: bool,
    pub mouse_position: Vector2<f32>,
    pub controls: Vec<(KeyMapping, Control)>
}

fn create_message_passer(address: &str) -> io::Result<MessagePasser>
{
    let stream = TcpStream::connect(address)?;
    stream.set_nodelay(true).unwrap();

    Ok(MessagePasser::new(stream))
}

pub fn create_screen_object(info: &ObjectCreatePartialInfo) -> SolidObject
{
    let assets = info.assets.lock();

    info.object_factory.create_solid(
        assets.model(assets.default_model(DefaultModel::Square)).clone(),
        Transform{scale: Vector3::repeat(2.0), ..Default::default()}
    )
}

pub struct Client
{
    pub client_info: Option<ClientInfo>,
    pub camera: Arc<RwLock<Camera>>,
    info: Option<GameStateInfo>,
    game_state: Option<Rc<RefCell<GameState>>>,
    game: Option<Game>
}

impl Client
{
    pub fn new(
        info: InitPartialInfo<TimestampQuery>,
        client_init_info: ClientInitInfo
    ) -> Result<Self, ImageError>
    {
        match fs::remove_file(LOG_PATH)
        {
            Ok(_) => (),
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) =>
            {
                eprintln!("error removing log file: {err}");
            }
        }

        let loot = Loot::new(client_init_info.data_infos.items_info.clone(), "items/loot.scm").unwrap_or_else(|err|
        {
            panic!("error parsing loot: {err}")
        });

        let camera = Camera::new(info.object_info.aspect(), -1.0..1.0);

        let timestamp_query = info.setup.clone();
        let mut info = info.object_info.to_full(&camera);

        let camera = Arc::new(RwLock::new(camera));

        let tiles_factory = TilesFactory::new(
            &mut info,
            client_init_info.tilemap
        )?;

        let assets = &info.partial.assets;
        let anatomy_locations = {
            let assets = assets.clone();
            Rc::new(RefCell::new(move |object_info: &mut ObjectCreateInfo, name: &str| -> UiAnatomyLocations
            {
                let base_image = image::open(format!("textures/special/{name}.png"))
                    .unwrap_or_else(|err|
                    {
                        panic!("{name}.png must exist: {err}")
                    });

                let mut assets = assets.lock();

                let part_creator = PartCreator{
                    assets: &mut assets,
                    resource_uploader: object_info.partial.builder_wrapper.resource_uploader_mut()
                };

                UiAnatomyLocations::new(part_creator, base_image)
            }))
        };

        let common_textures = CommonTextures::new(&mut assets.lock());

        let info = GameStateInfo{
            shaders: client_init_info.app_info.shaders,
            sliced_textures: client_init_info.sliced_textures,
            camera: camera.clone(),
            timestamp_query,
            data_infos: client_init_info.data_infos,
            loot,
            tiles_factory,
            anatomy_locations,
            common_textures
        };

        Ok(Self{
            client_info: None,
            camera,
            info: Some(info),
            game_state: None,
            game: None
        })
    }

    pub fn initialize(
        &mut self,
        info: &mut UpdateBuffersInfo,
        client_info: ClientInfo
    )
    {
        self.client_info = Some(client_info);

        self.initialize_inner(info)
    }

    fn initialize_inner(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        let client_info = self.client_info.as_ref().unwrap();

        let message_passer = match create_message_passer(&client_info.address)
        {
            Ok(x) => x,
            Err(err) =>
            {
                self.exit();
                panic!("error starting the game: {err}")
            }
        };

        let new_game_state = GameState::new(
            info,
            message_passer,
            self.info.clone().unwrap(),
            client_info
        );

        self.game = Some(Game::new(Rc::downgrade(&new_game_state)));
        self.game_state = Some(new_game_state);
    }

    pub fn exit(&mut self)
    {
        self.info.take();
        self.game_state.take();
    }

    pub fn with_game_state<T>(&self, f: impl FnOnce(&GameState) -> T) -> Option<T>
    {
        Some(f(&some_or_return!(self.game_state.as_ref()).borrow()))
    }

    pub fn resize(&mut self, aspect: f32)
    {
        self.camera.write().resize(aspect);

        some_or_return!(self.game_state.as_ref()).borrow_mut().resize(aspect);
    }

    pub fn no_update(&mut self)
    {
        some_or_return!(self.game_state.as_ref()).borrow_mut().no_update();
    }

    pub fn update(
        &mut self,
        info: &mut UpdateBuffersInfo,
        dt: f32
    )
    {
        crate::frame_time_this!{
            [] -> update,
            {
                if !self.game.as_mut().unwrap().update(info, dt)
                {
                    {
                        let game_state = self.game_state.take().unwrap();
                        game_state.borrow_mut().restart();
                    }

                    self.initialize_inner(info);

                    return self.update(info, dt);
                }

                let game_state = some_or_return!(self.game_state.as_ref());

                game_state.borrow_mut().update_loading();

                if self.game.as_mut().unwrap().player_exists()
                {
                    if game_state.borrow_mut().player_connected()
                    {
                        self.game.as_mut().unwrap().on_player_connected();

                        game_state.borrow_mut().on_player_connected();
                    }
                }

                if !game_state.borrow().running
                {
                    self.exit();
                }
            }
        };
    }

    pub fn update_buffers(&mut self, info: &mut UpdateBuffersInfo)
    {
        crate::frame_time_this!{
            [] -> update_buffers,
            some_or_return!(self.game_state.as_ref()).borrow_mut().update_buffers(info)
        };
    }

    pub fn draw(&mut self, mut info: DrawInfo)
    {
        crate::frame_time_this!{
            [] -> draw,
            some_or_return!(self.game_state.as_ref()).borrow().draw(&mut info)
        };
    }

    pub fn input(&mut self, control: yanyaengine::Control) -> bool
    {
        if let yanyaengine::Control::Keyboard{
            state,
            ..
        } = control.clone()
        {
            if let Some(ChangedKey{key: KeyMapping::Keyboard(key), ..}) = KeyMapping::from_control(control.clone())
            {
                if self.game.as_mut().unwrap().on_key_state(key, state == ElementState::Pressed)
                {
                    return true;
                }
            }
        }

        some_or_value!(self.game_state.as_ref(), false).borrow_mut().input(control)
    }

    pub fn mouse_move(&mut self, position: (f64, f64))
    {
        let position = Vector2::new(position.0 as f32, position.1 as f32);
        some_or_return!(self.game_state.as_ref()).borrow_mut().mouse_moved(position);
    }

    fn check_timings(&self)
    {
        #[cfg(any(debug_assertions, stimings))]
        {
            use crate::common::TimingsTrait;

            let mut timings = crate::common::THIS_FRAME_TIMINGS.lock();

            let frame_time = timings.update.total.unwrap_or(0.0)
                + timings.update_buffers.total.unwrap_or(0.0)
                + timings.draw.total.unwrap_or(0.0);

            if frame_time > (1000.0 / crate::common::TARGET_FPS as f64)
            {
                eprintln!("{}", timings.display(0).unwrap());
            }

            *timings = Default::default();
        }
    }

    pub fn render_pass_ended(&mut self)
    {
        some_or_return!(self.game_state.as_ref()).borrow_mut().render_pass_ended();

        if DebugConfig::is_enabled(DebugTool::FrameTimings) || cfg!(stimings)
        {
            self.check_timings()
        }
    }
}
