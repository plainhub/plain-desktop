# 收藏夹 API

## 模型

```graphql
type FavoriteFolder {
  rootPath: String!
  relativePath: String!
  alias: String
}
```

## Query

### `favoriteFolders: [FavoriteFolder!]!`

```graphql
{ favoriteFolders { rootPath relativePath alias } }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ favoriteFolders { rootPath relativePath alias } }"}'
```

## Mutation

### `addFavoriteFolder(rootPath: String!, fullPath: String!): [FavoriteFolder!]!`

plain-app 编址契约：`fullPath` 落在 `rootPath` 之下，返回更新后的整个收藏列表。

```graphql
mutation { addFavoriteFolder(
  rootPath: "/mnt/data"
  fullPath: "/mnt/data/Photos/2024"
) { rootPath fullPath alias } }
```

### `removeFavoriteFolder(fullPath: String!): [FavoriteFolder!]!`

```graphql
mutation { removeFavoriteFolder(fullPath: "/mnt/data/Photos/2024") { fullPath } }
```

### `setFavoriteFolderAlias(fullPath: String!, alias: String!): [FavoriteFolder!]!`

```graphql
mutation { setFavoriteFolderAlias(
  fullPath: "/mnt/data/Photos/2024"
  alias: "Vacation 2024"
) { fullPath alias } }
```
