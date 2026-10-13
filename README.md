# Plain Desktop

This is desktop app and web ui source code of [PlainApp](https://github.com/plainhub/plain-app).

PlainApp is an open-source app that lets you securely manage your phone from a web browser. Access files, media, contacts, SMS, calls, and more through a simple, easy-to-use interface on your desktop.

## Features

**Privacy First**
- All data stays on your device — no cloud, no third-party storage
- No Firebase Messaging or Analytics; only crash logs (optional) via Firebase Crashlytics
- Secured with TLS + XChaCha20-Poly1305 encryption

**Ad-Free, Always**
- 100% ad-free experience, forever

**Clean, Modern Interface**
- Minimalist and customizable UI
- Supports multiple languages, light/dark themes

**Web-Based Desktop Management**  
Access a self-hosted webpage on the same network to manage your phone:
- Files: Internal storage, SD card, USB, images, videos, audio
- Contacts, SMS, and call logs
- Installed apps (with APK export)
- Notifications and device info
- Screen mirroring
- PWA support — add the web app to your desktop/home screen

**Built-in Tools**
- Markdown note-taking
- RSS reader with clean UI
- Video and audio player (in-app and on the web)
- TV casting for media

**Always Improving**
- More features are on the way

PlainApp is designed with simplicity in mind, so you can focus on what matters most: your data.

Video: https://www.youtube.com/watch?v=TjRhC8pSQ6Q

Reddit: https://www.reddit.com/r/plainapp

Discord: https://discord.gg/RQWcS6DEEe

QQ Group: 812409393

## Work on plain-core

Keep [plain-core](https://github.com/plainhub/plain-core) next to this repository. Run from this repository root:

```sh
./scripts/core local
```

Use local plain-core changes and build normally; no push is needed. To stop using the local checkout: `./scripts/core local --off`.

```sh
./scripts/core update
```

Stop using local paths and lock this project to the latest commit on GitHub's plain-core/main. Commit the updated `Cargo.lock`.

## Run

Duplicate `.env.example` file to `.env.local` and update the `VITE_APP_API_HOST`.

```sh
yarn
yarn dev
yarn dev:tauri
```

## Build

```sh
yarn build
```

### Lint

```sh
yarn lint
```


### Resources

1. https://icon-sets.iconify.design/material-symbols/
