import { chachaEncryptBytes } from './crypto'

export type Resource = { type: 'MEDIA' | 'APP_FILE' | 'FILE' | 'PACKAGE_ICON' | 'SHARED_FILE'; id: string }
export type ResourceScope = { mode: 'c' | 's'; id: string } | { mode: 'g' }

export function encryptResourcePayload(key: Uint8Array, scope: ResourceScope, purpose: string, plain: string): string {
  const owner = scope.mode === 'g' ? '' : scope.id
  if (key.length !== 32 || (scope.mode !== 'g' && !owner)) throw new Error('Invalid resource credential')
  const aad = new TextEncoder().encode(`plain-${purpose}-v1\0${scope.mode}\0${owner}`)
  const bytes = chachaEncryptBytes(key, new TextEncoder().encode(plain), aad)
  const payload = btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  const envelope = scope.mode === 'g' ? `g.${payload}` : `${scope.mode}.${owner}.${payload}`
  if (new TextEncoder().encode(owner).length > 4096 || new TextEncoder().encode(envelope).length > 32768) throw new Error('Resource URL too long')
  return envelope
}

export function resourceId(key: Uint8Array, scope: ResourceScope, resource: Resource): string {
  if (!resource.id || (scope.mode === 's') !== (resource.type === 'SHARED_FILE')) throw new Error('Invalid resource scope')
  return encryptResourcePayload(key, scope, 'fs', JSON.stringify(resource))
}
