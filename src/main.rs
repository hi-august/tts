use std::{
    sync::Arc,
    collections::VecDeque,
    time::{Duration, Instant}
};

use coerce::actor::scheduler::timer::Timer;
use coerce::actor::system::ActorSystem;
use coerce::actor::LocalActorRef;
// use coerce::actor::worker::Worker;

use axum::{
    Router,
    routing::get,
    extract::{Extension,Path},
    response::{IntoResponse, Response},
};
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
    let text = text.trim_start();
    let mut common_rw = common.write().await;
    let timeout = common_rw.adaptive_timeout;
    let mut mp3_vec = match tokio::time::timeout(
        Duration::from_secs_f64(timeout), common_rw.actor_ref.send(ms_edge_actor::TTSMessage{start, voice, text: text.to_string()})
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
    while mp3_vec.len() < 2048 {
        mp3_vec = ms_edge::get_sample_vec().await;
        tokio::time::sleep(Duration::from_secs_f64(0.01)).await;
    }

    let duration = start.elapsed().as_secs_f64();
    if mp3_vec.len() == 7488 {
        common_rw.duration_vec.push(0.0);
    } else {
        common_rw.duration_vec.push(duration);
    }
    let (avg_duration, error_count)= ms_edge::duration_stats(&common_rw.duration_vec);
    let duration_vec_len = common_rw.duration_vec.len();
    if duration_vec_len > 30 {
        common_rw.adaptive_timeout = avg_duration * 2.5;
    }
    println!("{:.2},{:.3},{},{},{},{}", duration, avg_duration, error_count, duration_vec_len, text, mp3_vec.len());
    let mut response = mp3_vec.into_response();
    response.headers_mut().insert("Content-Type", "audio/mpeg".parse().unwrap());
    response
}

async fn ping() -> &'static str{
    "hi"
}

// #[tokio::main]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ms_edge::get_sample_vec().await;

    let ctx = ActorSystem::new();
    let ws_streams = VecDeque::new();
    let tts_actor = ms_edge_actor::TTSActor{tts: ms_edge::TTS::default(), ws_streams: ws_streams, timeout_vec: Vec::new()};
    // let mut worker_ref = Worker::new(tts_actor, 4, "worker", &mut ctx).await?;
    let actor_ref = ctx.new_anon_actor(tts_actor).await?;

    Timer::start(
        actor_ref.clone(),
        Duration::from_secs(3),
        ms_edge_actor::WSSMessage{},
    );
    let common = Common{adaptive_timeout: 5.0, actor_ref, duration_vec: Vec::new()};

    let common_rw= Arc::new(RwLock::new(common));
    let app = Router::new()
        .route("/ping", get(ping))
        .route("/tts/:text", get(tts))
        .layer(Extension(common_rw));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:5001")
        .await?;
    println!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
