/* eslint-disable no-template-curly-in-string */
import { config as dotenv } from 'dotenv'
import type { Configuration } from 'electron-builder'
import { resolve } from 'path'

dotenv()

const linuxPackageScripts = {
  afterInstall: 'build/linux/after-install.tpl',
  afterRemove: 'build/linux/after-remove.tpl',
  appArmorProfile: 'build/linux/apparmor-profile.tpl',
  fpm: [`--after-upgrade=${resolve(__dirname, 'linux', 'after-install.tpl')}`],
}

export const config = {
  productName: 'CCL',
  appId: 'ccl',
  directories: {
    output: 'build/output',
    buildResources: 'build',
    app: '.',
  },
  protocols: {
    name: 'CCL',
    schemes: ['xmcl'],
  },
  // assign publish for auto-updater
  // set this to your own repo!
  publish: [{
    provider: 'github',
    owner: 'voxelum',
    repo: 'x-minecraft-launcher',
  }],
  files: [{
    from: 'dist',
    to: '.',
    filter: ['**/*.js', '**/*.ico', '**/*.png', '**/*.webp', '**/*.svg', '*.node', '*.dll', '**/*.html', '**/*.css', '**/*.woff2', '**/*.wasm'],
  }, {
    from: '.',
    to: '.',
    filter: 'package.json',
  }],
  extraResources: [{
    from: 'main/agent-documents',
    to: 'agent-documents',
    filter: ['**/*.md'],
  }, {
    // 完整版 Campfire（GPL-3.0）随安装包分发，入口二直接拉起这份副本
    from: 'build/campfire',
    to: 'campfire',
    filter: ['**/*'],
  }],
  artifactName: 'ccl-${version}-${platform}-${arch}.${ext}',
  appx: {
    displayName: 'CCL',
    applicationId: 'ccl',
    identityName: 'ccl',
    backgroundColor: 'transparent',
    publisher: process.env.PUBLISHER,
    publisherDisplayName: 'CI010',
    setBuildNumber: true,
  },
  // Windows 安装包沿用 2.1.0 的命名（官网下载链接按这个名字写死）
  nsis: {
    artifactName: 'CCL-${version}-win-${arch}-setup.${ext}',
    oneClick: false,
    allowToChangeInstallationDirectory: true,
    perMachine: false,
  },
  dmg: {
    artifactName: 'ccl-${version}-${arch}.${ext}',
    contents: [
      {
        x: 410,
        y: 150,
        type: 'link',
        path: '/Applications',
      },
      {
        x: 130,
        y: 150,
        type: 'file',
      },
    ],
  },
  mac: {
    icon: 'icons/dark.icns',
    darkModeSupport: true,
    // CI 上没有 Apple 开发者证书：显式关掉签名，避免 electron-builder 去找证书后失败。
    // 产物是未签名的 dmg，用户首次打开需要右键 → 打开。
    identity: null,
    target: [
      {
        target: 'dmg',
        arch: ['arm64', 'x64'],
      },
    ],
    extendInfo: {
      NSLocalNetworkUsageDescription: 'XMCL and Minecraft need access to your local network to discover and connect to LAN games and servers.',
      NSMicrophoneUsageDescription: 'A Minecraft mod wants to access your microphone.',
      NSCameraUsageDescription: 'Please give us access to your camera',
      'com.apple.security.device.audio-input': true,
      'com.apple.security.device.camera': true,
    },
  },
  win: {
    signtoolOptions: {
      certificateFile: undefined as string | undefined,
      publisherName: 'CI010',
    },
    icon: 'icons/dark.ico',
    electronLanguages: ['en-US'],
    // 发安装包：CCL_NSIS=true RELEASE=true pnpm --prefix=xmcl-electron-app build
    target: process.env.CCL_NSIS
      ? [{ target: 'nsis', arch: ['x64'] }]
      : [
          {
            target: 'zip',
            arch: [
              'x64',
              'ia32',
            ],
          },
          'appx',
        ],
  },
  linux: {
    executableName: 'ccl',
    electronLanguages: ['en-US'],
    desktop: {
      entry: {
        MimeType: 'x-scheme-handler/xmcl',
        StartupWMClass: 'ccl',
      },
    },
    category: 'Game',
    // Linux 用 png：electron-builder 的 AppImage/deb 打包会拒绝 icns
    icon: 'icons/256x256.png',
    artifactName: 'ccl-${version}-${arch}.${ext}',
    // CCL_LINUX_MIN=1 只打 x64 tar.xz：Windows 上交叉打包只能出这个
    // （AppImage 需要 Linux 下的 mksquashfs，deb/rpm 需要 fpm）
    target: process.env.CCL_LINUX_MIN
      ? [{ target: 'tar.xz', arch: ['x64'] }]
      : [
          { target: 'deb', arch: ['x64', 'arm64'] },
          { target: 'rpm', arch: ['x64', 'arm64'] },
          { target: 'AppImage', arch: ['x64', 'arm64'] },
          { target: 'tar.xz', arch: ['x64', 'arm64'] },
          { target: 'pacman', arch: ['x64', 'arm64'] },
        ],
  },
  deb: linuxPackageScripts,
  rpm: linuxPackageScripts,
  pacman: linuxPackageScripts,
  snap: {
    publish: [
      'github',
    ],
  },
} satisfies Configuration
