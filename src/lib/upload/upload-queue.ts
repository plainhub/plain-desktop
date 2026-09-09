import type { IUploadItem } from '@/stores/temp'
import { upload } from './upload'
import emitter from '@/plugins/eventbus'

// Upload queue management interfaces and types
export interface IUploadTask {
  id: string
  upload: IUploadItem
  replace: boolean
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed'
  aborted?: boolean
}

interface ManagedUploadTask extends IUploadTask {
  completion: Promise<void>
  resolveCompletion: () => void
  rejectCompletion: (error: Error) => void
  completionSettled: boolean
  evictOnFailure: boolean
}

class UploadQueue {
  private queue: ManagedUploadTask[] = []
  private running: Map<string, ManagedUploadTask> = new Map()
  private readonly maxConcurrent = 3

  addTask(upload: IUploadItem, replace: boolean): string {
    return this.enqueueTask(upload, replace, false).id
  }

  addTaskAndWait(upload: IUploadItem, replace: boolean): Promise<void> {
    return this.enqueueTask(upload, replace, true).completion
  }

  private enqueueTask(upload: IUploadItem, replace: boolean, evictOnFailure: boolean): ManagedUploadTask {
    let resolveCompletion!: () => void
    let rejectCompletion!: (error: Error) => void
    const completion = new Promise<void>((resolve, reject) => {
      resolveCompletion = resolve
      rejectCompletion = reject
    })
    // Regular upload callers intentionally use the queue's status/retry API
    // instead of awaiting completion. Keep their failures handled while still
    // returning the original promise to transactional callers such as chat.
    void completion.catch(() => undefined)
    const task: ManagedUploadTask = {
      id: upload.id,
      upload,
      replace,
      status: 'pending',
      completion,
      resolveCompletion,
      rejectCompletion,
      completionSettled: false,
      evictOnFailure,
    }

    this.queue.push(task)
    this.processQueue()
    return task
  }

  pauseTask(taskId: string): boolean {
    const task = this.findTask(taskId)
    if (!task) {
      console.warn(`pauseTask: Task ${taskId} not found`)
      return false
    }

    console.log(`pauseTask: Attempting to pause task ${taskId}, current status: ${task.status}`)

    if (task.status === 'running') {
      task.status = 'paused'
      task.upload.status = 'paused'
      task.aborted = true

      // Abort ALL active XHRs (parallel chunk uploads)
      if (task.upload.xhrs && task.upload.xhrs.size > 0) {
        console.log(`pauseTask: Aborting ${task.upload.xhrs.size} active xhr(s) for task ${taskId}`)
        for (const xhr of task.upload.xhrs) {
          try {
            xhr.abort()
          } catch (error) {
            console.warn(`pauseTask: Error aborting xhr for task ${taskId}:`, error)
          }
        }
        task.upload.xhrs.clear()
      } else if (task.upload.xhr) {
        // Fallback for direct (non-chunked) uploads
        console.log(`pauseTask: Aborting xhr for task ${taskId}`)
        try {
          task.upload.xhr.abort()
        } catch (error) {
          console.warn(`pauseTask: Error aborting xhr for task ${taskId}:`, error)
        }
      } else {
        console.warn(`pauseTask: No xhr found for running task ${taskId}`)
      }

      this.running.delete(taskId)
      this.processQueue()
      console.log(`pauseTask: Task ${taskId} paused successfully`)
    } else if (task.status === 'pending') {
      task.status = 'paused'
      task.upload.status = 'paused'
      console.log(`pauseTask: Pending task ${taskId} paused successfully`)
    } else {
      console.warn(`pauseTask: Task ${taskId} cannot be paused, current status: ${task.status}`)
      return false
    }
    return true
  }

  resumeTask(taskId: string): boolean {
    const task = this.findTask(taskId)
    if (!task || task.status !== 'paused') return false

    task.status = 'pending'
    task.upload.status = 'uploading'
    task.aborted = false
    this.processQueue()
    return true
  }

  retryTask(taskId: string): boolean {
    const task = this.findTask(taskId)
    if (!task || task.status !== 'failed') return false

    task.status = 'pending'
    task.upload.status = 'uploading'
    task.upload.error = ''
    task.upload.uploadedSize = 0
    task.upload.uploadSpeed = 0
    task.upload.lastUploadedSize = 0
    task.upload.lastUpdateTime = undefined
    task.aborted = false
    this.processQueue()
    return true
  }

