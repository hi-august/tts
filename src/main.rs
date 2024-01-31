use std::{
    sync::Arc,
    time::Duration
};

use axum::{
    Router,
    routing::get,
    http::HeaderValue,
    extract::{Extension,Path},
    response::{IntoResponse, Response},
};

use tokio::sync::RwLock;
use coerce::sharding::Sharded;

mod ms_edge;
mod ms_edge_actor;

struct Common {
    sharded: Sharded<ms_edge_actor::TTSActor>,
    adaptive_timeout: f64,
    duration_vec: Vec<f64>,
}

async fn tts(Path(text): Path<String>, Extension(common): Extension<Arc<RwLock<Common>>>) -> Response {
    let start = ms_edge::gen_millis();
    let content = text.trim_start();
    let timeout = common.read().await.adaptive_timeout;
    let mut audio_vec = match tokio::time::timeout(
        Duration::from_secs_f64(timeout), common.read().await.sharded.send(ms_edge_actor::TTSMessage{start, content: content.to_string()})
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
        audio_vec = ms_edge::get_init_sample().await;
        audio_vec_len = audio_vec.len();
    }
    {
        let duration = (ms_edge::gen_millis() - start) / 1000.0;
        if audio_vec_len == ms_edge::GLB_AUDIO_SIZE {
            common.write().await.duration_vec.push(0.0);
        } else {
            common.write().await.duration_vec.push(duration);
        }
        let (avg_duration, duration_vec_len, error_count)= ms_edge::vec_stats(&common.read().await.duration_vec);
        if duration_vec_len > 30 {
            common.write().await.adaptive_timeout = avg_duration * 2.5;
        }
        println!("{:.2},{:.3},{},{},{},{}", duration, avg_duration, error_count, duration_vec_len, content, audio_vec_len);
    }
    let mut response = audio_vec.into_response();
    response.headers_mut().insert("Content-Type", HeaderValue::from_static("audio/mpeg"));
    response
}

async fn ping() -> &'static str{
    "hi"
}


#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ms_edge::get_init_sample().await;
    // sharded
    let sharded = ms_edge_actor::get_init_sharded().await;
    let common = Common{adaptive_timeout: 5.0, sharded, duration_vec: Vec::new()};
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
