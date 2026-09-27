import { describe, expect, it } from 'vitest'
import {
  compareShapes,
  loadMergedSchema,
  loadServerSchema,
  validateCorpusAgainst,
} from './contracts'
import { loadCorpus } from './pipeline'

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

const ENUM_EXTRA_ALLOWLIST = {
  Capability: ['LAN_SHARE', 'DISK_MANAGER'],
  DeviceType: ['UNKNOWN'],
}

const FIELD_WAIVERS = new Set([
  '[plain-app] Query.mounts',
  '[plain-app] Tag.type',
])

describe('plain-rs and plain-app GraphQL contracts', () => {
  it('validates frontend documents shared with plain-app', () => {
    const issues = validateCorpusAgainst(loadCorpus(), loadServerSchema('plain-app'))
      .filter((issue) => !APP_ABSENT_DOCS.has(issue.doc))
    expect(issues.map((issue) => `${issue.doc}: ${issue.error.path}: ${issue.error.message}`)).toEqual([])
  })

  it('matches plain-app field shapes', () => {
    const { issues } = compareShapes(
      loadMergedSchema(),
      loadServerSchema('plain-app'),
      'plain-app',
      ENUM_EXTRA_ALLOWLIST,
      FIELD_WAIVERS,
    )
    expect(issues.map((issue) => `${issue.path}: ${issue.message}`)).toEqual([])
  })
})
