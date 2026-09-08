import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { isWindowsPlatform } from '@/lib/platform'

export type MdnsFirewallState = 'idle' | 'ok' | 'blocked' | 'fixing' | 'fixed' | 'failed'

export interface MdnsFirewallStatus {
  supported: boolean
  firewallOn: boolean
  allowRule: boolean
  blockRule: boolean
  exePath: string
}

// Module-level state shared by every mount (NearbyModal, DeviceSwitcherModal)
// so a check/fix started in one modal is not repeated in the other.
const state = ref<MdnsFirewallState>('idle')
const exePath = ref('')
let probing = false

function supported(): boolean {
  return __IS_TAURI__ && isWindowsPlatform()
}

/** Read-only firewall check — no UAC, silent on failure. */
export async function probeMdnsFirewall(): Promise<void> {
  if (!supported() || probing) return
  probing = true
  try {
    const s = await invoke<MdnsFirewallStatus>('mdns_firewall_status')
    exePath.value = s.exePath
    // Windows drops unsolicited inbound mDNS unless an Allow rule exists for
    // this exe; a leftover Block rule (the firewall popup's "Cancel") wins.
    state.value = s.firewallOn && (s.blockRule || !s.allowRule) ? 'blocked' : 'ok'
  } catch {
    state.value = 'ok'
  } finally {
    probing = false
  }
}

/** UAC-elevated repair; polls until the rule shows up. */
export async function fixMdnsFirewall(): Promise<void> {
  if (state.value === 'fixing') return
  state.value = 'fixing'
  try {
    await invoke('fix_mdns_firewall')
  } catch {
    state.value = 'failed'
    return
  }
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 1500))
    const s = await invoke<MdnsFirewallStatus>('mdns_firewall_status').catch(() => null)
    if (s && s.allowRule && !s.blockRule) {
      state.value = 'fixed'
      return
    }
  }
  state.value = 'failed'
}

export function useMdnsFirewall() {
  return { state, exePath, probe: probeMdnsFirewall, fix: fixMdnsFirewall }
}
