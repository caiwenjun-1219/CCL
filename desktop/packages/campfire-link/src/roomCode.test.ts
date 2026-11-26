import { describe, expect, it } from 'vitest'
import { createRoomCode, findRoomCode, parseRoomCode } from './roomCode'

/**
 * 测试向量由独立实现（Python）按 Campfire 的 `RoomCode.cs` 规则生成，
 * 用来确认本实现与上游互通，而不只是自洽。
 */
const VECTORS = [
  {
    code: 'U/0000-0000-0000-0000',
    networkName: 'scaffolding-mc-0000-0000',
    networkSecret: '0000-0000',
  },
  {
    code: 'U/7000-0000-0000-0000',
    networkName: 'scaffolding-mc-7000-0000',
    networkSecret: '0000-0000',
  },
  {
    code: 'U/7QHP-0K00-0000-0000',
    networkName: 'scaffolding-mc-7QHP-0K00',
    networkSecret: '0000-0000',
  },
  {
    code: 'U/ZZZZ-ZZZZ-ZZZZ-ZZZZ',
    networkName: 'scaffolding-mc-ZZZZ-ZZZZ',
    networkSecret: 'ZZZZ-ZZZZ',
  },
]

describe('roomCode', () => {
  it.each(VECTORS)('解析 $code', (vector) => {
    const parsed = findRoomCode(vector.code)
    expect(parsed).toBeDefined()
    expect(parsed!.networkName).toBe(vector.networkName)
    expect(parsed!.networkSecret).toBe(vector.networkSecret)
  })

  it('生成的码符合格式并能还原出相同的网络参数', () => {
    for (let i = 0; i < 200; i++) {
      const created = createRoomCode()
      expect(created.code).toMatch(/^U\/[0-9A-HJ-NP-Z]{4}(-[0-9A-HJ-NP-Z]{4}){3}$/)

      const parsed = findRoomCode(created.code)
      expect(parsed).toBeDefined()
      expect(parsed!.networkName).toBe(created.networkName)
      expect(parsed!.networkSecret).toBe(created.networkSecret)
    }
  })

  it('大小写不敏感，I/O 按 1/0 处理', () => {
    const lower = findRoomCode('u/7qhp-0k00-0000-0000')
    expect(lower?.code).toBe('U/7QHP-0K00-0000-0000')

    // I 当 1、O 当 0：把向量里的 1/0 换成 I/O 仍应解析成功
    const withLetters = findRoomCode('U/7QHP-OKOO-OOOO-OOOO')
    expect(withLetters?.networkName).toBe('scaffolding-mc-7QHP-0K00')
  })

  it('能从整段聊天里提取', () => {
    const found = findRoomCode('来玩！邀请码 U/7QHP-0K00-0000-0000 复制即可')
    expect(found?.code).toBe('U/7QHP-0K00-0000-0000')
  })

  it('拒绝校验和不符的码', () => {
    // 把最后一位从 0 改成 1，数值不再是 7 的倍数
    expect(findRoomCode('U/7QHP-0K00-0000-0001')).toBeUndefined()
  })

  it('拒绝长度不对或前缀不对的输入', () => {
    expect(findRoomCode('')).toBeUndefined()
    expect(findRoomCode('U/7QHP-0K00-0000')).toBeUndefined()
    expect(findRoomCode('X/7QHP-0K00-0000-0000')).toBeUndefined()
    expect(parseRoomCode('随便一段话 U/7QHP-0K00-0000-0000')).toBeUndefined()
  })
})
