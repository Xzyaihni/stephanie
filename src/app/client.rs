use std::{
    fs,
    io,
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    net::TcpStream,
    collections::HashMap
};

use nalgebra::Vector2;

use parking_lot::RwLock;

use image::error::ImageError;

use strum::IntoEnumIterator;

use yanyaengine::{
    ElementState,
    UniformLocation,
    DefaultModel,
    ShaderId,
    ModelId,
    camera::Camera,
    object::{Model, model::Uvs},
    game_object::*
};

use game::Game;

use game_state::{GameState, GameStateInfo};

use crate::{
    LOG_PATH,
    app::AppInfo,
    common::{
        some_or_return,
        DataInfos,
        MessagePasser,
        tilemap::TileMapWithTextures
    }
};

pub use visibility_checker::VisibilityChecker;

pub use ui_element::{UiEvent, MouseEvent, UiElement};

pub use game::DrawableEntity;
pub use game_state::{
    Ui,
    Control,
    ControlState,
    KeyMapping,
    CommonTextures
};

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::{TilesFactory, ChunkInfo};

pub mod visibility_checker;

pub mod ui_element;

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;


pub struct RenderCreateInfo<'a, 'b>
{
    pub location: UniformLocation,
    pub shader: ShaderId,
    pub squares: &'a HashMap<Uvs, ModelId>,
    pub object_info: &'a mut ObjectCreateInfo<'b>
}

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
    pub debug_mode: bool
}

pub struct Client
{
    game_state: Option<Rc<RefCell<GameState>>>,
    game: Game,
    squares: HashMap<Uvs, ModelId>
}

impl Client
{
    pub fn new(
        info: InitPartialInfo,
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

        let camera = Camera::new(info.aspect(), -1.0..1.0);
        let mut info = InitInfo::new(info, &camera);

        let camera = Arc::new(RwLock::new(camera));

        let tiles_factory = TilesFactory::new(
            &mut info,
            client_init_info.app_info.shaders.world,
            client_init_info.tilemap
        )?;

        let stream = TcpStream::connect(&client_init_info.client_info.address)?;
        stream.set_nodelay(true).unwrap();

        let message_passer = MessagePasser::new(stream);

        let assets = info.object_info.partial.assets.clone();
        let info = GameStateInfo{
            shaders: client_init_info.app_info.shaders,
            camera,
            object_info: info.object_info,
            data_infos: client_init_info.data_infos,
            tiles_factory,
            message_passer,
            client_info: &client_init_info.client_info,
            host: client_init_info.host
        };

        let game_state = GameState::new(info);

        let game = Game::new(Rc::downgrade(&game_state));

        let mut assets = assets.lock();
        let squares = Uvs::iter().map(|uvs|
        {
            let square = if uvs == Uvs::Normal
            {
                assets.default_model(DefaultModel::Square)
            } else
            {
                assets.push_model(Model::square_with_uvs(uvs, 1.0))
            };

            (uvs, square)
        }).collect();

        Ok(Self{
            game_state: Some(game_state),
            game,
            squares
        })
    }

    pub fn exit(&mut self)
    {
        self.game_state.take();
    }

    pub fn resize(&mut self, aspect: f32)
    {
        some_or_return!(&self.game_state).borrow_mut().resize(aspect);
    }

    pub fn update(&mut self, dt: f32)
    {
        let game_state = some_or_return!(&self.game_state);

        self.game.update(dt);

        if self.game.player_exists()
        {
            if game_state.borrow_mut().player_connected()
            {
                self.game.on_player_connected();

                game_state.borrow_mut().on_player_connected();
            }

            self.game.camera_sync();
        }

        if !game_state.borrow().running
        {
            self.exit();
        }
    }

    pub fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
        some_or_return!(&self.game_state).borrow_mut().update_buffers(&self.squares, partial_info);
    }

    pub fn draw(&mut self, mut info: DrawInfo)
    {
        some_or_return!(&self.game_state).borrow().draw(&mut info);
    }

    pub fn input(&mut self, control: yanyaengine::Control)
    {
        if let yanyaengine::Control::Keyboard{
            logical,
            state: ElementState::Pressed,
            ..
        } = control.clone()
        {
            if let Some(KeyMapping::Keyboard(key)) = KeyMapping::from_control(control.clone())
            {
                if self.game.on_key(logical, key)
                {
                    return;
                }
            }
        }

        some_or_return!(&self.game_state).borrow_mut().input(control);
    }

    pub fn mouse_move(&mut self, position: (f64, f64))
    {
        let position = Vector2::new(position.0 as f32, position.1 as f32);
        some_or_return!(&self.game_state).borrow_mut().mouse_moved(position);
    }
}
