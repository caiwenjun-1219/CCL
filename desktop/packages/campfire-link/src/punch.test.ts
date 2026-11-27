import { describe, expect, it } from 'vitest'
import type { EasyTierInstance } from './easytier'
import { isDirectPeer } from './punch'

/** 样本取自 v2.6.4 的 `easytier-cli` 真实输出（客方视角）。 */
const PEER_RELAY = `| ipv4            | hostname                            | cost     | lat(ms) | loss | rx      | tx      | tunnel | NAT              | version                 |
|-----------------|-------------------------------------|----------|---------|------|---------|---------|--------|------------------|-------------------------|
| 10.144.144.2/24 | diag-guest                          | Local    | -       | -    | -       | -       | -      | SymmetricEasyInc | 2.6.4-8428a89d          |
|                 | PublicServer_Murphy-EasyTier        | p2p      | 100.76  | 0.0% | 2.39 kB | 1.62 kB | tcp    | Symmetric        | 2.4.5-4c4d172e          |
| 10.144.144.1/24 | diag-host                           | relay(2) | 501.00  | 0.0% | 0 B     | 0 B     |        | SymmetricEasyInc | 2.6.4-8428a89d          |`

const PEER_DIRECT = `| ipv4            | hostname                            | cost     | lat(ms) | loss | rx      | tx      | tunnel | NAT              | version                 |
|-----------------|-------------------------------------|----------|---------|------|---------|---------|--------|------------------|-------------------------|
| 10.144.144.2/24 | diag-guest                          | Local    | -       | -    | -       | -       | -      | SymmetricEasyInc | 2.6.4-8428a89d          |
| 10.144.144.1/24 | diag-host                           | p2p      | 3.20    | 0.0% | 1.20 kB | 0.90 kB | udp    | SymmetricEasyInc | 2.6.4-8428a89d          |`

const PEER_EMPTY = `| ipv4 | hostname   | cost  | lat(ms) | loss | rx | tx | tunnel | NAT     | version        |
|------|------------|-------|---------|------|----|----|--------|---------|----------------|
|      | diag-guest | Local | -       | -    | -  | -  | -      | Unknown | 2.6.4-8428a89d |`

const ROUTE_RELAY = `| ipv4            | hostname                            | proxy_cidrs | next_hop_ipv4 | next_hop_hostname            | next_hop_lat | path_len | path_latency |
|-----------------|-------------------------------------|-------------|---------------|------------------------------|--------------|----------|--------------|
| 10.144.144.2/24 | diag-guest                          |             | -             | Local                        | 0            | 0        | 0            |
| 10.144.144.1/24 | diag-host                           |             |               | PublicServer_Murphy-EasyTier | 95.785       | 2        | 1000         |`

const ROUTE_DIRECT = `| ipv4            | hostname   | proxy_cidrs | next_hop_ipv4 | next_hop_hostname | next_hop_lat | path_len | path_latency |
|-----------------|------------|-------------|---------------|-------------------|--------------|----------|--------------|
| 10.144.144.2/24 | diag-guest |             | -             | Local             | 0            | 0        | 0            |
| 10.144.144.1/24 | diag-host  |             | DIRECT        | DIRECT            | 3.2          | 1        | 3            |`

function fake(cli: Record<string, string>): EasyTierInstance {
  return {
    rpcPort: 0,
    alive: true,
    output: '',
    stop: async () => {},
    runCli: async (args: string[]) => {
      const out = cli[args[0]]
      if (out === undefined) throw new Error(`unexpected ${args.join(' ')}`)
      return out
    },
    addPortForward: async () => {},
    setUdpWhitelist: async () => {},
  } as unknown as EasyTierInstance
}

describe('isDirectPeer', () => {
  it('peer 表里 cost=p2p 即直连', async () => {
    await expect(isDirectPeer(fake({ peer: PEER_DIRECT }), '10.144.144.1')).resolves.toBe(true)
  })

  it('peer 表里 cost=relay(2) 是中继', async () => {
    await expect(isDirectPeer(fake({ peer: PEER_RELAY }), '10.144.144.1')).resolves.toBe(false)
  })

  it('peer 表还没有房主时回落到 route 表', async () => {
    await expect(
      isDirectPeer(fake({ peer: PEER_EMPTY, route: ROUTE_DIRECT }), '10.144.144.1'),
    ).resolves.toBe(true)
    await expect(
      isDirectPeer(fake({ peer: PEER_EMPTY, route: ROUTE_RELAY }), '10.144.144.1'),
    ).resolves.toBe(false)
  })

  it('CLI 全挂时不抛异常，按未直连处理', async () => {
    await expect(isDirectPeer(fake({}), '10.144.144.1')).resolves.toBe(false)
  })

  it('别人的地址不会误判', async () => {
    await expect(isDirectPeer(fake({ peer: PEER_DIRECT }), '10.144.144.9')).resolves.toBe(false)
  })
})
