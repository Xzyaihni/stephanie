use std::{
    thread,
    ops::ControlFlow
};

use crate::common::{
    MessagePasser,
    message::Message
};


pub fn receiver_loop<F, D>(mut messager: MessagePasser, mut on_message: F, on_close: D)
where
    F: FnMut(Message) -> ControlFlow<()> + Send + 'static,
    D: FnOnce() + Send + 'static
{
    thread::spawn(move ||
    {
        loop
        {
            if let Ok(messages) = messager.receive()
            {
                match messages.into_iter().try_for_each(&mut on_message)
                {
                    ControlFlow::Break(_) =>
                    {
                        on_close();
                        return;
                    },
                    _ => ()
                }
            } else
            {
                on_close();
                return;
            }
        }
    });
}
