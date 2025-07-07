use std::{
    thread::{self, JoinHandle},
    ops::ControlFlow
};

use crate::common::{
    MessagePasser,
    message::Message
};


pub fn receiver_loop<F, D>(
    mut messager: MessagePasser,
    mut on_message: F,
    on_close: D
) -> JoinHandle<()>
where
    F: FnMut(Message) -> ControlFlow<()> + Send + 'static,
    D: FnOnce() + Send + 'static
{
    thread::spawn(move ||
    {
        loop
        {
            match messager.receive()
            {
                Ok(messages) =>
                {
                    let flow = messages.into_iter().try_for_each(&mut on_message);
                    if let ControlFlow::Break(_) = flow
                    {
                        on_close();
                        return;
                    }
                },
                Err(err) =>
                {
                    eprintln!("error receiving message: {err}");

                    on_close();
                    return;
                }
            }
        }
    })
}
