# plain-nas × plain-desktop Rust 复用抽取方案(plain-rs)

> 状态:**已实施**(2026-09-04)。A 组 + B 组全部落地:C 组维持否决。
> 提交:plain-rs `ef39ef0`+`819b8ff`,plain-nas `2c0000d`+`58e92ee`,plain-desktop `2964160`+`5d71efb`。
> 实施决策:B1 采用 plain-rs 裸 MIME + nas 调用点对 text/* 追加 charset(plain-app 本机无 checkout,
> 以 plain-nas 的 Go 镜像表为主并入);B2 统一为 `name_1.ext`(plain-desktop 风格),抽为
> `plain_rs::utils::unique_path::unique_sibling`;B3 TIFF 尺寸 + EXIF GPS 移入
> `plain_rs::utils::image_dimensions`(新增 `exif_gps`)。
> 范围:把 plain-nas / plain-desktop 中可共用的 Rust 原语抽到 `plain-rs` crate
> (`../plain-rs`,git: `plainhub/plain-rs`),两个项目共用单一来源。

## 背景

- 已在 plain-rs 中共用(本方案之前):crypto(ECDH / Ed25519 / XChaCha20-Poly1305 /
  ChaCha20-Poly1305)、base64、hex、mime、query(percent-decode / parse_query /
  url_encode)、CORS 常量、short_uuid、image_dimensions、mdns;以及 2026-09-03
  迁入的 http_url、shortid、hostname、ifaddr、async_read_stream、hash::sha512_hex。
- plain-desktop 的本地服务是手写 HTTP/1.1 栈(裸 tokio TCP + 自解析请求 +
  tokio-rustls + rusqlite);plain-nas 是 axum + axum-server + sled。凡是绑定
  各自 HTTP 栈或存储层的代码**不可**抽取;候选仅限与传输、存储无关的协议 /
  解析 / 编码原语。

## A 组 — 直接抽取(低风险)

| # | 抽到 plain-rs | 来源(两边现状) | 说明 |
|---|---|---|---|
| A1 | `tls::ensure_self_signed_pem(cert_path, key_path, san_names)` | desktop `local/tls.rs::ensure_cert`(75 行,存在即加载)+ nas `tls_gen.rs::make_self_signed`(27 行) | 合并为"存在即加载、不存在则生成";SAN 列表由调用方传入(desktop: `localhost` + `127.0.0.1`;nas: `plainnas.local` + `localhost`)。**TLS acceptor 留在各项目** — desktop 用 tokio-rustls,nas 用 axum-server。需给 plain-rs 加 `rcgen` 依赖。 |
| A2 | `ws_frame::encode_frame / decode_frame / verify_handshake` | desktop `local/graphql/context.rs::encode_ws_event` + nas `src/ws_hub.rs` 编解码 | 两边线协议本就相同:`4 字节 i32 BE msg_type ‖ XChaCha(token, JSON)`,外加"首条二进制帧必须能用 token 解密,否则断开"的握手。抽成共享编解码器(payload 传字节,不依赖 serde)。事件类型整数常量留在各项目(两边编号不同,**不能合并**)。此模块兼作协议契约文档。 |
| A3 | `http::parse_range_header(header, file_size) -> Option<(start, end)>` | desktop `local/server/file_server.rs::parse_range_header`(36 行,单测完整)→ 移入;nas `api/fs.rs` 的内联 Range 解析改为调用 | desktop 版本支持后缀范围(`bytes=-500`);nas 目前不支持 — 采纳后顺带修复(行为改进,提交说明中标注)。 |
| A4 | `http::content_disposition(kind, filename)` | desktop `file_server.rs` 与 nas `api/fs.rs` 各自手拼 `filename="…"; filename*=utf-8''…`(RFC 5987) | 去重进 plain-rs,基于 `query::url_encode` 实现。nas 侧有 Go 兼容的 `+`→`%20` 细节需保留。 |
| A5 | 采纳 `plain_rs::crypto::gen_token` | nas `db/url_token.rs` 内联"32 随机字节 + base64"(4 行) | 不新增模块,纯删代码;sled 持久化与 Go token 迁移逻辑留在 nas。 |

A 组全部是 std-only 或复用 plain-rs 已有依赖(A1 新增 `rcgen`,两个项目本来就
都带着)。预计工作量:一天以内。每个模块带单测迁入。

## B 组 — 值得抽,但需要先拍板

| # | 内容 | 需要决策的点 |
|---|---|---|
| B1 | **统一 MIME 表**:nas `fsx.rs::guess_mime`(~80 行表,text 类带 `; charset=utf-8`)与 desktop 用的 `plain_rs::mime::mime_from_ext` 是两份表 | charset 策略:建议 plain-rs 只存裸 MIME,charset 由 nas 调用点对 `text/*` 追加;nas 行为有细微变化(仅 text 类型)。 |
| B2 | **统一重名冲突命名**:nas `file_tasks::unique_path` 生成 `name (1).ext`,desktop `unique_sibling` 生成 `name_1.ext` | 两种格式都是**可观察行为**(回收站恢复文件名 / 上传重名文件名)。要么参数化风格共存,要么统一成一种(会改变一边的产物命名)。建议参数化,不急。 |
| B3 | **TIFF 尺寸解析 + EXIF GPS 读取**(desktop `file_query.rs`,~70 行手写)→ 移入 `plain_rs::utils::image_dimensions` | 目前只有 desktop 用(plain-rs 的 image_dimensions 不支持 TIFF)。属于原则对齐而非去重,顺手做即可。 |

## C 组 — 已考虑并否决(不要抽)

记录在案,避免没有新事实时被重复提出:

- **手写 multipart 解析器**(desktop,~125 行):只有裸 socket 服务器需要;
  nas 走 `axum::extract::Multipart`。没有共同消费方。
- **服务器引导 / TLS acceptor / 端口回退**:axum-server vs 裸 accept loop,
  两套架构,共享只会造出配置地狱。
- **device_info**:nas 直读 `/proc`(Linux-only);desktop 用 sysinfo crate +
  macOS `pmset`/`sysctl`。目标平台不同。
- **log**:nas 是 240 行 stderr 迷你日志;desktop 用 tauri-plugin-log。
- **chunked upload / zip / DB 层 / DLNA**:协议与存储都不同;DLNA 角色相反
  (nas 是 renderer/发起端,desktop 是 receiver 端)。这些是业务代码,不是原语。
- **GraphQL executor 封装**(desktop `executor.rs`):太薄,不配进 plain-rs;
  其内部重复属于 desktop 自己的小清理。

## 执行顺序与验证

1. A1 → A5 逐个进 plain-rs(每个带单测,`cargo test` 把关),每落地一项同步改
   plain-nas / plain-desktop 调用点;
2. plain-nas:`cargo check` + `cargo test`(快);plain-desktop:只跑
   `cargo check`(Tauri 编译慢,测试套件走它自己的 CI 节奏);
3. B 组等上面三个决策点拍板后再做;
4. 全部完成后:plain-rs 提交并 push → plain-nas 的 path 依赖切 git 形态
   (写法已备在 plain-nas `Cargo.toml` 注释里);两个消费方仓库各自提交。

风险分级:A3 / A4 / A5 是纯函数且有现成测试(最低);A1 / A2 涉及文件系统与
协议行为 — 实现必须保持各项目现有行为逐字节不变(A2 只替换等价实现,不改线格式)。
