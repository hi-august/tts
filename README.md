# tts

### tokio-tungstenite连接到微软服务
### actix-web直接响应Vec<u8>
####
    1. String,&str相互转化
    2. Result错误处理
    3. mut可变变量
    4. unwrap,?可以获取Result,Option的值
    5. 使用异步websocket库,tokio_tungstenite
    6. tokio::time::timeout在rust-websocket同步库无法使用,rev会阻塞整个线程
    7. async声明异步执行,await等待响应
    8. 添加actor,生产ws_stream加速访问,不可复用报错

