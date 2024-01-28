use std::time::Instant;
use std::collections::VecDeque;

use futures_util::{SinkExt, StreamExt};
use messages::prelude::{Actor, Context, Handler, Notifiable, async_trait};

use crate::ms_edge;
static mut GLB_TIMEOUT_VEC: Vec<f64> = Vec::new();

pub struct WSSMsg;
pub struct TTSMsg {
    pub start: Instant,
    pub voice: &'static str,
    pub text: String
}

pub struct TTSActor {
    pub tts: ms_edge::TTS,
    pub ws_streams: VecDeque<ms_edge::WebSocketStream>
}

#[async_trait]
impl Actor for TTSActor {
    async fn started(&mut self) {
        println!("TTSActor has started")
    }
}

#[async_trait]
impl Notifiable<WSSMsg> for TTSActor {
    async fn notify(&mut self, _: WSSMsg, _: &Context<Self>) {
        if self.ws_streams.len() > 2 {
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

#[async_trait]
impl Handler<TTSMsg> for TTSActor {
    type Result = Vec<u8>;
    async fn handle(&mut self, msg: TTSMsg, _: &Context<Self>) -> Self::Result {
        // 弹出队列前端的元素
        let duration = msg.start.elapsed().as_secs_f64();
        unsafe {
            GLB_TIMEOUT_VEC.push(duration);
            let (avg_duration_timeout, _)= ms_edge::duration_stats(&GLB_TIMEOUT_VEC);
            println!("channel time {:.2}, {:.3} ws_stream len {}, {:?}", duration, avg_duration_timeout, self.ws_streams.len(), std::thread::current().id());
        }
        if let Some(ws_stream) = self.ws_streams.pop_front() {
            self.tts.send_content(ws_stream, msg.voice, msg.text).await.unwrap_or(Vec::new())
        } else {
            println!("ws_stream not found {}", self.ws_streams.len());
            ms_edge::get_sample_vec().await
        }
    }
}
