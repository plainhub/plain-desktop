import { ResourcePurpose } from './aad-purpose.generated'
import { chachaDecrypt, chachaEncryptBytes } from './crypto'

export type Resource = { type: 'MEDIA' | 'FOREGROUND_MEDIA_THUMBNAIL' | 'APP_FILE' | 'FILE' | 'PACKAGE_ICON' | 'SHARED_FILE'; id: string }
export type ResourceScope = { mode: 'c' | 's'; id: string }

function resourceAad(scope: ResourceScope, purpose: ResourcePurpose): Uint8Array {
  return new TextEncoder().encode(`${purpose}\0${scope.mode}\0${scope.id}`)
}

export function encryptResourcePayload(key: Uint8Array, scope: ResourceScope, purpose: ResourcePurpose, plain: string): string {
  const owner = scope.id
  if (key.length !== 32 || !owner) throw new Error('Invalid resource credential')
  const aad = resourceAad(scope, purpose)
  const bytes = chachaEncryptBytes(key, new TextEncoder().encode(plain), aad)
  const payload = btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  const envelope = `${scope.mode}.${owner}.${payload}`
  if (new TextEncoder().encode(owner).length > 4096 || new TextEncoder().encode(envelope).length > 32768) throw new Error('Resource URL too long')
  return envelope
}

export function resourceId(key: Uint8Array, scope: ResourceScope, resource: Resource): string {
  if (!resource.id || (scope.mode === 's') !== (resource.type === 'SHARED_FILE')) throw new Error('Invalid resource scope')
  return encryptResourcePayload(key, scope, ResourcePurpose.Fs, JSON.stringify(resource))
}

export function filePathFromResourceId(key: Uint8Array, envelope: string): string {
  const separator = envelope.lastIndexOf('.')
  const mode = envelope.slice(0, 1)
  const owner = envelope.slice(2, separator)
  if (key.length !== 32 || envelope[1] !== '.' || mode !== 'c' || !owner) throw new Error('Invalid file resource scope')
  const payload = envelope.slice(separator + 1)
  if (!/^[\w-]+$/.test(payload)) throw new Error('Invalid resource payload')
  const bytes = Uint8Array.from(atob(payload.replace(/-/g, '+').replace(/_/g, '/')), char => char.charCodeAt(0))
  const aad = resourceAad({ mode, id: owner }, ResourcePurpose.Fs)
  const resource = JSON.parse(chachaDecrypt(key, bytes, aad)) as Resource
  if (resource.type !== 'FILE' || typeof resource.id !== 'string' || !resource.id) throw new Error('File resource required')
  return resource.id
}
