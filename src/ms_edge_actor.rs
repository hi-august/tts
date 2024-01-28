use coerce::actor::context::ActorContext;
use coerce::actor::message::{Handler, Message};
use coerce::actor::scheduler::timer::TimerTick;
use coerce::actor::Actor;

use std::time::Instant;
use std::collections::VecDeque;
use futures_util::{SinkExt, StreamExt};

use async_trait::async_trait;

use crate::ms_edge;

#[derive(Clone)]
pub struct TTSMessage {
    pub start: Instant,
    pub voice: &'static str,
    pub text: String
}
#[derive(Clone)]
pub struct WSSMessage {}

pub struct TTSActor {
    pub timeout_vec: Vec<f64>,
    pub tts: ms_edge::TTS,
    pub ws_streams: VecDeque<ms_edge::WebSocketStream>,
}

impl Message for TTSMessage {
    type Result = Vec<u8>;
}

impl Message for WSSMessage {
    type Result = ();
}

impl TimerTick for WSSMessage {}

impl Actor for TTSActor {}

#[async_trait]
impl Handler<TTSMessage> for TTSActor {
    async fn handle(&mut self, msg: TTSMessage, _ctx: &mut ActorContext) -> Vec<u8> {
        // 弹出队列前端的元素
        let duration = msg.start.elapsed().as_secs_f64();
        self.timeout_vec.push(duration);
        let (avg_duration_timeout, _)= ms_edge::duration_stats(&self.timeout_vec);
        println!("channel time {:.2}, {:.3} ws_stream len {}, {:?}", duration, avg_duration_timeout, self.ws_streams.len(), std::thread::current().id());
        if let Some(ws_stream) = self.ws_streams.pop_front() {
            self.tts.send_content(ws_stream, msg.voice, msg.text).await.unwrap_or(Vec::new())
        } else {
            println!("ws_stream not found {}", self.ws_streams.len());
            ms_edge::get_sample_vec().await
        }
    }
}

#[async_trait]
impl Handler<WSSMessage> for TTSActor {
    async fn handle(&mut self, _: WSSMessage, _ctx: &mut ActorContext) {
        // println!("ws_stream {}", self.ws_streams.len());
        if self.ws_streams.len() > 10 {
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
