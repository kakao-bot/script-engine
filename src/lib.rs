use kakao_loco_client::prelude::*;

pub trait Script {
    fn on_message(&mut self, message: &Message<'_>) -> Option<String>;
}
