# 阶段一：构建 Rust 应用程序
FROM rust:alpine as builder
WORKDIR /app
COPY . .
RUN apk add libressl-dev musl-dev
RUN cargo build --release
 
# 阶段二：创建最终镜像
FROM alpine:latest
WORKDIR /app
COPY --from=builder /app/target/release/tts .
CMD ["./tts"]
