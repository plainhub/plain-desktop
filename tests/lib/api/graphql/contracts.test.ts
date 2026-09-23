import { describe, expect, it } from 'vitest'
import {
  compareShapes,
  loadMergedSchema,
  loadServerSchema,
  validateCorpusAgainst,
  type BackendName,
  type CorpusIssue,
} from './contracts'
import { loadCorpus } from './pipeline'

/**
 * Server contract snapshots (copied from plain-app `shared/apitest/schema.graphqls`
 * and plain-nas `apitest/schema.graphqls`). When either server schema changes,
 * re-copy the file and re-run: failures below enumerate every frontend document
 * or schema shape that must migrate.
 *
 * Exemption rules:
 * - plain-app: documents listed in APP_ABSENT_DOCS target the desktop local
 *   server or a NAS (chat item singular, disks, samba, media-scan controls,
 *   peer downloads, guest share) and are never sent to a phone.
 * - plain-nas: any issue caused by a type or root field the NAS snapshot
 *   simply does not declare is waived — the app.features capability gating
 *   keeps those documents away from a NAS backend at runtime. The waiver is
 *   derived from the snapshot itself, so it tightens automatically as the
 *   NAS schema grows. Everything else (unknown/named arguments, required
 *   arguments, variable nullability, unknown fields on types the NAS does
 *   serve) must hold for every document.
 */

const APP_ABSENT_DOCS = new Set([
  'chatItemGQL',
  'disksGQL',
  'downloadPeerFileGQL',
  'formatDiskGQL',
  'nasMountsMetaGQL',
  'pauseDownloadGQL',
  'pauseMediaScanGQL',
  'rebuildMediaIndexGQL',
  'respondChannelInviteGQL',
  'resumeDownloadGQL',
  'resumeMediaScanGQL',
  'retryDownloadGQL',
  'sambaSettingsGQL',
  'scanProgressGQL',
  'setSambaSettingsGQL',
  'setSambaUserPasswordGQL',
  'sharedInfoGQL',
  'stopMediaScanGQL',
])

/** Enum values the desktop union contract declares beyond a server's enum,
 *  reviewed and intentional. */
const ENUM_EXTRA_ALLOWLIST: Record<BackendName, Record<string, string[]>> = {
  'plain-app': {
    Capability: ['LAN_SHARE', 'DISK_MANAGER'],
    DeviceType: ['UNKNOWN'],
  },
  'plain-nas': {
    Capability: ['NOTIFICATIONS'],
    DeviceType: ['UNKNOWN'],
  },
}

/** Field paths whose desktop/server typing difference is reviewed and
 *  wire-compatible; every entry needs a reason. */
const FIELD_WAIVERS: Record<BackendName, string[]> = {
  'plain-app': [
    // Desktop names the mount type `Mount`, phones serve `StorageMount`; the
    // desktop field set is a superset (NAS partition meta via extension).
    '[plain-app] Query.mounts',
    // Pairing inputs accept plain strings on the desktop; JSON-string
    // variables coerce to the phone's DeviceType/DiscoveryMethod enums.
    "[plain-app] PairingDeviceInput.deviceType",
    '[plain-app] PairingDeviceInput.discoveryMethods',
    "[plain-app] PairingRequestInput.deviceType",
  ],
  'plain-nas': [
    '[plain-nas] Query.mounts',
    "[plain-nas] PairingDeviceInput.deviceType",
    '[plain-nas] PairingDeviceInput.discoveryMethods',
    "[plain-nas] PairingRequestInput.deviceType",
  ],
}

function quotedFrom(message: string, prefix: string): string | undefined {
  const marker = `${prefix} '`
  const start = message.indexOf(marker)
  if (start === -1) return undefined
  const rest = message.slice(start + marker.length)
  const end = rest.indexOf("'")
  if (end === -1) return undefined
  return rest.slice(0, end)
}

function isNasWaived(issue: CorpusIssue, nasSchema: ReturnType<typeof loadServerSchema>): boolean {
  const missingType = quotedFrom(issue.error.message, 'unknown type')
  if (missingType && !nasSchema.types.has(missingType)) return true
  const rootField = quotedFrom(issue.error.message, 'unknown field')
  if (rootField) {
    const onRoot = /on type '(QueryRoot|MutationRoot|Query|Mutation)'/.test(issue.error.message)
    if (onRoot) {
      const roots = [nasSchema.types.get(nasSchema.queryRoot), nasSchema.types.get(nasSchema.mutationRoot)]
      if (!roots.some((r) => r?.fields.some((f) => f.name === rootField))) return true
    }
  }
  const onField = quotedFrom(issue.error.message, 'on field')
  if (onField) {
    const fieldName = onField.split('.').pop()
    if (fieldName) {
      const roots = [nasSchema.types.get(nasSchema.queryRoot), nasSchema.types.get(nasSchema.mutationRoot)]
      if (!roots.some((r) => r?.fields.some((f) => f.name === fieldName))) return true
    }
  }
  return false
}

describe('server contract snapshots', () => {
  const corpus = loadCorpus()

  for (const backend of ['plain-app', 'plain-nas'] as const) {
    it(`every frontend document is executable against ${backend} (gated surfaces excluded)`, () => {
      const schema = loadServerSchema(backend)
      const issues = validateCorpusAgainst(corpus, schema).filter((issue) => {
        if (backend === 'plain-app') return !APP_ABSENT_DOCS.has(issue.doc)
        return !isNasWaived(issue, schema)
      })
      expect(issues.map((i) => `${i.doc}: ${i.error.path}: ${i.error.message}`)).toEqual([])
    })
  }

  for (const backend of ['plain-app', 'plain-nas'] as const) {
    it(`desktop contract shapes match ${backend} (wire-compatible views)`, () => {
      const merged = loadMergedSchema()
      const { issues } = compareShapes(
        merged,
        loadServerSchema(backend),
        backend,
        ENUM_EXTRA_ALLOWLIST[backend],
        new Set(FIELD_WAIVERS[backend]),
      )
      expect(issues.map((i) => `${i.path}: ${i.message}`)).toEqual([])
    })
  }
})
