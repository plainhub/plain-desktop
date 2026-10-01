# 书签 API

> 端口：HTTP :8080 / HTTPS :8443，`POST /graphql`（Bearer url_token）。
> 契约对齐 plain-app（手机端）书签面；存储为 SQLite `<data_dir>/plain.db` 的
> `bookmarks` / `bookmark_groups` 表（plain-rs `chat::db::bookmark`，与
> plain-desktop 共用同一套行为代码）。

## 模型

```graphql
type Bookmark {
  id: ID!
  url: String!
  title: String!          # 新建时 = url（页面标题抓取为手机端能力，NAS 暂无）
  faviconPath: String!    # 恒为 ""（favicon 抓取未实现，UI 显示占位图标）
  groupId: ID!            # "" 表示未分组
  pinned: Boolean!
  clickCount: Int!
  lastClickedAt: String   # RFC3339 UTC；从未点击为 null
  sortOrder: Int!
  createdAt: Instant!     # RFC3339 UTC 标量
  updatedAt: Instant!
}

type BookmarkGroup {
  id: ID!
  name: String!
  collapsed: Boolean!
  sortOrder: Int!
  itemCount: Int!        # 组内书签实时条数（无则 0）
  createdAt: Instant!
  updatedAt: Instant!
}

input BookmarkInput {
  url: String!
  title: String!
  groupId: String!
  pinned: Boolean!
  sortOrder: Int!
}
```

## Query

```graphql
{ bookmarks { id url title groupId pinned clickCount lastClickedAt sortOrder createdAt updatedAt } }
{ bookmarkGroups { id name collapsed sortOrder createdAt updatedAt } }
```

排序与手机端 DAO 一致：书签 `pinned DESC, sortOrder ASC, createdAt ASC`；分组 `sortOrder ASC, createdAt ASC`。

## Mutation

| Mutation | 语义 |
|---|---|
| `addBookmarks(urls: [String!]!, groupId: String!): [Bookmark!]!` | 批量新建；URL 去空白、跳过空串；title 初始 = url；pinned=false |
| `updateBookmark(id: ID!, input: BookmarkInput!): Bookmark!` | 全量更新；id 不存在报错 |
| `deleteBookmarks(ids: [ID!]!): ActionResult!` | 批量删除；`affectedCount` = 实际存在的 id 数 |
| `recordBookmarkClick(id: ID!): Boolean!` | clickCount+1、lastClickedAt=now；id 不存在静默成功 |
| `createBookmarkGroup(name: String!): BookmarkGroup!` | 新建分组；collapsed=false、sortOrder=0 |
| `updateBookmarkGroup(id: ID!, name: String!, collapsed: Boolean!, sortOrder: Int!): BookmarkGroup!` | 更新分组；itemCount 保持实时；不存在报错 |
| `deleteBookmarkGroup(id: ID!): Boolean!` | 删分组并把成员书签移入未分组（groupId=""），与手机端一致 |

示例：

```graphql
mutation { addBookmarks(urls: ["https://example.com"], groupId: "") { id url title } }
mutation { updateBookmark(id: "xxx", input: { url: "https://example.com", title: "Example", groupId: "", pinned: true, sortOrder: 0 }) { id pinned } }
mutation { recordBookmarkClick(id: "xxx") }
```

## 与手机端的差异

- **页面标题 / favicon 抓取未实现**：手机端新增书签后异步抓取 og:title 与 favicon 并经 WS 推送 `bookmark_updated`；NAS 端 title 保持 = url、faviconPath 恒 ""，用户可手动改名。web 端编辑/删除均本地乐观更新，不依赖该推送。
- 时间戳为 RFC3339 字符串（NAS 惯例），手机端为 `Instant`；线上均为 JSON 字符串/数字，客户端无感。
