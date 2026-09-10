const ECC_CODEWORDS_PER_BLOCK = [
  -1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28,
  28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
]

const NUM_ERROR_CORRECTION_BLOCKS = [
  -1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25,
  26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
]

const MODE_BYTE = [4, 8, 16, 16]

const PENALTY_N1 = 3
const PENALTY_N2 = 3
const PENALTY_N3 = 40
const PENALTY_N4 = 10

export interface QrMatrix {
  size: number
  modules: boolean[][]
}

export interface QrSvgOptions {
  border?: number
  darkColor?: string
  lightColor?: string
}

export function encodeQr(text: string): QrMatrix {
  const bytes = new TextEncoder().encode(text)
  const segment = appendByteSegment(bytes)
  let version = 1
  for (; version <= 40; version++) {
    if (
      segment.count < (1 << charCountBits(version)) &&
      segment.bits.length + 4 + charCountBits(version) <= dataCapacityBits(version)
    ) {
      break
    }
  }
  if (version > 40) throw new Error('QR data too long')
  const capacityBits = dataCapacityBits(version)

  const bb: number[] = []
  appendBits(MODE_BYTE[0], 4, bb)
  appendBits(segment.count, charCountBits(version), bb)
  bb.push(...segment.bits)
  appendBits(0, Math.min(4, capacityBits - bb.length), bb)
  appendBits(0, (8 - (bb.length % 8)) % 8, bb)
  for (let pad = 236; bb.length < capacityBits; pad ^= 236 ^ 17) appendBits(pad, 8, bb)
  const codewords = new Array<number>(Math.ceil(bb.length / 8)).fill(0)
  bb.forEach((bit, i) => (codewords[i >>> 3] |= bit << (7 - (i & 7))))
  return new QrCode(version, codewords)
}

export function qrCodeSvg(text: string, options: QrSvgOptions = {}): string {
  const border = options.border ?? 2
  const darkColor = options.darkColor ?? '#101010'
  const lightColor = options.lightColor ?? '#ffffff'
  const { size, modules } = encodeQr(text)
  const total = size + border * 2
  const parts: string[] = []
  for (let y = 0; y < size; y++) {
    let x = 0
    while (x < size) {
      if (!modules[y][x]) {
        x++
        continue
      }
      let run = 1
      while (x + run < size && modules[y][x + run]) run++
      parts.push(`M${x + border},${y + border}h${run}v1h-${run}z`)
      x += run
    }
  }
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${total} ${total}" ` +
    'shape-rendering="crispEdges">' +
    `<rect width="${total}" height="${total}" fill="${lightColor}"/>` +
    `<path fill="${darkColor}" d="${parts.join('')}"/>` +
    '</svg>'
  )
}

class QrCode implements QrMatrix {
  readonly size: number
  readonly mask: number
  readonly modules: boolean[][]
  private isFunction: boolean[][]

