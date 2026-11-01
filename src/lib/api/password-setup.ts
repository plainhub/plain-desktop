import { httpRequest } from './http'

export function secureSetupUrl(pageUrl: string, port: number): string {
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid HTTPS port')
  const url = new URL(pageUrl)
  url.protocol = 'https:'
  url.port = String(port)
  return url.href
}

export async function setupPassword(baseUrl: string, clientId: string, password: string): Promise<void> {
  const url = new URL('/auth/setup', baseUrl)
  if (url.protocol !== 'https:') throw new Error('Password setup requires HTTPS')
  const response = await httpRequest(url.href, {
    method: 'POST',
    headers: { 'c-id': clientId, 'Content-Type': 'application/json' },
    body: new TextEncoder().encode(JSON.stringify({ password })),
  })
  if (!response.ok) throw new Error('Password setup failed')
}
