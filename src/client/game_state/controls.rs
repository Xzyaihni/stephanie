use enum_amount::EnumCount;


#[repr(usize)]
#[derive(Debug, Clone, Copy, EnumCount)]
pub enum Control
{
	MoveUp = 0,
	MoveDown,
	MoveRight,
	MoveLeft,
	MainAction,
	SecondaryAction,
	Jump,
	Crouch,
	ZoomIn,
	ZoomOut,
	ZoomReset
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState
{
	Held,
	Clicked,
	Released,
	Locked
}

impl ControlState
{
	pub fn active(self) -> bool
	{
		!matches!(self, ControlState::Released)
	}
}