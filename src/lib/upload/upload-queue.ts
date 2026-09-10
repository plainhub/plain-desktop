import type { IUploadItem } from '@/stores/temp'
import { upload } from './upload'
import { isTransientUploadError } from './errors'
import emitter from '@/plugins/eventbus'

// Upload queue management interfaces and types
export interface IUploadTask {
  id: string
  upload: IUploadItem
  replace: boolean
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed'
  aborted?: boolean
}

// A task with a transient error (network hiccup, merge timeout) gets retried
// in place with backoff before it is declared failed.
const MAX_TASK_ATTEMPTS = 3
const RETRY_BACKOFF_MS = [2000, 8000]
// When this many tasks fail in a row with transient errors, the connection
// itself is presumed dead: pause everything so a 10k-file queue doesn't burn
// through every file failing instantly. The user resumes after fixing it.
const AUTO_PAUSE_FAILURE_STREAK = 5

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
  private failureStreak = 0

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

  removeTask(taskId: string): boolean {
    const task = this.findTask(taskId)
    if (!task) return false

    if (task.status === 'running') {
      task.aborted = true
      this.abortTaskXhrs(task)
      this.running.delete(taskId)
    }

    this.queue = this.queue.filter((t) => t.id !== taskId)
    task.upload.status = 'canceled'
    this.rejectTask(task, new Error('Upload canceled'))
    this.processQueue()
    return true
  }

  // Batch-level operations. A directory upload enqueues tens of thousands of
  // tasks sharing one batchId; driving these through the per-task API is
  // O(n²) (queue scan per task) and froze the UI on pause/remove.
  pauseTasksByBatch(batchId: string): IUploadItem[] {
    const affected: IUploadItem[] = []
    for (const task of this.tasksByBatch(batchId)) {
      if (task.status === 'running') {
        task.status = 'paused'
        task.upload.status = 'paused'
        task.upload.uploadSpeed = 0
        task.aborted = true
        this.abortTaskXhrs(task)
        this.running.delete(task.id)
        affected.push(task.upload)
      } else if (task.status === 'pending') {
        task.status = 'paused'
        task.upload.status = 'paused'
        affected.push(task.upload)
      }
    }
    if (affected.length > 0) this.processQueue()
    return affected
  }

  resumeTasksByBatch(batchId: string): void {
    let resumed = false
    for (const task of this.tasksByBatch(batchId)) {
      if (task.status !== 'paused') continue
      task.status = 'pending'
      task.upload.status = 'uploading'
      task.aborted = false
      resumed = true
    }
    if (resumed) this.processQueue()
  }

  retryTasksByBatch(batchId: string): void {
    let retried = false
    for (const task of this.tasksByBatch(batchId)) {
      if (task.status !== 'failed') continue
      this.resetTaskForRetry(task)
      retried = true
    }
    if (retried) this.processQueue()
  }

  retryTask(taskId: string): boolean {
    const task = this.findTask(taskId)
    if (!task || task.status !== 'failed') return false
    this.resetTaskForRetry(task)
    this.processQueue()
    return true
  }

  private resetTaskForRetry(task: ManagedUploadTask): void {
    task.status = 'pending'
    task.upload.status = 'uploading'
    task.upload.error = ''
    task.upload.uploadedSize = 0
    task.upload.uploadSpeed = 0
    task.upload.lastUploadedSize = 0
    task.upload.lastUpdateTime = undefined
    task.aborted = false
  }

  removeTasksByBatch(batchId: string): void {
    const removed = new Set<ManagedUploadTask>()
    for (const task of this.tasksByBatch(batchId)) {
      if (task.status === 'running') {
        task.aborted = true
        this.abortTaskXhrs(task)
        this.running.delete(task.id)
      }
      task.upload.status = 'canceled'
      this.rejectTask(task, new Error('Upload canceled'))
      removed.add(task)
    }
    if (removed.size === 0) return
    this.queue = this.queue.filter((t) => !removed.has(t))
    this.processQueue()
  }

  getQueueStatus() {
    return {
      pending: this.queue.filter((t) => t.status === 'pending').length,
      running: this.running.size,
      paused: this.queue.filter((t) => t.status === 'paused').length,
      total: this.queue.length,
    }
  }

  private keyOfTask(task: ManagedUploadTask): string {
    return task.upload.batchId || task.upload.id
  }

  private tasksByBatch(batchId: string): ManagedUploadTask[] {
    const out: ManagedUploadTask[] = []
    for (const task of this.queue) {
      if (this.keyOfTask(task) === batchId) out.push(task)
    }
    for (const task of this.running.values()) {
      if (this.keyOfTask(task) === batchId) out.push(task)
    }
    return out
  }

  private abortTaskXhrs(task: ManagedUploadTask): void {
    if (task.upload.xhrs) {
      for (const xhr of task.upload.xhrs) {
        try {
          xhr.abort()
        } catch {
          /* ignore */
        }
      }
      task.upload.xhrs.clear()
    }
    if (task.upload.xhr) {
      try {
        task.upload.xhr.abort()
      } catch {
        /* ignore */
      }
    }
  }

  private findTask(taskId: string): ManagedUploadTask | undefined {
    return this.running.get(taskId) || this.queue.find((t) => t.id === taskId)
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
      let result: { error?: string } | undefined

      for (let attempt = 0; ; attempt++) {
        result = (await upload(task.upload, task.replace)) as { error?: string } | undefined

        // Check if task was aborted during upload
        if (task.aborted) {
          return
        }

        // Respect the status already set by upload() / uploadWithChunks().
        // upload() may return { error } for some paths OR set upload.status
        // directly for others (returning undefined). Check both.
        const failed = !!result?.error || task.upload.status === 'error'
        if (!failed) break

        const errorText = result?.error || task.upload.error || 'Upload failed'
        task.upload.error ||= errorText
        if (attempt >= MAX_TASK_ATTEMPTS - 1 || !isTransientUploadError(errorText)) break

        if (!(await this.waitUnlessAborted(task, RETRY_BACKOFF_MS[Math.min(attempt, RETRY_BACKOFF_MS.length - 1)]))) {
          return
        }
        task.upload.status = 'uploading'
      }

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
        this.failureStreak = 0
        // Per-item speeds are point samples; leaving them set after completion
        // made batch-level throughput sum the whole upload history.
        task.upload.uploadSpeed = 0
        this.queue = this.queue.filter((candidate) => candidate !== task)
        this.resolveTask(task)
        this.emitSafely('upload_task_done', task.upload)
      } else if (task.status === 'failed') {
        task.upload.uploadSpeed = 0
        if (isTransientUploadError(task.upload.error) && ++this.failureStreak >= AUTO_PAUSE_FAILURE_STREAK) {
          this.failureStreak = 0
          this.pauseAllUploads()
          emitter.emit('toast', 'upload_auto_paused')
        }
        this.rejectTask(task, new Error(task.upload.error || 'Upload failed'))
        if (task.evictOnFailure) this.queue = this.queue.filter((candidate) => candidate !== task)
        this.emitSafely('upload_progress', task.upload)
      }
      this.processQueue()
    }
  }

  // Backoff sleep that bails out when the task is paused or removed while
  // waiting, so a paused queue stops immediately instead of after the delay.
  // Timer-only (no Date.now) so fake-timer tests can drive it deterministically.
  private waitUnlessAborted(task: ManagedUploadTask, delayMs: number): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      const finish = (result: boolean) => {
        clearInterval(interval)
        clearTimeout(timeout)
        resolve(result)
      }
      const interval = setInterval(() => {
        if (task.aborted) finish(false)
      }, 100)
      const timeout = setTimeout(() => finish(!task.aborted), delayMs)
      if (task.aborted) finish(false)
    })
  }

  private pauseAllUploads(): void {
    const all = [...this.running.values(), ...this.queue]
    for (const task of all) {
      if (task.status === 'running') {
        task.status = 'paused'
        task.upload.status = 'paused'
        task.upload.uploadSpeed = 0
        task.aborted = true
        this.abortTaskXhrs(task)
        this.running.delete(task.id)
      } else if (task.status === 'pending') {
        task.status = 'paused'
        task.upload.status = 'paused'
      }
    }
    this.processQueue()
  }

  resetFailureStreak(): void {
    this.failureStreak = 0
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

export function removeUpload(taskId: string): boolean {
  return uploadQueue.removeTask(taskId)
}

export function pauseUploadsByBatch(batchId: string): IUploadItem[] {
  return uploadQueue.pauseTasksByBatch(batchId)
}

export function resumeUploadsByBatch(batchId: string): void {
  uploadQueue.resumeTasksByBatch(batchId)
}

export function retryUploadsByBatch(batchId: string): void {
  uploadQueue.retryTasksByBatch(batchId)
}

export function retryUploadTask(taskId: string): boolean {
  return uploadQueue.retryTask(taskId)
}

export function removeUploadsByBatch(batchId: string): void {
  uploadQueue.removeTasksByBatch(batchId)
}

export function getUploadQueueStatus() {
  return uploadQueue.getQueueStatus()
}

// The failure streak is queue-lifetime state; tests need it reset between cases.
export function resetUploadFailureStreakForTests(): void {
  uploadQueue.resetFailureStreak()
}
