use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::{USER_AGENT, HeaderValue};

pub type WebSocketStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone)]
pub struct TTS {
    endpoint: &'static str,
    speech: &'static str,
    uuid: String,
}

impl TTS {
    pub fn default() -> TTS {
        TTS {
            endpoint: "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1",
            speech: r#"{"context":{"synthesis":{"audio":{"metadataoptions":{"sentenceBoundaryEnabled":"false","wordBoundaryEnabled":"false"},"outputFormat":"audio-24khz-48kbitrate-mono-mp3"}}}}"#,
            uuid: gen_uuid(),
        }
    }
    pub async fn connect(&self) -> Result<WebSocketStream, Box<dyn std::error::Error>> {
        // 构造带有 TrustedClientToken 和 X-ConnectionId 参数的 WebSocket URL
        let mut url = String::from(self.endpoint);
        url.push_str("?TrustedClientToken=6A5AA1D4EAFF4E9FB37E23D68491D6F4");
        url.push_str(format!("&X-ConnectionId={}", self.uuid).as_str());
        // 将 URL 转换为 WebSocket 请求
        let mut req = url.into_client_request()?;
        req.headers_mut().append(
            "Origin",
            HeaderValue::from_static("chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"),
        );
        req.headers_mut().append(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36 Edg/119.0.0.0"));
        // 发送 WebSocket 请求并将结果流拆分为写入器和读取器
        let (ws_stream, _) = connect_async(req).await?;
        Ok(ws_stream)
    }
    fn get_ssml(&self, voice: &'static str, text: String) -> String {
        format!(
            r#"
            <speak version="1.0" xmlns="https://www.w3.org/2001/10/synthesis" xml:lang="en-US">
                <voice name="{}">
                {}
                </voice>
            </speak>
        "#,
            voice, text
        )
    }
    pub async fn send_content(&self, ws_stream: WebSocketStream, voice: &'static str, text: String) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let ssml = self.get_ssml(voice, text);
        Ok(self.send_ssml(ws_stream, ssml).await?)
    }
    pub async fn send_ssml(&self, ws_stream: WebSocketStream, ssml: String) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // 将 WebSocket 拆分为写入器和读取器
        let (mut writer, mut reader) = ws_stream.split();
        let millis = gen_millis();
        let config_msg = format!(
            "X-Timestamp: {:?}\r\nContent-Type: application/json; charset=utf-8\r\nPath: speech.config\r\n\r\n{}",
            millis, self.speech
        );
        writer.send(config_msg.into()).await?;
        let content_msg = format!(
            "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{:?}\r\nPath:ssml\r\n\r\n{}",
            self.uuid, millis, ssml
        );
        writer.send(content_msg.into()).await?;
        let mut audio_vec: Vec<u8> = Vec::new();
        let pat = "Path:audio\r\n".as_bytes().to_vec();
        while let Some(data) = reader.next().await {
            match data {
                Ok(data) => {
                    if data.is_binary() {
                        let all = data.into_data();
                        if let Some(index) = all.windows(pat.len()).position(|window| window == &pat[..]) {
                            audio_vec.extend_from_slice(&all[index + pat.len()..]);
                        }
                    } else if data.is_text() && data.into_text()?.contains("Path:turn.end") {
                        break;
                    }
                }
                Err(e) => eprintln!("websocket error: {e:?}"),
            }
        }
        writer.close().await?;
        Ok(audio_vec)
    }
}

fn gen_uuid() -> String {
    Uuid::new_v4().to_string().to_uppercase()
}

pub fn gen_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime::now panic")
        .as_millis()
}

pub fn vec_stats(nums: &[f64]) -> (f64, usize, usize) {
    let sum: f64 = nums.iter().sum();
    let error_count = nums.iter().filter(|&&x| x == 0.0).count();
    let length = nums.len() - error_count;
    (sum / length as f64, length as usize, error_count)
}

pub async fn get_or_init_sample() -> Vec<u8> {
    let sample: &'static str = "/tmp/sample.mp3";
    let voice: &'static str = "zh-TW-HsiaoChenNeural";
    if !Path::new(&sample).exists() {
        let tts = TTS::default();
        let ws_stream = match tts.connect().await {
            Ok(vv) => vv,
            Err(error) => {
                println!("connect: {:?}", error);
                return Vec::new()
            }
        };
        let audio_vec = match tts.send_content(ws_stream, voice, "嗯".to_string()).await {
            Ok(vv) => vv,
            Err(error) => {
                println!("send_content: {:?}", error);
                return Vec::new()
            }
        };
        if audio_vec.len() == 7488 {
            println!("save sample file!");
            let _ = fs::write(sample, &audio_vec);
        }
        return audio_vec;
    } else {
        fs::read(sample).unwrap_or(Vec::new())
    }
}
