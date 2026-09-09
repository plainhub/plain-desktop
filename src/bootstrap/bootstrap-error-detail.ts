export function bootstrapErrorDetail(error: unknown): string {
  if (error instanceof Error) return `${error.name}: ${error.message}`.slice(0, 1024)
  try {
    const serialized = JSON.stringify(error)
    return (serialized ?? String(error)).slice(0, 1024)
  } catch {
    return String(error).slice(0, 1024)
  }
}
