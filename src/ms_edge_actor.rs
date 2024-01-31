use std::{collections::VecDeque, time::Duration};

use coerce::actor::{
    context::ActorContext, Actor, new_actor_id, ActorCreationErr, ActorFactory, ActorRecipe,
};
use coerce::actor::message::Handler;
use coerce::actor::system::ActorSystem;
use coerce::actor::scheduler::timer::{Timer, TimerTick};
use coerce::remote::system::{RemoteActorSystem, NodeId};
use coerce::remote::net::server::RemoteServer;
use coerce::sharding::{Sharding, Sharded};
use coerce::persistent::Persistence;
use coerce::persistent::journal::provider::inmemory::InMemoryStorageProvider;
use coerce_macros::JsonMessage;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::ms_edge;

pub struct TTSActor {
    pub timeout_vec: Vec<f64>,
    pub tts: ms_edge::TTS,
    pub ws_streams: VecDeque<ms_edge::WebSocketStream>,
}

#[derive(JsonMessage, Serialize, Deserialize)]
#[result("Vec<u8>")]
pub struct TTSMessage {
    pub content: String,
    pub start: f64,
}

#[derive(JsonMessage, Serialize, Deserialize)]
#[derive(Clone)]
#[result("()")]
pub struct WSSMessage {}

impl TimerTick for WSSMessage {}

#[async_trait]
impl Actor for TTSActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("TTSActor is_starting {:?}", ctx.is_starting());
        println!("Starting sched tasks");
        let actor_ref = self.actor_ref(ctx);
        Timer::start(
            actor_ref,
            Duration::from_secs(6),
            WSSMessage{},
        );
    }
}

#[async_trait]
impl Handler<TTSMessage> for TTSActor {
    async fn handle(&mut self, msg: TTSMessage, _: &mut ActorContext) -> Vec<u8> {
        // 弹出队列前端的元素
        let duration = (ms_edge::gen_millis() - msg.start) / 1000.0;
        self.timeout_vec.push(duration + 0.00001);
        let (avg_timeout, _, _)= ms_edge::vec_stats(&self.timeout_vec);
        println!("channel {:.2},{:.3},ws_stream len {},{:?}", duration.clone(), avg_timeout, self.ws_streams.len(), std::thread::current().id());
        if let Some(ws_stream) = self.ws_streams.pop_front() {
            self.tts.send_content(ws_stream, ms_edge::GLB_TTS_VOICE, msg.content).await.unwrap_or(Vec::new())
        } else {
            println!("channel ws_stream not found {}", self.ws_streams.len());
            Vec::new()
        }
    }
}

#[async_trait]
impl Handler<WSSMessage> for TTSActor {
    async fn handle(&mut self, _: WSSMessage, _ctx: &mut ActorContext) {
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

// sharded
pub async fn get_init_sharded() -> Sharded<TTSActor> {
    let actor_id = new_actor_id();
    let listen_addr = "127.0.0.1:6000";
    let persistence = Persistence::from(InMemoryStorageProvider::new());
    let (remote, _) = create_system(persistence.clone(), listen_addr, 1, None).await;
    let sharding = Sharding::<TTSActorFactory>::builder(remote.clone())
        .build()
        .await;
    let sharded = sharding.get(actor_id.clone(), Some(TTSActorRecipe));
    let start = ms_edge::gen_millis();
    let _ = sharded.send(TTSMessage{start, content: "喔".to_string()}).await;
    sharded
}

pub async fn create_system(
    persistence: Persistence,
    listen_addr: &str,
    node_id: NodeId,
    seed_addr: Option<&str>,
) -> (RemoteActorSystem, RemoteServer) {
    let system = ActorSystem::new().to_persistent(persistence);
    let remote = RemoteActorSystem::builder()
        .with_actor_system(system)
        .with_tag(format!("node-{node_id}"))
        .with_actors(|a| {
            a.with_actor(TTSActorFactory)
                .with_handler::<TTSActor, TTSMessage>("TTSActor:TTSMessage")
        })
        .with_id(node_id)
        .build()
        .await;

    let mut server = remote.clone().cluster_worker().listen_addr(listen_addr);
    if let Some(seed_addr) = seed_addr {
        server = server.with_seed_addr(seed_addr);
    }
    let server = server.start().await;
    (remote, server)
}

pub struct TTSActorRecipe;

impl ActorRecipe for TTSActorRecipe {
    fn read_from_bytes(_bytes: &Vec<u8>) -> Option<Self> {
        Some(Self)
    }
    fn write_to_bytes(&self) -> Option<Vec<u8>> {
        Some(vec![])
    }
}

#[derive(Clone)]
pub struct TTSActorFactory;

#[async_trait]
impl ActorFactory for TTSActorFactory {
    type Actor = TTSActor;
    type Recipe = TTSActorRecipe;

    async fn create(&self, _: TTSActorRecipe) -> Result<TTSActor, ActorCreationErr> {
        Ok(Self::Actor{
            tts: ms_edge::TTS::default(), ws_streams: VecDeque::new(), timeout_vec: Vec::new()
        })
    }
}
