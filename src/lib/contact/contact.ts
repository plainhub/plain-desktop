import type {
  ContactEventType,
  EmailType,
  ImProtocol,
  PhoneType,
  PostalType,
  WebsiteType,
} from '@/lib/interfaces'

// Option lists for the contact editor; values are the GraphQL enum names.
// CUSTOM is last so the common types come first in the dropdown.
export const types = {
  phoneNumberTypes: [
    'HOME', 'MOBILE', 'WORK', 'FAX_WORK', 'FAX_HOME', 'PAGER', 'OTHER',
    'CALLBACK', 'CAR', 'COMPANY_MAIN', 'ISDN', 'MAIN', 'OTHER_FAX', 'RADIO',
    'TELEX', 'TTY_TDD', 'WORK_MOBILE', 'WORK_PAGER', 'ASSISTANT', 'CUSTOM',
  ] as PhoneType[],
  emailTypes: ['HOME', 'WORK', 'OTHER', 'MOBILE', 'CUSTOM'] as EmailType[],
  addressTypes: ['HOME', 'WORK', 'OTHER', 'CUSTOM'] as PostalType[],
  eventTypes: ['ANNIVERSARY', 'BIRTHDAY', 'OTHER', 'CUSTOM'] as ContactEventType[],
  imProtocols: ['AIM', 'MSN', 'YAHOO', 'SKYPE', 'QQ', 'GOOGLE_TALK', 'ICQ', 'JABBER', 'NETMEETING', 'CUSTOM'] as ImProtocol[],
  websiteTypes: ['HOMEPAGE', 'BLOG', 'FTP', 'HOME', 'WORK', 'OTHER', 'CUSTOM'] as WebsiteType[],
}
