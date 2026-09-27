# TempValue API

临时键值（瞬时 UI 状态交接）。**设置类持久化请用 prefs.json**（见 `developer.md`），本 API 只剩 `setTempValue`。

## Mutation

### `setTempValue(key: String!, value: String!): TempValue!`

存一个短期键值对，返回 `{key, value}` 方便前端回显。对齐 plain-app `setTempValue`（其 `TempHelper` 是内存实现，我们同样是**内存 map，不落盘**）；消费方是 `/zip?tmp=<key>`（zip 打包流程读一次即消费）。

```graphql
mutation { setTempValue(key: "upload-state", value: "active") { key value } }
```

`TempValue`：

```graphql
type TempValue { key: String! value: String! }
```

## 历史说明

Go 版 plainnas 的 `setKeyValue` / `deleteKeyValue`（任意键写 fjall KV）已删除：plain-app 契约里没有这两个 mutation，web 也不调用；设置类数据统一进 `<data_dir>/prefs.json`（`dataStoreEntries` / `deleteDataStoreEntry` 面，见 `developer.md`）。
