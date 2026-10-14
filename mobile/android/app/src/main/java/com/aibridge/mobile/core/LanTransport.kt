package com.aibridge.mobile.core

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import com.aibridge.mobile.ffi.PeerInfo
import com.aibridge.mobile.ffi.Transport
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap

private const val SERVICE_TYPE = "_aibridge._tcp"
private const val SERVICE_NAME = "aibridge-android"
private const val ATTR_DEVICE_KIND = "device_kind"
private const val ATTR_FINGERPRINT = "fingerprint"
private const val FRAME_MAX_BYTES = 4 * 1024 * 1024
private const val CONNECT_TIMEOUT_MILLIS = 10_000

/**
 * Transport trait 的宿主实现：仅局域网直传（E1/E2 会话扩散、A8 离线包搬运的传输通道）。
 *
 * 边界（docs/安全模型.md「跨设备」）：仅局域网 —— 不出网、不打洞、无中继、无云；
 * core 不持有任何网络句柄，载荷（bytes）语义由 core 解释，本类只做搬运。
 * 骨架约定（待与桌面端对齐 · 未验证，与 iOS LanTransport 同口径）：
 *   - NSD / mDNS 服务类型 `_aibridge._tcp`；
 *   - TXT 属性携带 `device_kind`（Windows / iOS / Android）与 `fingerprint`（配对公钥指纹，
 *     本机指纹由 core 派生后经设置注入的形态待确认，见 README 待确认清单）；
 *   - 线帧 = 4 字节大端长度前缀 + 载荷。
 * 实例由组合根构造并显式传递，不是单例。
 */
class LanTransport(context: Context) : Transport {

    private val nsdManager = context.applicationContext.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val peersByName = ConcurrentHashMap<String, PeerInfo>()
    private val endpointsByName = ConcurrentHashMap<String, InetSocketAddress>()
    private val inbound = ArrayDeque<ByteArray>()

    @Volatile
    private var serverSocket: ServerSocket? = null

    init {
        startListener()
        startDiscovery()
    }

    /** 局域网在线设备（PeerInfo：peer_id / device_kind / fingerprint，取自 NSD 服务与 TXT）。 */
    override fun peers(): List<PeerInfo> =
        peersByName.values.sortedBy { it.peerId }

    /** 向指定设备直发载荷（失败抛错；重试语义在 core）。 */
    override fun send(peerId: String, payload: ByteArray) {
        val endpoint = endpointsByName[peerId] ?: throw IOException("局域网内未发现设备：$peerId")
        Socket().use { socket ->
            socket.connect(endpoint, CONNECT_TIMEOUT_MILLIS)
            val out = DataOutputStream(socket.getOutputStream())
            out.writeInt(payload.size) // 4 字节大端长度前缀
            out.write(payload)
            out.flush()
        }
    }

    /** 收取对端载荷（超时返回 null；超时毫秒由 core 传入）。 */
    override fun receive(timeoutMillis: UInt): ByteArray? {
        val deadline = System.currentTimeMillis() + timeoutMillis.toLong()
        while (true) {
            synchronized(inbound) {
                if (inbound.isNotEmpty()) return inbound.removeFirst()
            }
            if (System.currentTimeMillis() >= deadline) return null
            Thread.sleep(50)
        }
    }

    // ---- 发现与监听 ----

    private fun startListener() {
        val server = try {
            ServerSocket(0)
        } catch (_: Exception) {
            return // 监听不可用时仍保留发送与发现能力
        }
        serverSocket = server
        val serviceInfo = NsdServiceInfo().apply {
            serviceName = SERVICE_NAME
            serviceType = SERVICE_TYPE
            port = server.localPort
            setAttribute(ATTR_DEVICE_KIND, "android")
            // 待确认：fingerprint TXT 的携带形态待 core / 桌面端线协议对齐（同 iOS 未验证项）。
        }
        try {
            nsdManager.registerService(serviceInfo, NsdManager.PROTOCOL_DNS_SD, registrationListener)
        } catch (_: Exception) {
            // 注册失败（如服务名冲突）不影响发送侧
        }
        Thread({ acceptLoop(server) }, "aibridge-lan-accept").start()
    }

    private fun acceptLoop(server: ServerSocket) {
        while (!server.isClosed) {
            val socket = try {
                server.accept()
            } catch (_: Exception) {
                return
            }
            Thread({ handleConnection(socket) }, "aibridge-lan-connection").start()
        }
    }

    /** 按 4 字节大端长度前缀重组载荷；完整载荷入 inbound 队列。 */
    private fun handleConnection(socket: Socket) {
        try {
            socket.use {
                val input = DataInputStream(it.getInputStream())
                while (true) {
                    val length = input.readInt()
                    if (length < 0 || length > FRAME_MAX_BYTES) return
                    val payload = ByteArray(length)
                    input.readFully(payload)
                    synchronized(inbound) { inbound.addLast(payload) }
                }
            }
        } catch (_: Exception) {
            // 对端断开 / 帧异常：丢弃该连接（重试语义在 core）
        }
    }

    private fun startDiscovery() {
        try {
            nsdManager.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discoveryListener)
        } catch (_: Exception) {
            // 发现不可用时仍保留发送 / 监听能力
        }
    }

    private val registrationListener = object : NsdManager.RegistrationListener {
        override fun onRegistrationFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit
        override fun onServiceRegistered(serviceInfo: NsdServiceInfo) = Unit
        override fun onServiceUnregistered(serviceInfo: NsdServiceInfo) = Unit
        override fun onServiceUnregistrationFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit
    }

    private val discoveryListener = object : NsdManager.DiscoveryListener {
        override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
        override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
        override fun onDiscoveryStarted(serviceType: String) = Unit
        override fun onDiscoveryStopped(serviceType: String) = Unit
        override fun onServiceLost(serviceInfo: NsdServiceInfo) {
            val name = serviceInfo.serviceName ?: return
            peersByName.remove(name)
            endpointsByName.remove(name)
        }

        override fun onServiceFound(serviceInfo: NsdServiceInfo) {
            if (serviceInfo.serviceType != SERVICE_TYPE) return
            try {
                nsdManager.resolveService(serviceInfo, resolveListener)
            } catch (_: Exception) {
                // 解析失败：下一次发现事件重试
            }
        }
    }

    private val resolveListener = object : NsdManager.ResolveListener {
        override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) = Unit

        override fun onServiceResolved(resolved: NsdServiceInfo) {
            val name = resolved.serviceName ?: return
            val attributes = resolved.attributes ?: emptyMap()
            val deviceKind = attributes[ATTR_DEVICE_KIND]?.let { String(it) } ?: "unknown"
            val fingerprint = attributes[ATTR_FINGERPRINT]?.let { String(it) } ?: ""
            peersByName[name] = PeerInfo(peerId = name, deviceKind = deviceKind, fingerprint = fingerprint)
            val host = resolved.host ?: return
            endpointsByName[name] = InetSocketAddress(host, resolved.port)
        }
    }
}