  removeTask(taskId: string): boolean {
    const task = this.findTask(taskId)
    if (!task) return false

    if (task.status === 'running') {
      task.aborted = true
      // Abort all active XHRs
      if (task.upload.xhrs && task.upload.xhrs.size > 0) {
        for (const xhr of task.upload.xhrs) {
          try {
            xhr.abort()
          } catch (_) {
            /* ignore */
          }
        }
        task.upload.xhrs.clear()
      } else {
        task.upload.xhr?.abort()
      }
      this.running.delete(taskId)
    }

    this.queue = this.queue.filter((t) => t.id !== taskId)
    task.upload.status = 'canceled'
    this.rejectTask(task, new Error('Upload canceled'))
    this.processQueue()
    return true
  }

  getQueueStatus() {
    return {
      pending: this.queue.filter((t) => t.status === 'pending').length,
      running: this.running.size,
      paused: this.queue.filter((t) => t.status === 'paused').length,
      total: this.queue.length,
    }
  }

  private findTask(taskId: string): ManagedUploadTask | undefined {
    return this.queue.find((t) => t.id === taskId) || this.running.get(taskId)
  }

  private processQueue(): void {
    // Start new tasks if we have capacity
    while (this.running.size < this.maxConcurrent) {
      const nextTask = this.queue.find((t) => t.status === 'pending')
      if (!nextTask) break

      this.executeTask(nextTask)
    }
  }

  private async executeTask(task: ManagedUploadTask): Promise<void> {
    task.status = 'running'
    task.upload.status = 'uploading'
    task.aborted = false
    this.running.set(task.id, task)

    try {
      const result = (await upload(task.upload, task.replace)) as { error?: string } | undefined

      // Check if task was aborted during upload
      if (task.aborted) {
        return
      }

      // Respect the status already set by upload() / uploadWithChunks().
      // upload() may return { error } for some paths OR set upload.status
      // directly for others (returning undefined). Check both.
      if (result?.error) {
        task.status = 'failed'
        task.upload.status = 'error'
        task.upload.error ||= result.error
      } else if (task.upload.status === 'error') {
        // uploadWithChunks set error status internally but returned undefined
        task.status = 'failed'
      } else {
        task.status = 'completed'
        task.upload.status = 'done'
      }
    } catch (error: any) {
      // Check if task was aborted during upload
      if (task.aborted) {
        return
      }

      task.status = 'failed'
      task.upload.status = 'error'
      task.upload.error = error.message || 'Upload failed'
    } finally {
      this.running.delete(task.id)
      if (task.status === 'completed') {
        this.queue = this.queue.filter((candidate) => candidate !== task)
        this.resolveTask(task)
        this.emitSafely('upload_task_done', task.upload)
      } else if (task.status === 'failed') {
        this.rejectTask(task, new Error(task.upload.error || 'Upload failed'))
        if (task.evictOnFailure) this.queue = this.queue.filter((candidate) => candidate !== task)
        this.emitSafely('upload_progress', task.upload)
      }
      this.processQueue()
    }
  }

  private resolveTask(task: ManagedUploadTask): void {
    if (task.completionSettled) return
    task.completionSettled = true
    task.resolveCompletion()
  }

  private rejectTask(task: ManagedUploadTask, error: Error): void {
    if (task.completionSettled) return
    task.completionSettled = true
    task.rejectCompletion(error)
  }

  private emitSafely(event: 'upload_task_done' | 'upload_progress', upload: IUploadItem): void {
    try {
      emitter.emit(event, upload)
    } catch (error) {
      // Queue ownership is already settled. UI observers cannot turn a stored
      // upload result into a failed or permanently retained transaction.
      console.error(`Upload queue observer failed for ${event}`, error)
    }
  }
}

const uploadQueue = new UploadQueue()

export function addUploadTask(upload: IUploadItem, replace: boolean): string {
  return uploadQueue.addTask(upload, replace)
}

/** Queue one upload and settle only when its terminal result is known. */
export function addUploadTaskAndWait(upload: IUploadItem, replace: boolean): Promise<void> {
  return uploadQueue.addTaskAndWait(upload, replace)
}

export function pauseUpload(taskId: string): boolean {
  return uploadQueue.pauseTask(taskId)
}

export function resumeUpload(taskId: string): boolean {
  return uploadQueue.resumeTask(taskId)
}

export function retryUpload(taskId: string): boolean {
  return uploadQueue.retryTask(taskId)
}

export function removeUpload(taskId: string): boolean {
  return uploadQueue.removeTask(taskId)
}

export function getUploadQueueStatus() {
  return uploadQueue.getQueueStatus()
}
