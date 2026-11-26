/**
 * 房间服务的线协议（TCP，每个玩家一条连接）。
 *
 * 请求帧：kind 长度（1 字节）、kind（UTF-8，"namespace:path"）、body 长度（4 字节，大端）、body。
 * 响应帧：status（1 字节，0 成功）、body 长度（4 字节，大端）、body。
 *
 * 常量与帧格式取自 Campfire（GPL-3.0）的 `RoomProtocol.cs` / `RoomFraming.cs`。
 */
import { Socket } from 'node:net'

export const RoomMethod = {
  ping: 'c:ping',
  protocols: 'c:protocols',
  serverPort: 'c:server_port',
  playerPing: 'c:player_ping',
  playerList: 'c:player_profiles_list',
} as const

export const RoomStatus = {
  ok: 0,
  notReady: 32,
  error: 255,
} as const

/** 虚拟网络中房主的固定地址。 */
export const HOST_VIRTUAL_ADDRESS = '10.144.144.1'
/** 房主 hostname 前缀，后缀是房间服务端口。 */
export const HOST_NAME_PREFIX = 'scaffolding-mc-server-'
/** 房间服务偏好的端口。 */
export const PREFERRED_ROOM_PORT = 13448
/** 房间服务指纹，用于确认对端确实是房间服务。 */
export const ROOM_FINGERPRINT = Buffer.from([
  0x41, 0x57, 0x48, 0x44, 0x86, 0x37, 0x40, 0x59, 0x57, 0x44, 0x92, 0x43, 0x96, 0x99, 0x85, 0x01,
])

const MAX_BODY = 1024 * 1024

export interface RoomResponse {
  status: number
  body: Buffer
}

function encodeRequest(kind: string, body?: Buffer): Buffer {
  const kindBytes = Buffer.from(kind, 'utf8')
  if (kindBytes.length > 255) throw new Error('request kind too long')
  const payload = body ?? Buffer.alloc(0)
  const frame = Buffer.allocUnsafe(1 + kindBytes.length + 4 + payload.length)
  frame[0] = kindBytes.length
  kindBytes.copy(frame, 1)
  frame.writeUInt32BE(payload.length, 1 + kindBytes.length)
  payload.copy(frame, 5 + kindBytes.length)
  return frame
}

/** 读一整帧响应。把已到达的字节缓存起来，避免半包被丢弃。 */
function readResponse(socket: Socket, timeoutMs: number): Promise<RoomResponse> {
  return new Promise((resolve, reject) => {
    let buffered: Buffer = Buffer.alloc(0)
    const timer = setTimeout(() => {
      cleanup()
      reject(new Error('room response timed out'))
    }, timeoutMs)

    const onData = (chunk: Buffer) => {
      buffered = buffered.length === 0 ? chunk : Buffer.concat([buffered, chunk])
      if (buffered.length < 5) return
      const length = buffered.readUInt32BE(1)
      if (length > MAX_BODY) {
        cleanup()
        reject(new Error('room response too large'))
        return
      }
      if (buffered.length < 5 + length) return
      cleanup()
      resolve({ status: buffered[0], body: buffered.subarray(5, 5 + length) })
    }
    const onError = (error: Error) => {
      cleanup()
      reject(error)
    }
    const onClose = () => {
      cleanup()
      reject(new Error('room connection closed'))
    }
    const cleanup = () => {
      clearTimeout(timer)
      socket.off('data', onData)
      socket.off('error', onError)
      socket.off('close', onClose)
    }

    socket.on('data', onData)
    socket.on('error', onError)
    socket.on('close', onClose)
  })
}

/**
 * 房间服务客户端。每次请求新建一条连接（与 Campfire 一致：查询是短连接），
 * 因此不必处理跨帧粘包。
 */
export class RoomClient {
  private constructor(private readonly socket: Socket) {}

  static connect(port: number, host = '127.0.0.1', timeoutMs = 3000): Promise<RoomClient> {
    return new Promise((resolve, reject) => {
      const socket = new Socket()
      const timer = setTimeout(() => {
        socket.destroy()
        reject(new Error(`connect ${host}:${port} timed out`))
      }, timeoutMs)

      socket.once('connect', () => {
        clearTimeout(timer)
        socket.setNoDelay(true)
        resolve(new RoomClient(socket))
      })
      socket.once('error', (error) => {
        clearTimeout(timer)
        reject(error)
      })
      socket.connect(port, host)
    })
  }

  async request(kind: string, body?: Buffer, timeoutMs = 5000): Promise<RoomResponse> {
    const pending = readResponse(this.socket, timeoutMs)
    this.socket.write(encodeRequest(kind, body))
    return pending
  }

  /** 回显 body，用于确认对端是房间服务。 */
  async ping(): Promise<boolean> {
    const { status, body } = await this.request(RoomMethod.ping, ROOM_FINGERPRINT)
    return status === RoomStatus.ok && body.equals(ROOM_FINGERPRINT)
  }

  /** 房主当前的 MC 局域网端口。 */
  async getServerPort(): Promise<number> {
    const { status, body } = await this.request(RoomMethod.serverPort)
    if (status !== RoomStatus.ok || body.length < 2) {
      throw new Error(`room returned status ${status} for server port`)
    }
    return body.readUInt16BE(0)
  }

  close(): void {
    this.socket.destroy()
  }
}

/** 探测某个本地端口上是否已有可用的房间服务。 */
export async function probeRoom(port: number, host = '127.0.0.1'): Promise<boolean> {
  try {
    const client = await RoomClient.connect(port, host, 1500)
    try {
      return await client.ping()
    } finally {
      client.close()
    }
  } catch {
    return false
  }
}
