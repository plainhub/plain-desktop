import { chachaDecrypt, chachaEncryptBytes } from './crypto'

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

export function filePathFromResourceId(key: Uint8Array, envelope: string): string {
  const separator = envelope.lastIndexOf('.')
  const mode = envelope.slice(0, 1)
  const owner = mode === 'g' ? '' : envelope.slice(2, separator)
  if (key.length !== 32 || envelope[1] !== '.' || !['c', 'g'].includes(mode)
    || (mode === 'g' ? separator !== 1 : !owner)) throw new Error('Invalid file resource scope')
  const payload = envelope.slice(separator + 1)
  if (!/^[\w-]+$/.test(payload)) throw new Error('Invalid resource payload')
  const bytes = Uint8Array.from(atob(payload.replace(/-/g, '+').replace(/_/g, '/')), char => char.charCodeAt(0))
  const aad = new TextEncoder().encode(`plain-fs-v1\0${mode}\0${owner}`)
  const resource = JSON.parse(chachaDecrypt(key, bytes, aad)) as Resource
  if (resource.type !== 'FILE' || typeof resource.id !== 'string' || !resource.id) throw new Error('File resource required')
  return resource.id
}
