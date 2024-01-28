use std::{
    collections::VecDeque,
    time::{Duration, Instant}
};
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};

use futures::StreamExt;
use tokio::sync::RwLock;
use tokio_stream::wrappers::IntervalStream;
use messages::prelude::{Address, RuntimeActorExt};

mod ms_edge;
mod ms_edge_actor;

static mut GLB_DURATION_VEC: Vec<f64> = Vec::new();

struct Common {
    tts_address: Address<ms_edge_actor::TTSActor>,
    adaptive_timeout: f64,
}

impl Common {
    fn default() -> Common {
        let address = ms_edge_actor::TTSActor{ws_streams: VecDeque::new(), tts: ms_edge::TTS::default()}.spawn();
        let tasks = IntervalStream::new(tokio::time::interval(Duration::from_secs(3)))
            .map(|_| ms_edge_actor::WSSMsg);
        address.clone().spawn_stream_forwarder(tasks);
        Common {
            tts_address: address,
            adaptive_timeout: 5.0,
        }
    }
}

#[get("/tts/{text}")]
async fn tts(req: HttpRequest, common: web::Data<RwLock<Common>>) -> impl Responder {
    let start = Instant::now();
    let voice: &'static str = "zh-TW-HsiaoChenNeural";
    let text = req.match_info().query("text").trim_start();
    let mut common_lock = common.write().await;
    let timeout = common_lock.adaptive_timeout;
    let mut mp3_vec = match tokio::time::timeout(
        Duration::from_secs_f64(timeout), common_lock.tts_address.send(ms_edge_actor::TTSMsg{start, voice, text: text.to_string()})
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
    unsafe {
        if mp3_vec.len() == 7488 {
            GLB_DURATION_VEC.push(0.0);
        } else {
            GLB_DURATION_VEC.push(duration);
        }
        let (avg_duration, error_count)= ms_edge::duration_stats(&GLB_DURATION_VEC);
        let duration_vec_len = GLB_DURATION_VEC.len();
        if duration_vec_len > 30 {
            common_lock.adaptive_timeout = avg_duration * 2.5;
        }
        println!("{:.2},{:.3},{},{},{},{}", duration, avg_duration, error_count, duration_vec_len, text, mp3_vec.len());
    }
    HttpResponse::Ok().content_type("audio/mpeg").body(mp3_vec)
}

#[get("/ping")]
async fn ping() -> impl Responder {
    HttpResponse::Ok().body("hi")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    ms_edge::get_sample_vec().await;
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(RwLock::new(Common::default())))
            .service(ping)
            .service(tts)
    })
    .workers(5)
    .worker_max_blocking_threads(1)
    .bind(("0.0.0.0", 5001))?
    .run()
    .await
}
