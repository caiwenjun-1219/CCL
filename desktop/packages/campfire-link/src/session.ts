/**
 * 联机会话编排（Host / Guest）。
 *
 * 房主：
 *   1. 起房间服务（本地 TCP），把 MC 的局域网端口登记进去
 *   2. 起 EasyTier，hostname 为 `scaffolding-mc-server-<房间服务端口>`，
 *      虚拟地址固定 10.144.144.1，白名单放行房间服务端口与游戏端口
 *   3. 生成邀请码
 *
 * 客方：
 *   1. 从邀请码解析 network_name / network_secret，起 EasyTier（DHCP）
 *   2. 端口转发 本地 → 10.144.144.1:13448 访问房间服务
 *   3. 问出房主的游戏端口，再转发一个本地端口到 10.144.144.1:<游戏端口>
 *
 * 协议与拓扑对齐 Campfire（GPL-3.0）：`LinkSession.cs`、`RoomServer.cs`。
 */
import { createServer, type Server, type Socket } from 'node:net'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { isLegacyWindows, renderEasyTierConfig } from './config'
import {
  ensureEasyTier,
  findFreePort,
  startEasyTier,
  type EasyTierInstance,
  type EasyTierInstall,
} from './easytier'
import { DEFAULT_NODE_IDS, resolveNodeUrls } from './nodes'
import {
  DEFAULT_PUNCH_TIMEOUT_MS,
  isDirectPeer,
  waitForRpc,
  watchLinkMode,
  type LinkMode,
} from './punch'
import {
  HOST_NAME_PREFIX,
  HOST_VIRTUAL_ADDRESS,
  PREFERRED_ROOM_PORT,
  RoomClient,
  RoomMethod,
  RoomStatus,
} from './protocol'
import { createRoomCode, findRoomCode, type RoomCodeInfo } from './roomCode'

export type LinkRole = 'host' | 'guest'
export type LinkStage = 'idle' | 'preparing' | 'starting' | 'waiting' | 'ready' | 'error'

export interface LinkState {
  role: LinkRole
  stage: LinkStage
  /** 邀请码（房主必填，客方为输入的那个） */
  code?: string
  /** 房主：房间服务端口；客方：远端房间服务端口 */
  roomPort?: number
  /** 房主：MC 局域网端口；客方：从房主问到的端口 */
  gamePort?: number
  /** 客方：本地可直接连接 127.0.0.1:<localPort> */
  localPort?: number
  /** 客方：房主虚拟地址 */
  hostAddress?: string
  /** 链路形态：正在打洞 / 直连 / 中继 */
  mode?: LinkMode
  error?: string
  /** 最近一条进度说明 */
  message?: string
}

export interface LinkSessionOptions {
  /** EasyTier 安装/缓存目录 */
  baseDir: string
  /** 启用的公共节点 id */
  nodeIds?: readonly string[]
  /** 追加的自定义节点 URI */
  customNodes?: readonly string[]
  /** 只走直连，不用中继 */
  directOnly?: boolean
  /** 隐藏本机地址，全部走中继 */
  hideAddress?: boolean
  /**
   * 打洞优先（默认开）：先只开 P2P 等直连，超时才退回公共节点中继。
   * 关掉就直接允许中继，省掉打洞窗口那十几秒。
   */
  punchFirst?: boolean
  /** 打洞窗口（毫秒），默认 12 秒 */
  punchTimeoutMs?: number
  onState?: (state: LinkState) => void
  onLog?: (line: string) => void
}

function withState(options: LinkSessionOptions) {
  let state: LinkState
  const emit = (patch: Partial<LinkState>) => {
    state = { ...state, ...patch }
    options.onState?.(state)
    return state
  }
  return { init: (role: LinkRole) => { state = { role, stage: 'idle' } }, emit, get: () => state }
}

/** 极简房间服务：只实现客方需要的 ping / server_port。 */
class MiniRoomServer {
  private server?: Server
  port = 0

  constructor(private readonly gamePort: number) {}

  listen(): Promise<number> {
    return new Promise((resolve, reject) => {
      const server = createServer((socket) => this.handle(socket))
      server.once('error', reject)
      server.listen(PREFERRED_ROOM_PORT, '127.0.0.1', () => {
        const address = server.address()
        this.port = typeof address === 'object' && address ? address.port : PREFERRED_ROOM_PORT
        this.server = server
        resolve(this.port)
      })
    })
  }

