import type { Ref } from 'vue'
import type { ChatCaptureDestination } from '@/lib/screen-capture/tauri-capture-adapter'

interface CapturedMmsPort {
  currentDestination: () => ChatCaptureDestination
  messageBody: Ref<string>
  pendingFiles: Ref<File[]>
  sendMessage: () => Promise<boolean>
}

function requireNonBlank(value: string, field: string): void {
  if (!value.trim()) throw new Error(`capture ${field} is required`)
}

export function snapshotMessageCaptureDestination(destination: ChatCaptureDestination): ChatCaptureDestination {
  requireNonBlank(destination.chatId, 'address')
  requireNonBlank(destination.channelId, 'thread id')
  requireNonBlank(destination.appDir, 'app directory')
  return Object.freeze({
    chatId: destination.chatId,
    channelId: destination.channelId,
    appDir: destination.appDir,
  })
}

function sameDestination(left: ChatCaptureDestination, right: ChatCaptureDestination): boolean {
  return left.chatId === right.chatId && left.channelId === right.channelId && left.appDir === right.appDir
}

/**
 * Sends one captured PNG through useMessageSend without consuming any draft the
 * user already had in the SMS composer. useMessageSend snapshots the MMS body,
 * files, address, thread and upload directory synchronously before its first
 * await, so the draft can be restored immediately while the send stays frozen.
 */
export async function sendCapturedMms(file: File, destination: ChatCaptureDestination, port: CapturedMmsPort): Promise<void> {
  if (file.type !== 'image/png') throw new Error('captured MMS must be a PNG')
  const frozenDestination = snapshotMessageCaptureDestination(destination)
  const currentDestination = snapshotMessageCaptureDestination(port.currentDestination())
  if (!sameDestination(frozenDestination, currentDestination)) throw new Error('capture target changed before MMS delivery')

  const draftBody = port.messageBody.value
  const draftFiles = [...port.pendingFiles.value]
  let completion: Promise<boolean>
  port.messageBody.value = ''
  port.pendingFiles.value = [file]
  try {
    completion = port.sendMessage()
  } finally {
    port.messageBody.value = draftBody
    port.pendingFiles.value = draftFiles
  }

  if (!(await completion)) throw new Error('MMS capture send failed')
}
