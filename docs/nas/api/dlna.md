# DLNA API

## 模型

```graphql
type DlnaRenderer {
  udn: String!            # unique device name
  name: String!
  manufacturer: String
  modelName: String
  location: String!       # SSDP location URL
}
```

## Query

### `dlnaRenderers: [DlnaRenderer!]!`

返回当前已发现的 DLNA 渲染器列表。**同时启动 / 续接 background discovery task**，新发现的 renderer 通过 WS `dlna_renderer_found` 事件推送给**当前 client**。

```graphql
{ dlnaRenderers {
  udn name manufacturer modelName location
} }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ dlnaRenderers { udn name location } }"}'
```

dev 模式下，client_id 固定为 `"dev"`，所以**所有 dev 请求共享 discovery 结果**。

## Mutation

### `dlnaCast(rendererUdn: String!, url: String!, title: String!, mime: String!, type: MediaDataType!): Boolean!`

`type` 是 `MediaDataType`（AUDIO/VIDEO/IMAGE/DOC）——投屏只可能是媒体类型；
`DOC` 会显式报错（旧实现接受宽泛的 `DataType` 并把未知值静默当 Video 投，已废除）。

推一个媒体到指定 renderer 播放。

```graphql
mutation { dlnaCast(
  rendererUdn: "uuid:..."
  url: "http://192.168.1.1:8080/media/abc.mp4"
  title: "Big Buck Bunny"
  mime: "video/mp4"
  type: Video
) }
```

底层是 SOAP SetAVTransportURI + Play。**3s 超时**。

错误：

| 错误 | 含义 |
|------|------|
| `renderer not found` | udn 不在缓存里 |
| `soap: ...` | renderer 拒绝或超时 |

## WebSocket 事件

msg_type=7 `dlna_renderer_found`：新 renderer 被 SSDP 搜到时推送
msg_type=8 `dlna_discovery_done`：discovery 阶段结束

**这两个 channel 用 subscribe_with_cid**，所以**只推给触发 discovery 的 client**（dev 模式下全 dev 共享）。

Payload：

```json
// msg 7
{
  "udn": "uuid:...",
  "name": "Living Room TV",
  "manufacturer": "Samsung",
  "modelName": "UN55KS8000",
  "location": "http://192.168.1.50:9197/dmr"
}

// msg 8
{}
```
