/**
 * Campfire 邀请码的编解码。
 *
 * 格式：`U/XXXX-XXXX-XXXX-XXXX`（大写）。
 *
 * - 16 个字符是 base-34 数位，**最低位在前**，字母表 `0-9A-Z` 去掉易混的 I 和 O。
 * - 整个数值必须是 7 的倍数，作为输入校验。
 * - 前 8 位（最低 8 位）拼成虚拟网络名：`scaffolding-mc-<0..3>-<4..7>`。
 * - 后 8 位拼成网络密钥：`<8..11>-<12..15>`。
 *
 * 这套规则来自 Campfire（GPL-3.0）的 `RoomCode.cs`。算法本身不受版权保护，
 * 但本模块作为其行为等价实现，随本仓库一并以 GPL-3.0-or-later 提供。
 */
import { randomBytes } from 'node:crypto'

const ALPHABET = '0123456789ABCDEFGHJKLMNPQRSTUVWXYZ'
const NETWORK_PREFIX = 'scaffolding-mc-'
const DIGIT_COUNT = 16
const CHECKSUM = 7
/** `U/` + 16 位 + 3 个连字符 */
const TEXT_LENGTH = 21
const RADIX = BigInt(ALPHABET.length)
const RANGE = RADIX ** BigInt(DIGIT_COUNT)

export interface RoomCodeInfo {
  /** 展示给玩家的邀请码，如 `U/1A2B-3C4D-5E6F-7G8H` */
  code: string
  /** EasyTier 的 network_name */
  networkName: string
  /** EasyTier 的 network_secret */
  networkSecret: string
}

function group(digits: string, start: number): string {
  return digits.slice(start, start + 4)
}

function fromDigits(digits: string): RoomCodeInfo {
  const body = [
    group(digits, 0),
    group(digits, 4),
    group(digits, 8),
    group(digits, 12),
  ].join('-')

  return {
    code: `U/${body}`,
    networkName: `${NETWORK_PREFIX}${group(digits, 0)}-${group(digits, 4)}`,
    networkSecret: `${group(digits, 8)}-${group(digits, 12)}`,
  }
}

/** 把一个 16 位以内的数值展开成 16 个 base-34 数位（最低位在前）。 */
function toDigits(value: bigint): string {
  const digits: string[] = []
  let rest = value
  for (let i = 0; i < DIGIT_COUNT; i++) {
    digits.push(ALPHABET.charAt(Number(rest % RADIX)))
    rest /= RADIX
  }
  return digits.join('')
}

/** 生成一个随机房间码。 */
export function createRoomCode(): RoomCodeInfo {
  const bytes = randomBytes(8)
  let value = BigInt(0)
  for (let i = bytes.length - 1; i >= 0; i--) {
    value = (value << BigInt(8)) | BigInt(bytes[i])
  }
  value %= RANGE
  value -= value % BigInt(CHECKSUM)
  return fromDigits(toDigits(value))
}

function digitOf(char: string): number {
  if (char === 'I') return ALPHABET.indexOf('1')
  if (char === 'O') return ALPHABET.indexOf('0')
  return ALPHABET.indexOf(char)
}

/**
 * 从一段文本里找出第一个合法邀请码。
 *
 * 允许整段聊天消息，大小写不敏感，`I`/`O` 分别按 `1`/`0` 处理。
 */
export function findRoomCode(text: string): RoomCodeInfo | undefined {
  if (!text) return undefined
  const upper = text.toUpperCase()

  for (let start = 0; start + TEXT_LENGTH <= upper.length; start++) {
    if (upper[start] !== 'U' || upper[start + 1] !== '/') continue

    const body = upper.slice(start + 2, start + TEXT_LENGTH)
    if (body.length !== TEXT_LENGTH - 2) continue

    let value = BigInt(0)
    let ok = true
    // body 末尾是最高位
    for (let i = body.length - 1; i >= 0; i--) {
      const char = body[i]
      if (i === 4 || i === 9 || i === 14) {
        if (char !== '-') {
          ok = false
          break
        }
        continue
      }
      const digit = digitOf(char)
      if (digit < 0) {
        ok = false
        break
      }
      value = value * RADIX + BigInt(digit)
    }

    if (ok && value % BigInt(CHECKSUM) === BigInt(0)) {
      return fromDigits(toDigits(value))
    }
  }

  return undefined
}

/** 严格解析：整串必须恰好是一个邀请码。 */
export function parseRoomCode(text: string): RoomCodeInfo | undefined {
  const trimmed = text.trim().toUpperCase()
  if (trimmed.length !== TEXT_LENGTH) return undefined
  return findRoomCode(trimmed)
}
