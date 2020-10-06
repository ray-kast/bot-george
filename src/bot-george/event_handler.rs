use log::*;
use serenity::{async_trait, client::Context, model::channel::Message};

pub struct Handler;
pub struct Framework;

impl serenity::client::EventHandler for Handler {}

// TODO: this is here because async closures are unstable
macro_rules! stupid_try {
    ($x:expr) => {
        match $x {
            Ok(o) => o,
            Err(e) => {
                error!("{}", e);
                return;
            },
        }
    };
}

#[async_trait]
impl serenity::framework::Framework for Framework {
    async fn dispatch(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            info!("Ignoring message from bot {:?}", msg.author);
            return;
        }

        stupid_try!(
            msg.channel_id
                .send_message(ctx.http, |m| m.content("bruh"))
                .await
        );
    }
}
