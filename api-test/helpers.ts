import { randomBytes } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { request as http } from 'node:http'
import { request as https } from 'node:https'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'

export interface Target {
  name: string
  url: string
  clientId: string
  keyEnv: string
  caFile?: string
  insecure?: boolean
}

const envFile = new URL('.env.local', import.meta.url)
if (existsSync(envFile)) process.loadEnvFile(envFile)
const configured: Target[] = JSON.parse(readFileSync(process.env.API_TEST_CONFIG ?? new URL('targets.local.json', import.meta.url), 'utf8'))
if (!Array.isArray(configured) || !configured.length) throw new Error('Configure at least one target in targets.local.json')
const selected = process.env.API_TEST_TARGETS?.split(',').map((name) => name.trim())
if (selected?.some((name) => !configured.some((target) => target.name === name))) throw new Error('Unknown API_TEST_TARGETS name')
export const targets = configured.filter((target) => !selected || selected.includes(target.name))

export function key(target: Target): Buffer {
  const bytes = Buffer.from(process.env[target.keyEnv] ?? '', 'base64')
  if (!target.name || !target.clientId || bytes.length !== 32) throw new Error(`${target.name}: configure clientId and a base64 32-byte key in ${target.keyEnv}`)
  return bytes
}
for (const target of targets) key(target)

export function tls(target: Target) {
  return { rejectUnauthorized: target.insecure !== true, ca: target.caFile ? readFileSync(target.caFile) : undefined }
}

export function encrypt(target: Target, plaintext: string): Buffer {
  const nonce = randomBytes(24)
  return Buffer.concat([nonce, xchacha20poly1305(key(target), nonce).encrypt(Buffer.from(plaintext))])
}

export function decrypt(target: Target, frame: Buffer): Buffer {
  return Buffer.from(xchacha20poly1305(key(target), frame.subarray(0, 24)).decrypt(frame.subarray(24)))
}

export function encode(target: Target, query: string, variables?: Record<string, unknown>): Buffer {
  return encrypt(target, `${Date.now()}|${randomBytes(16).toString('hex')}|${JSON.stringify({ query, variables })}`)
}

export function post(target: Target, body: Buffer): Promise<{ status: number; body: Buffer }> {
  const url = new URL(`${target.url.replace(/\/+$/, '')}/graphql`)
  return new Promise((resolve, reject) => {
    const request = (url.protocol === 'https:' ? https : http)(
      url,
      {
        method: 'POST',
        ...tls(target),
        signal: AbortSignal.timeout(10000),
        headers: { 'Content-Type': 'application/octet-stream', 'Content-Length': body.length, 'c-id': target.clientId },
      },
      (response) => {
        const chunks: Buffer[] = []
        response.on('data', (chunk: Buffer) => chunks.push(chunk))
        response.on('error', reject)
        response.on('end', () => resolve({ status: response.statusCode ?? 0, body: Buffer.concat(chunks) }))
      }
    )
    request.on('error', reject)
    request.end(body)
  })
}

export async function gql(target: Target, query: string, variables?: Record<string, unknown>): Promise<Record<string, any>> {
  const response = await post(target, encode(target, query, variables))
  if (response.status !== 200) throw new Error(`${target.name}: HTTP ${response.status}`)
  const result = JSON.parse(decrypt(target, response.body).toString('utf8'))
  if (result.errors?.length) throw new Error(`${target.name}: ${result.errors.map((error: { message: string }) => error.message).join('; ')}`)
  if (!result.data || typeof result.data !== 'object' || Array.isArray(result.data)) throw new Error(`${target.name}: missing GraphQL data`)
  return result.data
}
