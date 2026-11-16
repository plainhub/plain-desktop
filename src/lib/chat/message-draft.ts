export type ChatTextFormat = 'PLAIN' | 'MARKDOWN'
export type ChatAttachmentType = 'IMAGE' | 'FILE'
export type DraftAttachment = { type: ChatAttachmentType; file: File }
