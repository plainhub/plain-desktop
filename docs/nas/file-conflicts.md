# File operation conflicts (Copy/Paste & Upload)

This document describes how PlainNAS handles name/path conflicts when **copy/paste** and **uploading** into a destination that already contains a file/folder with the same name.

## When a conflict prompt appears

A conflict prompt is shown when the operation would create one or more destination paths that already exist.

- Copy/Paste: checked per selected item against its computed destination path.
- Upload: checked per file being uploaded against its destination path.

## Copy / Paste

### Folder → Folder

Buttons:
- **Merge**
- **Replace**
- **Cancel**

Meaning:
- **Merge**: keep the existing destination folder and copy items into it; if files inside have the same name, they are overwritten.
- **Replace**: delete the existing destination folder path, then copy the folder.
- **Cancel**: abort the paste.

Note:
- Current implementation of **Replace (folder)** uses a hard delete (`deleteFiles`), not Trash.

### File → File (single file)

Buttons:
- **Replace**
- **Keep both**
- **Cancel**

Meaning:
- **Replace**: overwrite the destination file.
- **Keep both**: do not overwrite; PlainNAS will auto-generate a unique filename like `name (1).ext`.
- **Cancel**: abort the paste.

### File → File (multiple files)

Buttons:
- **Replace**
- **Keep both**
- **Skip**
- **Cancel**

Meaning:
- **Replace**: overwrite all conflicting destination files.
- **Keep both**: do not overwrite; conflicting files get unique names.
- **Skip**: do not paste the conflicting files; non-conflicting files still paste.
- **Cancel**: abort the paste.

## Upload

Upload conflict prompts apply to the *newly added batch* of uploads.

### Folder → Folder

If you upload a folder and the destination already contains a folder with the same name, buttons are:
- **Merge**
- **Replace**
- **Cancel**

Meaning:
- **Merge**: upload files into the existing destination folder; if a file path already exists, it is overwritten.
- **Replace**: delete the existing destination folder path first, then proceed.
- **Cancel**: cancel this upload batch.

### File → File (single file)

Buttons:
- **Replace**
- **Keep both**
- **Cancel**

Meaning:
- **Replace**: upload and overwrite.
- **Keep both**: upload without overwriting; PlainNAS will choose a unique name.
- **Cancel**: cancel this upload batch.

### File → File (multiple files)

Buttons:
- **Replace**
- **Keep both**
- **Skip**
- **Cancel**

Meaning:
- **Replace**: overwrite all conflicts.
- **Keep both**: keep existing; upload to unique filenames.
- **Skip**: skip conflicting files; upload the rest.
- **Cancel**: cancel this upload batch.

## Implementation notes (for developers)

- The frontend uses GraphQL `pathStat` / `pathStats` to detect whether a destination path exists and whether it is a directory.
- Upload overwrite behavior is controlled via the `replace` flag passed to the chunk merge mutation.