  private handle(socket: Socket) {
    let buffer = Buffer.alloc(0)
    const onData = (chunk: Buffer) => {
      buffer = Buffer.concat([buffer, chunk])
      if (buffer.length < 1) return
      const kindLength = buffer[0]
      if (buffer.length < 1 + kindLength + 4) return
      const kind = buffer.subarray(1, 1 + kindLength).toString('utf8')
      const bodyLength = buffer.readUInt32BE(1 + kindLength)
      if (buffer.length < 5 + kindLength + bodyLength) return
      const body = buffer.subarray(5 + kindLength, 5 + kindLength + bodyLength)
      buffer = Buffer.alloc(0)
      this.respond(socket, kind, body)
    }
    socket.on('data', onData)
    socket.on('error', () => socket.destroy())
  }

  private respond(socket: Socket, kind: string, body: Buffer) {
    let status: number = RoomStatus.ok
    let payload: Buffer = Buffer.alloc(0)
    switch (kind) {
      case RoomMethod.ping:
        payload = body
        break
      case RoomMethod.protocols:
        payload = Buffer.from(
          [RoomMethod.ping, RoomMethod.protocols, RoomMethod.serverPort].join('\u0000'),
          'utf8',
        )
        break
      case RoomMethod.serverPort: {
        payload = Buffer.allocUnsafe(2)
        payload.writeUInt16BE(this.gamePort)
        break
      }
      default:
        status = RoomStatus.error
        break
    }
    const frame = Buffer.allocUnsafe(5 + payload.length)
    frame[0] = status
    frame.writeUInt32BE(payload.length, 1)
    payload.copy(frame, 5)
    socket.end(frame)
  }

  async close(): Promise<void> {
    const server = this.server
    this.server = undefined
    if (!server) return
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
}

/** 房主会话。 */
export class HostSession {
  private install?: EasyTierInstall
  private network?: EasyTierInstance
  private server?: MiniRoomServer
  private configPath?: string
  private readonly holder: ReturnType<typeof withState>

  constructor(
    private readonly options: LinkSessionOptions & {
      /** MC 局域网端口；不传则用 25565 */
      gamePort?: number
      /** 复用旧邀请码 */
      code?: string
    },
  ) {
    this.holder = withState(options)
  }

  get state(): LinkState {
    return this.holder.get()
  }

  async start(): Promise<LinkState> {
    const emit = this.holder.emit
    this.holder.init('host')
    emit({ stage: 'preparing', message: '准备联机组件' })

    try {
      const baseDir = join(this.options.baseDir, 'EasyTier')
      mkdirSync(baseDir, { recursive: true })
      this.install = await ensureEasyTier(baseDir, (message) => emit({ message }))

      const gamePort = this.options.gamePort ?? 25565
      emit({ stage: 'starting', message: '启动房间服务', gamePort })

      this.server = new MiniRoomServer(gamePort)
      const roomPort = await this.server.listen()

      const parsed = this.options.code ? findRoomCode(this.options.code) : undefined
      const info: RoomCodeInfo = parsed ?? createRoomCode()

      const configPath = join(baseDir, `ccl-host-${Date.now()}.toml`)
      this.configPath = configPath
      writeFileSync(
        configPath,
        renderEasyTierConfig({
          networkName: info.networkName,
          networkSecret: info.networkSecret,
          hostname: `${HOST_NAME_PREFIX}${roomPort}`,
          ipv4: HOST_VIRTUAL_ADDRESS,
          peers: resolveNodeUrls(this.options.nodeIds ?? DEFAULT_NODE_IDS, this.options.customNodes),
          tcpWhitelist: [roomPort, gamePort],
          directOnly: this.options.directOnly,
          hideAddress: this.options.hideAddress,
          legacyWindows: isLegacyWindows(),
        }),
        'utf8',
      )

      const rpcPort = await findFreePort()
      emit({ stage: 'waiting', message: '建立虚拟网络', code: info.code, roomPort })
      this.network = startEasyTier({
        install: this.install,
        configPath,
        rpcPort,
        onLog: this.options.onLog,
      })

      return emit({ stage: 'ready', message: '房间已创建', code: info.code, roomPort, gamePort })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      await this.stop()
      return emit({ stage: 'error', error: message, message })
    }
  }

  async stop(): Promise<void> {
    await this.network?.stop().catch(() => undefined)
    this.network = undefined
    await this.server?.close().catch(() => undefined)
    this.server = undefined
    if (this.configPath) {
      try {
        rmSync(this.configPath, { force: true })
      } catch {
        // 忽略
      }
      this.configPath = undefined
    }
    this.holder.emit({ stage: 'idle', message: '已停止' })
  }
}

/** 客方会话。 */
export class GuestSession {
  private install?: EasyTierInstall
  private network?: EasyTierInstance
  private configPath?: string
  private stopWatch?: () => void
  private readonly holder: ReturnType<typeof withState>

