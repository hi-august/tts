use std::{
    sync::Arc,
    collections::VecDeque,
    time::{Duration, Instant}
};

use axum::{
    Router,
    routing::get,
    http::HeaderValue,
    extract::{Extension,Path},
    response::{IntoResponse, Response},
};

use coerce::actor::scheduler::timer::Timer;
use coerce::actor::system::ActorSystem;
use coerce::actor::LocalActorRef;

use tokio::sync::RwLock;

mod ms_edge;
mod ms_edge_actor;

struct Common {
    actor_ref: LocalActorRef<ms_edge_actor::TTSActor>,
    adaptive_timeout: f64,
    duration_vec: Vec<f64>,
}

async fn tts(Path(text): Path<String>, Extension(common): Extension<Arc<RwLock<Common>>>) -> Response {
    let start = Instant::now();
    let voice: &'static str = "zh-TW-HsiaoChenNeural";
    let content = text.trim_start();
    let timeout = common.read().await.adaptive_timeout;
    let mut audio_vec = match tokio::time::timeout(
        Duration::from_secs_f64(timeout), common.read().await.actor_ref.send(ms_edge_actor::TTSMessage{start, voice, content: content.to_string()})
    ).await {
        Ok(Ok(vv)) => {vv},
        Ok(Err(error)) => {
            println!("send_content error: {:?}", error);
            Vec::new()
        }
        Err(error) => {
            println!("timeout {:.2} error {:?}", timeout, error);
            Vec::new()
        }
    };
    let mut audio_vec_len = audio_vec.len();
    while audio_vec_len < 2048 {
        audio_vec = ms_edge::get_or_init_sample().await;
        tokio::time::sleep(Duration::from_secs_f64(0.01)).await;
    }

    audio_vec_len = audio_vec.len();
    let duration = start.elapsed().as_secs_f64();
    if audio_vec_len == 7488 {
        common.write().await.duration_vec.push(0.0);
    } else {
        common.write().await.duration_vec.push(duration);
    }
    let (avg_duration, duration_vec_len, error_count)= ms_edge::vec_stats(&common.read().await.duration_vec);
    if duration_vec_len > 30 {
        common.write().await.adaptive_timeout = avg_duration * 2.5;
    }
    println!("{:.2},{:.3},{},{},{},{}", duration, avg_duration, error_count, duration_vec_len, content, audio_vec_len);
    let mut response = audio_vec.into_response();
    response.headers_mut().insert("Content-Type", HeaderValue::from_static("audio/mpeg"));
    response
}

async fn ping() -> &'static str{
    "hi"
}

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ms_edge::get_or_init_sample().await;

    let ctx = ActorSystem::new();
    let ws_streams = VecDeque::new();
    let tts_actor = ms_edge_actor::TTSActor{tts: ms_edge::TTS::default(), ws_streams, timeout_vec: Vec::new()};
    let actor_ref = ctx.new_anon_actor(tts_actor).await?;

    Timer::start(
        actor_ref.clone(),
        Duration::from_secs(5),
        ms_edge_actor::WSSMessage{},
    );
    let common = Common{adaptive_timeout: 5.0, actor_ref, duration_vec: Vec::new()};

    let common_rw= Arc::new(RwLock::new(common));
    let app = Router::new()
        .route("/ping", get(ping))
        .route("/tts/:text", get(tts))
        .layer(Extension(common_rw));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:5000")
        .await?;
    println!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
