import type { EventRecord } from './api'

export function eventDetails(entry: EventRecord, translate: (key: string) => string): string {
  const data = entry.data
  switch (data.__typename) {
    case 'LoginEventData':
      return ''
    case 'LoginFailedEventData':
      return translate(`event_reason.${data.reason.toLowerCase()}`)
    case 'SessionEventData':
      return data.session.clientName || data.session.clientId
    case 'DeviceNameEventData':
      return `${data.oldName} → ${data.newName}`
    case 'VolumeEventData':
    case 'VolumeFailedEventData':
      return [data.volume.name, data.volume.mountPath, failure('failure' in data ? data.failure : undefined, translate)].filter(Boolean).join(' · ')
    case 'DiskFormatEventData':
    case 'DiskFormatFailedEventData':
      return [data.disk.name, data.disk.path, data.fileSystem, failure('failure' in data ? data.failure : undefined, translate)].filter(Boolean).join(' · ')
  }
}

type EventFailure = Extract<EventRecord['data'], { __typename: 'VolumeFailedEventData' }>['failure']

function failure(value: EventFailure | undefined, translate: (key: string) => string): string {
  return value ? [translate(`event_reason.${value.reason.toLowerCase()}`), value.detail].filter(Boolean).join(': ') : ''
}
