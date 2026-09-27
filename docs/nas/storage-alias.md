# Storage Volume Aliases

PlainNAS supports user-defined aliases for storage volumes exposed via GraphQL.

Notes:
- `StorageMount.id` is a stable identifier.
  - Local mounted volumes prefer the filesystem UUID: `fsuuid:<uuid>`.
  - Local mounts without a UUID use a best-effort device fallback: `dev:<path>`.
  - Remote mounts use: `remote:<src>`.

- Mutation: set or clear an alias (empty string clears)

```
mutation {
  setMountAlias(id: "fsuuid:XXXX-XXXX", alias: "Media Drive")
}
```

- Query: volumes include an optional `alias` field

```
query {
  mounts { id name alias mountPoint fsType totalBytes }
}
```

Aliases are persisted in the embedded KV DB (`storage:volume_alias`) and survive restarts.
