use std::{
    fs,
    io,
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    net::TcpStream
};

use nalgebra::Vector2;

use parking_lot::RwLock;

use image::error::ImageError;

use yanyaengine::{
    ElementState,
    camera::Camera,
    game_object::*
};

use game::Game;

use game_state::{GameState, GameStateInfo};

use crate::{
    LOG_PATH,
    app::{AppInfo, TimestampQuery},
    common::{
        some_or_value,
        some_or_return,
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
    CommonTextures,
    ui::element::{UiElement, UiElementShapeMask}
};

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::{TilesFactory, ChunkInfo};

pub mod visibility_checker;

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;


pub struct ClientInitInfo
{
    pub client_info: ClientInfo,
    pub app_info: AppInfo,
    pub tilemap: TileMapWithTextures,
    pub data_infos: DataInfos,
    pub host: bool
}

pub struct ClientInfo
{
    pub address: String,
    pub name: String,
    pub debug: bool
}

pub struct Client
{
    pub camera: Arc<RwLock<Camera>>,
    game_state: Option<Rc<RefCell<GameState>>>,
    game: Game
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

        let camera = Camera::new(info.object_info.aspect(), -1.0..1.0);

        let timestamp_query = info.setup.clone();
        let mut info = info.object_info.to_full(&camera);

        let camera = Arc::new(RwLock::new(camera));

        let tiles_factory = TilesFactory::new(
            &mut info,
            client_init_info.tilemap
        )?;

        let stream = TcpStream::connect(&client_init_info.client_info.address)?;
        stream.set_nodelay(true).unwrap();

        let message_passer = MessagePasser::new(stream);

        let info = GameStateInfo{
            shaders: client_init_info.app_info.shaders,
            camera: camera.clone(),
            timestamp_query,
            object_info: info,
            data_infos: client_init_info.data_infos,
            tiles_factory,
            message_passer,
            player_name: client_init_info.client_info.name,
            debug_mode: client_init_info.client_info.debug,
            host: client_init_info.host
        };

        let game_state = GameState::new(info);

        let game = Game::new(Rc::downgrade(&game_state));

        Ok(Self{
            game_state: Some(game_state),
            camera,
            game
        })
    }

    pub fn exit(&mut self)
    {
        self.game_state.take();
    }

    pub fn resize(&mut self, aspect: f32)
    {
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
            update,
            {
                if !self.game.update(info, dt)
                {
                    *self.game_state.as_ref().unwrap().borrow_mut() = todo!();
                }

                let game_state = some_or_return!(self.game_state.as_ref());

                if self.game.player_exists()
                {
                    if game_state.borrow_mut().player_connected()
                    {
                        self.game.on_player_connected();

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
            update_buffers,
            some_or_return!(self.game_state.as_ref()).borrow_mut().update_buffers(info)
        };
    }

    pub fn draw(&mut self, mut info: DrawInfo)
    {
        crate::frame_time_this!{
            draw,
            some_or_return!(self.game_state.as_ref()).borrow().draw(&mut info)
        };
    }

    pub fn input(&mut self, control: yanyaengine::Control) -> bool
    {
        if let yanyaengine::Control::Keyboard{repeat: true, ..} = control
        {
            return true;
        }

        if let yanyaengine::Control::Keyboard{
            state,
            ..
        } = control.clone()
        {
            if let Some((KeyMapping::Keyboard(key), _)) = KeyMapping::from_control(control.clone())
            {
                if self.game.on_key_state(key, state == ElementState::Pressed)
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

    pub fn render_pass_ended(&mut self)
    {
        some_or_return!(self.game_state.as_ref()).borrow_mut().render_pass_ended();
    }
}