  constructor(version: number, dataCodewords: number[]) {
    this.size = version * 4 + 17
    this.modules = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false))
    this.isFunction = Array.from({ length: this.size }, () => new Array<boolean>(this.size).fill(false))
    this.drawFunctionPatterns()
    this.drawCodewords(this.addEccAndInterleave(version, dataCodewords))
    let mask = -1
    let minPenalty = Infinity
    for (let candidate = 0; candidate < 8; candidate++) {
      this.applyMask(candidate)
      this.drawFormatBits(candidate)
      const penalty = this.getPenaltyScore()
      if (penalty < minPenalty) {
        mask = candidate
        minPenalty = penalty
      }
      this.applyMask(candidate)
    }
    this.mask = mask
    this.applyMask(mask)
    this.drawFormatBits(mask)
  }

  private drawFunctionPatterns() {
    for (let i = 0; i < this.size; i++) {
      this.setFunctionModule(6, i, i % 2 === 0)
      this.setFunctionModule(i, 6, i % 2 === 0)
    }
    this.drawFinderPattern(3, 3)
    this.drawFinderPattern(this.size - 4, 3)
    this.drawFinderPattern(3, this.size - 4)
    const positions = this.alignmentPatternPositions()
    for (let i = 0; i < positions.length; i++) {
      for (let j = 0; j < positions.length; j++) {
        if (
          !(i === 0 && j === 0) &&
          !(i === 0 && j === positions.length - 1) &&
          !(i === positions.length - 1 && j === 0)
        ) {
          this.drawAlignmentPattern(positions[i], positions[j])
        }
      }
    }
    this.drawFormatBits(0)
    this.drawVersion()
  }

  private drawFormatBits(mask: number) {
    const data = (0 << 3) | mask
    let rem = data
    for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 1335)
    const bits = ((data << 10) | rem) ^ 21522
    for (let i = 0; i <= 5; i++) this.setFunctionModule(8, i, getBit(bits, i))
    this.setFunctionModule(8, 7, getBit(bits, 6))
    this.setFunctionModule(8, 8, getBit(bits, 7))
    this.setFunctionModule(7, 8, getBit(bits, 8))
    for (let i = 9; i < 15; i++) this.setFunctionModule(14 - i, 8, getBit(bits, i))
    for (let i = 0; i < 8; i++) this.setFunctionModule(this.size - 1 - i, 8, getBit(bits, i))
    for (let i = 8; i < 15; i++) this.setFunctionModule(8, this.size - 15 + i, getBit(bits, i))
    this.setFunctionModule(8, this.size - 8, true)
  }

  private drawVersion() {
    const version = (this.size - 17) / 4
    if (version < 7) return
    let rem = version
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 7973)
    const bits = (version << 12) | rem
    for (let i = 0; i < 18; i++) {
      const color = getBit(bits, i)
      const a = this.size - 11 + (i % 3)
      const b = Math.floor(i / 3)
      this.setFunctionModule(a, b, color)
      this.setFunctionModule(b, a, color)
    }
  }

  private drawFinderPattern(x: number, y: number) {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const dist = Math.max(Math.abs(dx), Math.abs(dy))
        const xx = x + dx
        const yy = y + dy
        if (xx >= 0 && xx < this.size && yy >= 0 && yy < this.size) {
          this.setFunctionModule(xx, yy, dist !== 2 && dist !== 4)
        }
      }
    }
  }

  private drawAlignmentPattern(x: number, y: number) {
    for (let dy = -2; dy <= 2; dy++) {
      for (let dx = -2; dx <= 2; dx++) {
        this.setFunctionModule(x + dx, y + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1)
      }
    }
  }

  private setFunctionModule(x: number, y: number, isDark: boolean) {
    this.modules[y][x] = isDark
    this.isFunction[y][x] = true
  }

  private addEccAndInterleave(version: number, data: number[]): number[] {
    if (data.length !== numDataCodewords(version)) throw new Error('Invalid QR data length')
    const numBlocks = NUM_ERROR_CORRECTION_BLOCKS[version]
    const blockEccLen = ECC_CODEWORDS_PER_BLOCK[version]
    const rawCodewords = Math.floor(numRawDataModules(version) / 8)
    const numShortBlocks = numBlocks - (rawCodewords % numBlocks)
    const shortBlockLen = Math.floor(rawCodewords / numBlocks)
    const rsDiv = reedSolomonComputeDivisor(blockEccLen)
    const blocks: number[][] = []
    for (let i = 0, k = 0; i < numBlocks; i++) {
      const dat = data.slice(k, k + shortBlockLen - blockEccLen + (i < numShortBlocks ? 0 : 1))
      k += dat.length
      const ecc = reedSolomonComputeRemainder(dat, rsDiv)
      if (i < numShortBlocks) dat.push(0)
      blocks.push(dat.concat(ecc))
    }
    const result: number[] = []
    for (let i = 0; i < blocks[0].length; i++) {
      blocks.forEach((block, j) => {
        if (i !== shortBlockLen - blockEccLen || j >= numShortBlocks) result.push(block[i])
      })
    }
    return result
  }

  private drawCodewords(data: number[]) {
    let i = 0
    for (let right = this.size - 1; right >= 1; right -= 2) {
      if (right === 6) right = 5
      for (let vert = 0; vert < this.size; vert++) {
        for (let j = 0; j < 2; j++) {
          const x = right - j
          const upward = ((right + 1) & 2) === 0
          const y = upward ? this.size - 1 - vert : vert
          if (!this.isFunction[y][x] && i < data.length * 8) {
            this.modules[y][x] = getBit(data[i >>> 3], 7 - (i & 7))
            i++
          }
        }
      }
    }
  }

  private applyMask(mask: number) {
    for (let y = 0; y < this.size; y++) {
      for (let x = 0; x < this.size; x++) {
        let invert: boolean
        switch (mask) {
          case 0:
            invert = (x + y) % 2 === 0
            break
          case 1:
            invert = y % 2 === 0
            break
          case 2:
            invert = x % 3 === 0
            break
          case 3:
            invert = (x + y) % 3 === 0
            break
          case 4:
            invert = (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0
            break
          case 5:
            invert = ((x * y) % 2) + ((x * y) % 3) === 0
            break
          case 6:
            invert = (((x * y) % 2) + ((x * y) % 3)) % 2 === 0
            break
          default:
            invert = (((x + y) % 2) + ((x * y) % 3)) % 2 === 0
        }
        if (!this.isFunction[y][x] && invert) this.modules[y][x] = !this.modules[y][x]
      }
    }
  }

  private getPenaltyScore() {
    let result = 0
    for (let y = 0; y < this.size; y++) {
      let runColor = false
      let runLength = 0
      const runHistory = [0, 0, 0, 0, 0, 0, 0]
      for (let x = 0; x < this.size; x++) {
        if (this.modules[y][x] === runColor) {
          runLength++
          if (runLength === 5) result += PENALTY_N1
          else if (runLength > 5) result++
        } else {
          this.finderPenaltyAddHistory(runLength, runHistory)
          if (!runColor) result += this.finderPenaltyCountPatterns(runHistory) * PENALTY_N3
          runColor = this.modules[y][x]
          runLength = 1
        }
      }
      result += this.finderPenaltyTerminateAndCount(runColor, runLength, runHistory) * PENALTY_N3
    }
    for (let x = 0; x < this.size; x++) {
      let runColor = false
      let runLength = 0
      const runHistory = [0, 0, 0, 0, 0, 0, 0]
      for (let y = 0; y < this.size; y++) {
        if (this.modules[y][x] === runColor) {
          runLength++
          if (runLength === 5) result += PENALTY_N1
          else if (runLength > 5) result++
        } else {
          this.finderPenaltyAddHistory(runLength, runHistory)
          if (!runColor) result += this.finderPenaltyCountPatterns(runHistory) * PENALTY_N3
          runColor = this.modules[y][x]
          runLength = 1
        }
      }
      result += this.finderPenaltyTerminateAndCount(runColor, runLength, runHistory) * PENALTY_N3
    }
    for (let y = 0; y < this.size - 1; y++) {
      for (let x = 0; x < this.size - 1; x++) {
        const color = this.modules[y][x]
        if (
          color === this.modules[y][x + 1] &&
          color === this.modules[y + 1][x] &&
          color === this.modules[y + 1][x + 1]
        ) {
          result += PENALTY_N2
        }
      }
    }
    let dark = 0
    for (const row of this.modules) {
      for (const color of row) if (color) dark++
    }
    const k = Math.ceil(Math.abs(dark * 20 - this.size * this.size * 10) / (this.size * this.size)) - 1
    return result + k * PENALTY_N4
  }

  private alignmentPatternPositions(): number[] {
    const version = (this.size - 17) / 4
    if (version === 1) return []
    const numAlign = Math.floor(version / 7) + 2
    const step = version === 32 ? 26 : Math.ceil((version * 4 + 4) / (numAlign * 2 - 2)) * 2
    const result = [6]
    for (let pos = this.size - 7; result.length < numAlign; pos -= step) {
      result.splice(1, 0, pos)
    }
    return result
  }

  private finderPenaltyCountPatterns(runHistory: number[]) {
    const n = runHistory[1]
    const core =
      n > 0 && runHistory[2] === n && runHistory[3] === n * 3 && runHistory[4] === n && runHistory[5] === n
    return (
      (core && runHistory[0] >= n * 4 && runHistory[6] >= n ? 1 : 0) +
      (core && runHistory[6] >= n * 4 && runHistory[0] >= n ? 1 : 0)
    )
  }

  private finderPenaltyTerminateAndCount(
    currentRunColor: boolean,
    currentRunLength: number,
    runHistory: number[],
  ) {
    let runLength = currentRunLength
    if (currentRunColor) {
      this.finderPenaltyAddHistory(runLength, runHistory)
      runLength = 0
    }
    runLength += this.size
    this.finderPenaltyAddHistory(runLength, runHistory)
    return this.finderPenaltyCountPatterns(runHistory)
  }

  private finderPenaltyAddHistory(currentRunLength: number, runHistory: number[]) {
    let runLength = currentRunLength
    if (runHistory[0] === 0) runLength += this.size
    runHistory.pop()
    runHistory.unshift(runLength)
  }
}

