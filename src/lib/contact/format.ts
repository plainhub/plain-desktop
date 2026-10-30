import type { IContact } from '@/lib/interfaces'
import { containsChinese } from '@/lib/strutil'

export function getContactFullName(item: IContact): string {
  let name: string
  if (containsChinese(item.firstName) || containsChinese(item.lastName)) {
    name = `${item.lastName}${item.middleName}${item.firstName}`
  } else {
    name = [item.firstName, item.middleName, item.lastName].filter((it) => it).join(' ')
  }
  const suffixComma = item.suffix ? `, ${item.suffix}` : ''
  const fn = `${item.prefix} ${name} ${suffixComma}`.trim()
  if (fn) return fn
  if (item.emails.length) return item.emails[0].value
  return ''
}
