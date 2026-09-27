# Office documents (DOC/XLS/PPT) preview

PlainNAS supports read-only preview for common Microsoft Office document formats by converting them to PDF on demand and letting the browser render the PDF.

## Supported formats

- Word: `.doc`, `.docx`
- Excel: `.xls`, `.xlsx`
- PowerPoint: `.ppt`, `.pptx`

The preview is always served as a PDF.

## How it works

- The frontend opens a file via the public `/fs` endpoint.
- When requesting a preview, it calls:

  - `GET /fs?id=<encryptedFileId>&preview=pdf`

- The backend:
  - decrypts the `id` to a real filesystem path
  - runs LibreOffice in headless mode to convert the source file to PDF
  - caches the generated PDF under `DATA_DIR/preview_pdf/`
  - serves the cached PDF via `/fs` (with `Content-Disposition: inline` by default)

### Cache behavior

- Cache key includes: `path + mtime + size`.
- If the source file changes, a new preview file is generated automatically.
- Concurrent requests for the same file/key are deduplicated with an in-process lock.

## Requirements (optional component)

Office preview requires LibreOffice (provides `soffice` / `libreoffice`).

- Default install does **not** install LibreOffice (it is large).
- To install it during setup:

  - `plain-nas install --with-libreoffice`

## UI behavior / fallback

- If LibreOffice is available (`app.features` contains `DOC_PREVIEW`), clicking an Office file opens the PDF preview in a new tab.
- If LibreOffice is **not** available, clicking an Office file falls back to **download** (no preview attempt).

## Troubleshooting

- If you see errors like “LibreOffice is required for DOC/DOCX preview”, install LibreOffice or reinstall with `--with-libreoffice`.
- On some systems, `soffice` is not in `PATH` for systemd services; the backend probes common absolute paths, but a standard package install is still recommended.
