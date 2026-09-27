# DLNA Casting (TV Cast)

DLNA / UPnP AVTransport casting is picky about the *play URL*. Practical constraints (same idea as PlainAPP):

- Use **HTTP** in LAN (many TVs reject **self-signed HTTPS**)
- Keep URL **short**
- Avoid **query strings / special characters**
- Prefer a simple `name.ext` (some TVs key off the extension)

## PlainNAS behavior

PlainNAS accepts a URL in `dlnaCast(...)`. If the URL is our own file endpoint:

- Input (UI-friendly): `GET /fs?id=<encrypted>`

Then PlainNAS rewrites it to a DLNA-friendly URL:

- Output (TV-friendly): `GET /media/<aliasId>.<ext>`

How it works:

1) Decrypt `id` -> real filesystem path
2) Create an **in-memory alias** (TTL 30min)
3) Send `http://<host>:<http_port>/media/<aliasId>.<ext>` to the renderer

## Requirements

- Must enable `server.http_port` (rewrite needs a reachable HTTP listener)
- TV must reach `<host>:<http_port>` in the LAN
- Only `/fs?id=...` URLs are rewritten; external URLs are passed through

## Troubleshooting checklist

- The URL sent to the TV is `http://.../media/...` (not `https://...`, not `/fs?id=...`)
- `server.http_port` is set and PlainNAS is listening on it
- TV and server are on same network / no firewall blocking the port

## Endpoints

- `/fs?id=...` browser-friendly (encrypted id, supports thumbnails)
- `/media/<aliasId>.<ext>` DLNA-friendly (short, no query, created on cast)
