use std::{collections::VecDeque, time::Duration};

use coerce::actor::{
    context::ActorContext, Actor, new_actor_id, LocalActorRef,
};
use coerce::actor::message::{Handler, Message};
use coerce::actor::system::ActorSystem;
use coerce::actor::scheduler::ActorType;
use coerce::actor::scheduler::timer::{Timer, TimerTick};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};

use crate::edge;

pub struct TTSActor {
    pub timeout_vec: Vec<f64>,
    pub tts: edge::TTS,
    pub ws_streams: VecDeque<edge::WebSocketStream>,
}

#[derive(Clone)]
pub struct TTSMessage {
    pub content: String,
    pub start: f64,
    pub timeout: f64,
}

#[derive(Clone)]
pub struct WSSMessage {}

impl Message for TTSMessage {
    type Result = Vec<u8>;
}

impl Message for WSSMessage {
    type Result = ();
}

impl TimerTick for WSSMessage {}

#[async_trait]
impl Actor for TTSActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        let actor_ref = self.actor_ref(ctx);
        Timer::start_immediately(
            actor_ref,
            Duration::from_secs(13),
            WSSMessage{},
        );
    }
}

#[async_trait]
impl Handler<TTSMessage> for TTSActor {
    async fn handle(&mut self, msg: TTSMessage, _: &mut ActorContext) -> Vec<u8> {
        let duration = (edge::gen_millis() - msg.start) / 1000.0;
        if duration > msg.timeout {
            println!("channel timeout {:.2},ws_stream len {},{:?}", duration.clone(), self.ws_streams.len(), std::thread::current().id());
            // 弹出队列后端的元素
            if let Some(ws_stream) = self.ws_streams.pop_back() {
                let (mut writer, _) = ws_stream.split();
                let _ = writer.close().await;
            }
            return Vec::new()
        }
        self.timeout_vec.push(duration + 0.00001);
        let (avg_timeout, _, _)= edge::vec_stats(&self.timeout_vec);
        println!("channel {:.2},{:.3},ws_stream len {},{:?}", duration.clone(), avg_timeout, self.ws_streams.len(), std::thread::current().id());
        // 弹出队列前端的元素
        if let Some(ws_stream) = self.ws_streams.pop_front() {
            self.tts.send_content(ws_stream, edge::GLB_TTS_VOICE, msg.content).await.unwrap_or(Vec::new())
        } else {
            Vec::new()
        }
    }
}

#[async_trait]
impl Handler<WSSMessage> for TTSActor {
    async fn handle(&mut self, _: WSSMessage, _ctx: &mut ActorContext) {
        if self.ws_streams.len() > 0 {
            // 弹出队列末尾的元素
            if let Some(ws_stream) = self.ws_streams.pop_back() {
                let (mut writer, _) = ws_stream.split();
                let _ = writer.close().await;
            }
        }
        match self.tts.connect().await {
            Ok(vv) => {
                // 在队列的前端添加元素
                self.ws_streams.push_front(vv);
            }
            Err(error) => {
                println!("connect error {:?}", error);
            }
        };
    }
}

// actors_ref
pub async fn get_tts_actors() -> VecDeque<LocalActorRef<TTSActor>> {
    let mut tts_actors = VecDeque::new();
    println!("TTSActor starting");
    for _ in 0..12 {
        let actor_id = new_actor_id();
        let ctx = ActorSystem::new();
        let actor = TTSActor{tts: edge::TTS::default(), ws_streams: VecDeque::new(), timeout_vec: Vec::new()};
        let actor_ref = ctx.new_actor_deferred(actor_id, actor, ActorType::Tracked).await;
        tts_actors.push_front(actor_ref)
    }
    tts_actors
}