  constructor(
    private readonly options: LinkSessionOptions & {
      code: string
      /** 等待房主出现的上限（毫秒） */
      timeoutMs?: number
    },
  ) {
    this.holder = withState(options)
  }

  get state(): LinkState {
    return this.holder.get()
  }

  async start(): Promise<LinkState> {
    const emit = this.holder.emit
    this.holder.init('guest')

    const info = findRoomCode(this.options.code)
    if (!info) {
      return emit({ stage: 'error', error: '邀请码无效', message: '邀请码无效' })
    }
    emit({ stage: 'preparing', code: info.code, message: '准备联机组件' })

    try {
      const baseDir = join(this.options.baseDir, 'EasyTier')
      mkdirSync(baseDir, { recursive: true })
      this.install = await ensureEasyTier(baseDir, (message) => emit({ message }))

      const deadline = Date.now() + (this.options.timeoutMs ?? 60_000)
      // 打洞优先：先只开 P2P 等直连。directOnly 时本来就不许回退中继。
      const punchFirst = this.options.punchFirst !== false
      const punchTimeout = this.options.punchTimeoutMs ?? DEFAULT_PUNCH_TIMEOUT_MS
      let mode: LinkMode = punchFirst ? 'punching' : 'relay'
      let gamePort: number | undefined

      await this.startNetwork(info, punchFirst)
      // no_tun 模式没有 TUN，OS 里根本没有 10.144.144.0/24 的路由 ——
      // 想访问房主的房间服务，必须先在本机开一个到虚拟地址的端口转发。
      let roomTarget = await this.openRoomForward()

      if (punchFirst) {
        emit({ stage: 'waiting', mode, message: '正在 P2P 打洞（优先直连）' })
        const punchDeadline = Math.min(deadline, Date.now() + punchTimeout)
        const punched = await this.waitForHost(roomTarget, punchDeadline, 500)
        if (punched) {
          gamePort = punched.gamePort
          // 连上了不等于打洞成功：可能现在走的是中继，按 peer 表判定。
          const direct = this.network ? await isDirectPeer(this.network, HOST_VIRTUAL_ADDRESS) : false
          mode = direct ? 'direct' : 'relay'
          if (!direct) {
            emit({ mode, message: '直连未建立，已用公共节点中继（后台继续尝试打洞）' })
          }
        } else if (this.options.directOnly) {
          throw new Error('打洞失败（已选择只走直连）')
        } else {
          // 打洞窗口内连不上 → 关掉 p2p_only 重开一个实例，让公共节点兜底中转。
          emit({ mode: 'relay', message: '打洞未成功，改用公共节点中继' })
          await this.startNetwork(info, false)
          roomTarget = await this.openRoomForward()
          emit({ stage: 'waiting', mode, message: '经公共节点中继连接' })
          const relayed = await this.waitForHost(
            roomTarget,
            Math.max(deadline, Date.now() + 10_000),
            1_000,
          )
          if (!relayed) throw new Error('等待房主超时（打洞与中继都没连上）')
          gamePort = relayed.gamePort
        }
      } else {
        emit({ stage: 'waiting', mode, message: '等待房主上线' })
        const relayed = await this.waitForHost(
          roomTarget,
          Math.max(deadline, Date.now() + 10_000),
          1_000,
        )
        if (!relayed) throw new Error('等待房主超时')
        gamePort = relayed.gamePort
      }

      if (!this.network) throw new Error('EasyTier 实例已退出')
      const network = this.network

      emit({ stage: 'waiting', mode, message: '建立端口转发', gamePort, hostAddress: HOST_VIRTUAL_ADDRESS })
      const localPort = await findFreePort()
      await network.addPortForward(
        'tcp',
        `127.0.0.1:${localPort}`,
        `${HOST_VIRTUAL_ADDRESS}:${gamePort}`,
      )

      // 中继兜底连上之后，EasyTier 仍会继续尝试把链路升级成直连，翻回来要能看见。
      this.stopWatch = watchLinkMode(network, HOST_VIRTUAL_ADDRESS, (next) => {
        if (next === mode) return
        mode = next
        emit({
          mode,
          message: next === 'direct'
            ? `已升级为直连（P2P 打洞成功），127.0.0.1:${localPort}`
            : `链路切回中继，127.0.0.1:${localPort}`,
        })
      })

      return emit({
        stage: 'ready',
        mode,
        message: (mode === 'direct'
          ? 'P2P 直连成功，'
          : '已连接（经公共节点中继），') + `进入游戏后连接 127.0.0.1:${localPort}`,
        gamePort,
        localPort,
        hostAddress: HOST_VIRTUAL_ADDRESS,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      await this.stop()
      return emit({ stage: 'error', error: message, message })
    }
  }

  /** 写配置 + 起 EasyTier。`p2pOnly` 为 true 时不允许走中继（打洞阶段）。 */
  private async startNetwork(info: RoomCodeInfo, p2pOnly: boolean): Promise<void> {
    if (!this.install) throw new Error('EasyTier 未就绪')
    await this.stopNetwork()

    const baseDir = join(this.options.baseDir, 'EasyTier')
    const configPath = join(baseDir, `ccl-guest-${Date.now()}.toml`)
    writeFileSync(
      configPath,
      renderEasyTierConfig({
        networkName: info.networkName,
        networkSecret: info.networkSecret,
        hostname: `ccl-guest-${Math.random().toString(36).slice(2, 8)}`,
        peers: resolveNodeUrls(this.options.nodeIds ?? DEFAULT_NODE_IDS, this.options.customNodes),
        directOnly: this.options.directOnly,
        p2pOnly,
        hideAddress: this.options.hideAddress,
        legacyWindows: isLegacyWindows(),
      }),
      'utf8',
    )
    this.configPath = configPath

    const rpcPort = await findFreePort()
    this.holder.emit({ stage: 'starting', message: p2pOnly ? '加入虚拟网络（P2P）' : '加入虚拟网络' })
    this.network = startEasyTier({
      install: this.install,
      configPath,
      rpcPort,
      onLog: this.options.onLog,
    })
  }

  private async stopNetwork(): Promise<void> {
    this.stopWatch?.()
    this.stopWatch = undefined
    await this.network?.stop().catch(() => undefined)
    this.network = undefined
    if (this.configPath) {
      try {
        rmSync(this.configPath, { force: true })
      } catch {
        // 忽略
      }
      this.configPath = undefined
    }
  }

  /**
   * 在 127.0.0.1 上开一个到房主房间服务的转发，返回本地端口。
   *
   * no_tun 模式下 OS 没有虚拟网段的路由，直连 `10.144.144.1:13448` 是走不通的，
   * 必须借 EasyTier 自己的 port-forward 代理进去。
   */
  private async openRoomForward(): Promise<number> {
    const network = this.network
    if (!network) throw new Error('EasyTier 未启动')
    // RPC 没起来时 addPortForward 的重试次数不够，先等它可用。
    await waitForRpc(network)
    const port = await findFreePort()
    await network.addPortForward(
      'tcp',
      `127.0.0.1:${port}`,
      `${HOST_VIRTUAL_ADDRESS}:${PREFERRED_ROOM_PORT}`,
    )
    return port
  }

  /** 轮询（经本机转发端口的）房主房间服务，超时返回 undefined。 */
  private async waitForHost(
    targetPort: number,
    deadline: number,
    pollMs: number,
  ): Promise<{ gamePort: number } | undefined> {
    while (Date.now() < deadline) {
      try {
        // 房间服务是一问一答的短连接，ping 与查询各开一条。
        const pingClient = await RoomClient.connect(targetPort, '127.0.0.1', 3000)
        let ok = false
        try {
          ok = await pingClient.ping()
        } finally {
          pingClient.close()
        }
        if (!ok) throw new Error('对端不是房间服务')

        const portClient = await RoomClient.connect(targetPort, '127.0.0.1', 3000)
        try {
          return { gamePort: await portClient.getServerPort() }
        } finally {
          portClient.close()
        }
      } catch {
        if (this.network && !this.network.alive) return undefined
        const sleep = Math.min(pollMs, Math.max(0, deadline - Date.now()))
        if (sleep <= 0) break
        await new Promise((resolve) => setTimeout(resolve, sleep))
      }
    }
    return undefined
  }

  /** 打洞阶段的额外信息：房主是否已经直连（供上层展示，不做额外阻塞）。 */
  async isDirect(): Promise<boolean> {
    if (!this.network) return false
    return isDirectPeer(this.network, HOST_VIRTUAL_ADDRESS)
  }

  async stop(): Promise<void> {
    await this.stopNetwork()
    this.holder.emit({ stage: 'idle', message: '已停止' })
  }
}

export { probeRoom, ROOM_FINGERPRINT } from './protocol'
