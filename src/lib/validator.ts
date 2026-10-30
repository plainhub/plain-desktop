

export function isIP(v: string): boolean {
  if (!isIP4(v)) {
    return isIP6(v)
  }
  return true
}

export function isIP6(v: string): boolean {
  return /^(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))$/.test(
    v
  )
}

export function isIP4(v: string): boolean {
  return /^((25[0-5]|(2[0-4]|1[0-9]|[1-9]|)[0-9])(\.(?!$)|$)){4}$/.test(v)
}

export function isIPWithOptionalPort(v: string): boolean {
  if (v.includes(':')) {
    if (isIP6(v)) {
      return true
    }

    let ip: string
    let port: string
    if (v.includes(']:') && v.startsWith('[')) {
      const split = v.split(']:')
      ip = split[0].replace(/^\[+/, '')
      port = split[1]
    } else {
      const split = v.split(':')
      ip = split[0]
      port = split[1]
    }

    return isIP(ip) && isPortOrPortRangeMultiple(port)
  }

  return isIP(v)
}

export function isPortOrPortRangeMultiple(v: string): boolean {
  if (v.includes(',')) {
    return v.split(',').every(isPortOrPortRange)
  }
  return isPortOrPortRange(v)
}

export function isPortOrPortRange(v: string): boolean {
  if (/^[0-9]{1,5}|([0-9]{1,5}\\-[0-9]{1,5})$/.test(v)) {
    if (v.includes('-')) {
      const parts = v.split('-')
      const left = parseInt(parts[0])
      const right = parseInt(parts[1])
      return left >= 1 && left < right && left < 65536 && right > 0 && right < 65536
    } else {
      const n = parseInt(v)
      return n >= 1 && n <= 65535
    }
  }

  return false
}

export function isNetWithOptionalPort(v: string): boolean {
  if (v.includes(':')) {
    if (isIP6Net(v)) {
      return true
    }

    let ip: string
    let port: string
    if (v.includes(']:') && v.startsWith('[')) {
      const split = v.split(']:')
      ip = split[0].replace(/^\[+/, '')
      port = split[1]
    } else {
      const split = v.split(':')
      ip = split[0]
      port = split[1]
    }

    return isIPNet(ip) && isPortOrPortRangeMultiple(port)
  }

  return isIPNet(v)
}

export function isIP4Net(v: string): boolean {
  const split = v.split('/')
  if (split.length !== 2) {
    return false
  }

  const len = parseInt(split[1])
  return isIP4(split[0]) && len >= 0 && len <= 32
}

export function isIP6Net(v: string): boolean {
  const split = v.split('/')
  if (split.length !== 2) {
    return false
  }

  const len = parseInt(split[1])
  return isIP6(split[0]) && len > 32 && len <= 128
}

export function isIPNet(v: string): boolean {
  if (!isIP4Net(v)) {
    return isIP6Net(v)
  }

  return true
}

export function isDomain(v: string, wildcard: boolean): boolean {
  let vv = v
  if (wildcard) {
    vv = v.replace(/^[*.]+/, '')
  }

  if (/[a-z0-9]{2}/.test(vv)) {
    return true
  }

  return /^(?=.{1,253}\.?$)(?:(?!-|[^.]+_)[A-Za-z0-9-_]{1,63}(?<!-)(?:\.|$)){2,}$/.test(vv)
}

export function isDomainWithOptionalPort(v: string): boolean {
  if (v.includes(':')) {
    const split = v.split(':')
    const domain = split[0]
    const port = split[1]

    return isDomain(domain, true) && isPortOrPortRangeMultiple(port)
  }

  return isDomain(v, true)
}
