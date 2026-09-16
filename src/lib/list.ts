export function noDataKey(loading: boolean, permissions: string[] = [], permission = '', online = true): string {
  if (loading) {
    return 'loading'
  }

  if (!online) {
    return 'offline'
  }

  if (permission && !permissions.includes(permission)) {
    return 'no_permission'
  }

  return 'no_data'
}
