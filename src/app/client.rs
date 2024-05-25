use std::{
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    net::TcpStream
};

use nalgebra::Vector2;

use parking_lot::RwLock;

use vulkano::{
    device::Device,
    buffer::{
        BufferUsage,
        Subbuffer,
        allocator::{SubbufferAllocator, SubbufferAllocatorCreateInfo}
    },
    memory::allocator::StandardMemoryAllocator
};

use image::error::ImageError;

use yanyaengine::{
    camera::Camera,
    object::{
        ObjectVertex,
        model::Model
    },
    game_object::*
};

use game::Game;

use game_state::{GameState, GameStateInfo};

use crate::common::{
    ItemsInfo,
    EnemiesInfo,
    MessagePasser,
    tilemap::TileMapWithTextures
};

pub use visibility_checker::VisibilityChecker;

pub use ui_element::{UiEvent, MouseEvent, UiElement};

pub use game::DrawableEntity;
pub use game_state::{Control, ControlState};

pub use connections_handler::ConnectionsHandler;
pub use tiles_factory::{TilesFactory, ChunkInfo};

pub mod visibility_checker;

pub mod ui_element;

pub mod game_state;
pub mod game;

pub mod connections_handler;
pub mod tiles_factory;

pub mod world_receiver;


#[derive(Debug, Clone)]
pub struct ObjectAllocator
{
    allocator: Rc<SubbufferAllocator>,
    frames: usize
}

impl ObjectAllocator
{
    pub fn new(device: Arc<Device>, frames: usize) -> Self
    {
        let allocator = StandardMemoryAllocator::new_default(device);
        let allocator = SubbufferAllocator::new(
            Arc::new(allocator),
            SubbufferAllocatorCreateInfo{
                buffer_usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
                ..Default::default()
            }
        );

        let allocator = Rc::new(allocator);

        Self{allocator, frames}
    }

    pub fn subbuffers(&self, model: &Model) -> Box<[Subbuffer<[ObjectVertex]>]>
    {
        (0..self.frames).map(|_|
        {
            self.allocator.allocate_slice(model.vertices.len() as u64).unwrap()
        }).collect::<Vec<_>>().into_boxed_slice()
    }

    pub fn subbuffers_amount(&self) -> usize
    {
        self.frames
    }
}

pub struct ClientInitInfo
{
    pub client_info: ClientInfo,
    pub tilemap: TileMapWithTextures,
    pub items_info: Arc<ItemsInfo>,
    pub enemies_info: Arc<EnemiesInfo>
}

pub struct ClientInfo
{
    pub address: String,
    pub name: String,
    pub debug_mode: bool
}

pub struct Client
{
    game_state: Rc<RefCell<GameState>>,
    game: Game
}

impl Client
{
    pub fn new(
        info: InitPartialInfo,
        client_init_info: ClientInitInfo
    ) -> Result<Self, ImageError>
    {
        let camera = Camera::new(info.aspect(), -1.0..1.0);
        let mut info = InitInfo::new(info, &camera);

        let camera = Arc::new(RwLock::new(camera));

        let tiles_factory = TilesFactory::new(&mut info, client_init_info.tilemap)?;

        let stream = TcpStream::connect(&client_init_info.client_info.address)?;
        let message_passer = MessagePasser::new(stream);

        let info = GameStateInfo{
            camera,
            object_info: info.object_info,
            items_info: client_init_info.items_info,
            enemies_info: client_init_info.enemies_info,
            tiles_factory,
            message_passer,
            client_info: &client_init_info.client_info
        };

        let game_state = GameState::new(info);

        let game = Game::new(game_state.player());
        let game_state = Rc::new(RefCell::new(game_state));

        Ok(Self{game_state, game})
    }

    pub fn resize(&mut self, aspect: f32)
    {
        self.game_state.borrow_mut().resize(aspect);
    }

    pub fn running(&self) -> bool
    {
        self.game_state.borrow().running
    }

    pub fn update(&mut self, dt: f32)
    {
        let mut writer = self.game_state.borrow_mut();

        self.game.update(&mut writer, dt);

        writer.update(dt);

        if self.game.player_exists(&mut writer)
        {
            if writer.player_connected()
            {
                self.game.on_player_connected(&mut writer);
            }

            self.game.camera_sync(&mut writer);
        }
    }

    pub fn update_buffers(&mut self, partial_info: UpdateBuffersPartialInfo)
    {
        self.game_state.borrow_mut().update_buffers(partial_info);
    }

    pub fn draw(&mut self, mut info: DrawInfo)
    {
        self.game_state.borrow().draw(&mut info);
    }

    pub fn input(&mut self, control: yanyaengine::Control)
    {
        self.game_state.borrow_mut().input(control);
    }

    pub fn mouse_move(&mut self, position: (f64, f64))
    {
        let position = Vector2::new(position.0 as f32, position.1 as f32);
        self.game_state.borrow_mut().mouse_moved(position);
    }
}