function appendByteSegment(bytes: Uint8Array): { count: number; bits: number[] } {
  const bits: number[] = []
  for (const b of bytes) appendBits(b, 8, bits)
  return { count: bytes.length, bits }
}

function appendBits(val: number, len: number, bb: number[]) {
  if (len < 0 || len > 31 || val >>> len !== 0) throw new RangeError('Value out of range')
  for (let i = len - 1; i >= 0; i--) bb.push((val >>> i) & 1)
}

function getBit(x: number, i: number) {
  return ((x >>> i) & 1) !== 0
}

function charCountBits(version: number) {
  return MODE_BYTE[Math.floor((version + 7) / 17) + 1]
}

function dataCapacityBits(version: number) {
  return numDataCodewords(version) * 8
}

function numRawDataModules(version: number) {
  let result = (16 * version + 128) * version + 64
  if (version >= 2) {
    const numAlign = Math.floor(version / 7) + 2
    result -= (25 * numAlign - 10) * numAlign - 55
    if (version >= 7) result -= 36
  }
  return result
}

function numDataCodewords(version: number) {
  return (
    Math.floor(numRawDataModules(version) / 8) -
    ECC_CODEWORDS_PER_BLOCK[version] * NUM_ERROR_CORRECTION_BLOCKS[version]
  )
}

function reedSolomonComputeDivisor(degree: number) {
  const result: number[] = new Array<number>(degree).fill(0)
  result[degree - 1] = 1
  let root = 1
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < result.length; j++) {
      result[j] = reedSolomonMultiply(result[j], root)
      if (j + 1 < result.length) result[j] ^= result[j + 1]
    }
    root = reedSolomonMultiply(root, 2)
  }
  return result
}

function reedSolomonComputeRemainder(data: number[] | Uint8Array, divisor: number[]) {
  const result = new Array<number>(divisor.length).fill(0)
  for (const b of data) {
    const factor = b ^ (result.shift() as number)
    result.push(0)
    divisor.forEach((coef, i) => (result[i] ^= reedSolomonMultiply(coef, factor)))
  }
  return result
}

function reedSolomonMultiply(x: number, y: number) {
  let z = 0
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ (z >>> 7) * 285
    z ^= ((y >>> i) & 1) * x
  }
  return z
}
