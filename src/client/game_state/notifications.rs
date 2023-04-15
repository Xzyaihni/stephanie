use enum_amount::EnumCount;


#[derive(Debug, Clone, EnumCount)]
pub enum Notification
{
	PlayerConnected
}

#[derive(Debug, Clone)]
pub struct Notifications
{
	notifications: [bool; Notification::COUNT]
}

impl Notifications
{
	pub fn new() -> Self
	{
		Self{notifications: [false; Notification::COUNT]}
	}

	pub fn set(&mut self, notification: Notification)
	{
		self.notifications[notification.index()] = true;
	}

	pub fn get(&mut self, notification: Notification) -> bool
	{
		let notif = self.notifications.get_mut(notification.index()).unwrap();
		if *notif
		{
			*notif = false;
			true
		} else
		{
			false
		}
	}
}