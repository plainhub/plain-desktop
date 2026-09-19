import toast from '@/components/toaster'
import { initQuery, deviceInfoGQL } from '@/lib/api/query'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatSeconds, formatFileSize } from '@/lib/format'

interface InfoItem { label: string; value: any; isTime?: boolean }

function formatMemory(bytes: number): string {
  return formatFileSize(bytes)
}

export function useDeviceInfo() {
  const { t } = useI18n()
  const basicInfos = ref<InfoItem[]>([])
  const systemInfos = ref<InfoItem[]>([])
  const hardwareInfos = ref<InfoItem[]>([])
  const platformInfos = ref<InfoItem[]>([])
  const statusInfos = ref<InfoItem[]>([])

  const { loading, refetch } = initQuery({
    handle: (data: any, error: string) => {
      if (error) { toast(t(error), 'error'); return }
      const d = data.deviceInfo

      basicInfos.value = [
        { label: 'device_name', value: d.name },
        { label: 'platform', value: d.platform },
        { label: 'manufacturer', value: d.manufacturer },
        { label: 'model', value: d.model },
        { label: 'language', value: d.language },
        { label: 'app_version', value: d.appVersion ? (d.appBuildNumber ? `${d.appVersion} (${d.appBuildNumber})` : d.appVersion) : '' },
      ].filter((it) => it.value)
      if (data.sims && data.sims.length > 0) {
        basicInfos.value.push({ label: 'phone_number', value: data.sims.map((s: any) => (s.label ? s.label + ' ' : '') + s.number) })
      }

      systemInfos.value = [
        { label: 'os_name', value: d.osName },
        { label: 'os_version', value: d.osVersion },
        { label: 'kernel', value: d.kernelVersion },
      ].filter((it) => it.value)

      const disp = d.display
      hardwareInfos.value = [
        { label: 'cpu_arch', value: d.cpuArch },
        { label: 'cpu_model', value: d.cpuModel },
        { label: 'total_memory', value: d.totalMemory ? formatMemory(d.totalMemory) : '' },
        { label: 'total_storage', value: d.totalStorage ? formatMemory(d.totalStorage) : '' },
        { label: 'screen_resolution', value: disp ? `${disp.width} × ${disp.height}` : '' },
        { label: 'screen_density', value: disp?.density ?? '' },
      ].filter((it) => it.value)

      if (d.android) {
        const a = d.android
        platformInfos.value = [
          { label: 'android_version', value: `${d.osVersion} (SDK ${a.sdkVersion})` },
          { label: 'security_patch', value: a.securityPatch },
          { label: 'bootloader', value: a.bootloader },
          { label: 'build_number', value: a.buildNumber },
          { label: 'baseband', value: a.radioVersion },
          { label: 'hardware', value: a.hardware },
          { label: 'board', value: a.board },
          { label: 'device', value: a.device },
          { label: 'brand', value: a.buildBrand },
          { label: 'java_vm', value: a.javaVmVersion },
          { label: 'opengl_es', value: a.glEsVersion },
          { label: 'build_fingerprint', value: a.fingerprint },
          { label: 'build_time', value: a.buildTime, isTime: true },
        ].filter((it) => it.value)
      } else {
        platformInfos.value = []
      }

      const st = data.deviceStatus
      if (st) {
        statusInfos.value = [
          { label: 'battery_level', value: st.batteryLevel != null ? `${st.batteryLevel}%` : '' },
          { label: 'charging', value: st.charging != null ? (st.charging ? t('yes') : t('no')) : '' },
          { label: 'temperatures', value: st.temperatures?.length ? st.temperatures.map((z: any) => `${z.label}: ${z.celsius} ℃`) : '' },
          { label: 'cpu_usage', value: st.cpuUsage != null ? `${st.cpuUsage.toFixed(1)}%` : '' },
          { label: 'memory_available', value: st.memoryAvailable != null ? formatMemory(st.memoryAvailable) : '' },
          { label: 'storage_available', value: st.storageAvailable != null ? formatMemory(st.storageAvailable) : '' },
          { label: 'uptime', value: st.uptimeSec != null ? formatSeconds(st.uptimeSec) : '' },
        ].filter((it) => it.value)
      } else {
        statusInfos.value = []
      }
    },
    document: deviceInfoGQL,
  })

  return { basicInfos, systemInfos, hardwareInfos, platformInfos, statusInfos, loading, refetch }
}
